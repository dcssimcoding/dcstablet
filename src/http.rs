//! Bounded nonblocking HTTP ingress. Network I/O never occupies application workers.
//!
//! Each connection carries one request/response and then closes. The browser API
//! uses fixed-length JSON bodies; transfer encodings and upgrades are rejected.
use crate::{
    STOP,
    state::{Stage, State},
    workers,
};
use std::{
    io::{self, Read, Write},
    net::{IpAddr, SocketAddr, TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TryRecvError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const MAX_CONNECTIONS: usize = 32;
const LOCAL_RESERVE: usize = 4;
const MAX_PER_IP: usize = 8;
const REQUEST_QUEUE: usize = 16;
const MAX_HEADER: usize = 8192;
const MAX_BODY: usize = 65536;
const HEADER_TIMEOUT: Duration = Duration::from_secs(2);
const BODY_TIMEOUT: Duration = Duration::from_secs(3);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(2);
const HANDLER_TIMEOUT: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(5);

#[derive(Clone, Copy)]
pub enum Method {
    Get,
    Post,
    Other,
}
struct Head {
    method: Method,
    url: String,
    headers: Vec<(String, String)>,
    end: usize,
    length: usize,
}
pub struct Request {
    head: Head,
    body: Vec<u8>,
    remote: SocketAddr,
    response: SyncSender<Vec<u8>>,
}
impl Request {
    pub fn method(&self) -> &Method {
        &self.head.method
    }
    pub fn url(&self) -> &str {
        &self.head.url
    }
    pub fn header(&self, name: &str) -> Option<&str> {
        self.head
            .headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
    pub fn body(&self) -> &[u8] {
        &self.body
    }
    pub fn remote_addr(&self) -> Option<&SocketAddr> {
        Some(&self.remote)
    }
    pub fn respond(self, status: u16, kind: &str, body: Vec<u8>) {
        // The one-response channel is bounded; timed-out clients cannot block workers.
        let _ = self.response.try_send(response(status, kind, &body));
    }
}
fn response(status: u16, kind: &str, body: &[u8]) -> Vec<u8> {
    debug_assert!(!kind.contains(['\r', '\n']));
    let reason = match status {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        408 => "Request Timeout",
        409 => "Conflict",
        413 => "Content Too Large",
        417 => "Expectation Failed",
        429 => "Too Many Requests",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "Error",
    };
    let mut bytes=format!("HTTP/1.1 {status} {reason}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\n\r\n",body.len()).into_bytes();
    bytes.extend_from_slice(body);
    bytes
}
fn parse_head(bytes: &[u8]) -> Result<Option<Head>, u16> {
    let mut storage = [httparse::EMPTY_HEADER; 64];
    let mut parsed = httparse::Request::new(&mut storage);
    let end = match parsed.parse(bytes) {
        Ok(httparse::Status::Complete(end)) => end,
        Ok(httparse::Status::Partial) => {
            return if bytes.len() >= MAX_HEADER {
                Err(431)
            } else {
                Ok(None)
            };
        }
        Err(httparse::Error::TooManyHeaders) => return Err(431),
        Err(_) => return Err(400),
    };
    if end > MAX_HEADER {
        return Err(431);
    }
    if !matches!(parsed.version, Some(0 | 1)) {
        return Err(400);
    }
    let url = parsed.path.ok_or(400u16)?;
    if !url.starts_with('/') {
        return Err(400);
    }
    let method = match parsed.method {
        Some("GET") => Method::Get,
        Some("POST") => Method::Post,
        Some(_) => Method::Other,
        None => return Err(400),
    };
    let mut headers = Vec::new();
    let mut length = None;
    let mut hosts = 0;
    for header in parsed.headers.iter() {
        if !header
            .value
            .iter()
            .all(|b| *b == b'\t' || (32..=126).contains(b))
        {
            return Err(400);
        }
        let value = std::str::from_utf8(header.value)
            .map_err(|_| 400u16)?
            .trim();
        if header.name.eq_ignore_ascii_case("Transfer-Encoding")
            || header.name.eq_ignore_ascii_case("Upgrade")
        {
            return Err(400);
        }
        if header.name.eq_ignore_ascii_case("Expect") {
            return Err(417);
        }
        if header.name.eq_ignore_ascii_case("Content-Length") {
            if length.is_some() || value.is_empty() || !value.bytes().all(|c| c.is_ascii_digit()) {
                return Err(400);
            }
            length = Some(value.parse::<usize>().map_err(|_| 413u16)?);
        }
        if header.name.eq_ignore_ascii_case("Host") {
            hosts += 1;
        }
        if header.name.eq_ignore_ascii_case("Origin")
            && headers
                .iter()
                .any(|(name, _): &(String, String)| name.eq_ignore_ascii_case("Origin"))
        {
            return Err(400);
        }
        headers.push((header.name.to_owned(), value.to_owned()));
    }
    if hosts > 1 || (parsed.version == Some(1) && hosts != 1) {
        return Err(400);
    }
    let length = length.unwrap_or(0);
    let path = url.split('?').next().unwrap_or("");
    let limit = match (method, path) {
        (Method::Post, "/offer") => MAX_BODY,
        (Method::Post, "/controls") => 512,
        _ => 0,
    };
    if length > limit {
        return Err(413);
    }
    Ok(Some(Head {
        method,
        url: url.to_owned(),
        headers,
        end,
        length,
    }))
}
enum Phase {
    Reading {
        bytes: Vec<u8>,
        head: Option<Head>,
        deadline: Instant,
    },
    Waiting {
        response: Receiver<Vec<u8>>,
        deadline: Instant,
    },
    Writing {
        bytes: Vec<u8>,
        offset: usize,
        deadline: Instant,
    },
}
struct Connection {
    stream: TcpStream,
    remote: SocketAddr,
    phase: Phase,
}
impl Connection {
    fn error(&mut self, status: u16) {
        self.phase = Phase::Writing {
            bytes: response(status, "text/plain", b"Request rejected"),
            offset: 0,
            deadline: Instant::now() + RESPONSE_TIMEOUT,
        };
    }
    fn tick(&mut self, queue: &SyncSender<Request>) -> bool {
        match &mut self.phase {
            Phase::Reading {
                bytes,
                head,
                deadline,
            } => {
                if Instant::now() >= *deadline {
                    self.error(408);
                    return true;
                }
                // One bounded read per tick prevents one client monopolizing ingress.
                let mut chunk = [0u8; 4096];
                match self.stream.read(&mut chunk) {
                    Ok(0) => return false,
                    Ok(count) => bytes.extend_from_slice(&chunk[..count]),
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => return true,
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => return true,
                    Err(_) => return false,
                }
                if head.is_none() {
                    match parse_head(bytes) {
                        Ok(Some(parsed)) => {
                            *head = Some(parsed);
                            *deadline = Instant::now() + BODY_TIMEOUT;
                        }
                        Ok(None) => return true,
                        Err(status) => {
                            self.error(status);
                            return true;
                        }
                    }
                }
                let parsed = head.as_ref().unwrap();
                let total = parsed.end + parsed.length;
                if bytes.len() < total {
                    return true;
                }
                // Pipelined requests/extra bytes are never handed to another parser.
                if bytes.len() != total {
                    self.error(400);
                    return true;
                }
                let body = bytes[parsed.end..total].to_vec();
                let head = head.take().unwrap();
                let (response, receiver) = mpsc::sync_channel(1);
                let request = Request {
                    head,
                    body,
                    remote: self.remote,
                    response,
                };
                match queue.try_send(request) {
                    Ok(()) => {
                        self.phase = Phase::Waiting {
                            response: receiver,
                            deadline: Instant::now() + HANDLER_TIMEOUT,
                        }
                    }
                    Err(_) => self.error(503),
                }
                true
            }
            Phase::Waiting { response, deadline } => {
                if Instant::now() >= *deadline {
                    self.error(504);
                    return true;
                }
                match response.try_recv() {
                    Ok(bytes) => {
                        self.phase = Phase::Writing {
                            bytes,
                            offset: 0,
                            deadline: Instant::now() + RESPONSE_TIMEOUT,
                        }
                    }
                    Err(TryRecvError::Disconnected) => self.error(500),
                    Err(TryRecvError::Empty) => {}
                }
                true
            }
            Phase::Writing {
                bytes,
                offset,
                deadline,
            } => {
                if Instant::now() >= *deadline {
                    return false;
                }
                // The absolute write deadline is not extended by slow readers.
                match self.stream.write(&bytes[*offset..]) {
                    Ok(0) => false,
                    Ok(count) => {
                        *offset += count;
                        *offset < bytes.len()
                    }
                    Err(e)
                        if matches!(
                            e.kind(),
                            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                        ) =>
                    {
                        true
                    }
                    Err(_) => false,
                }
            }
        }
    }
}
fn admit(connections: &[Connection], remote: IpAddr) -> bool {
    connections.len() < MAX_CONNECTIONS
        && connections
            .iter()
            .filter(|c| c.remote.ip() == remote)
            .count()
            < MAX_PER_IP
        && (remote.is_loopback()
            || connections
                .iter()
                .filter(|c| !c.remote.ip().is_loopback())
                .count()
                < MAX_CONNECTIONS - LOCAL_RESERVE)
}
fn run(listener: TcpListener, queue: SyncSender<Request>, stop: &AtomicBool) -> io::Result<()> {
    let mut connections = Vec::<Connection>::new();
    while !stop.load(Ordering::Relaxed) && !STOP.load(Ordering::Relaxed) {
        // Limit accepts per tick too: a connection flood must not starve existing I/O.
        for _ in 0..MAX_CONNECTIONS {
            match listener.accept() {
                Ok((stream, remote)) => {
                    if !admit(&connections, remote.ip()) {
                        continue;
                    }
                    if stream.set_nonblocking(true).is_err() || stream.set_nodelay(true).is_err() {
                        continue;
                    }
                    connections.push(Connection {
                        stream,
                        remote,
                        phase: Phase::Reading {
                            bytes: Vec::with_capacity(4096),
                            head: None,
                            deadline: Instant::now() + HEADER_TIMEOUT,
                        },
                    });
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::Interrupted
                            | io::ErrorKind::ConnectionAborted
                            | io::ErrorKind::ConnectionReset
                    ) =>
                {
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
        let mut index = 0;
        while index < connections.len() {
            if connections[index].tick(&queue) {
                index += 1;
            } else {
                connections.swap_remove(index);
            }
        }
        thread::sleep(POLL);
    }
    // Flush already-computed responses (notably /shutdown) within a fixed budget.
    // Incomplete input is closed immediately; shutdown never waits for a client body.
    for connection in &mut connections {
        if let Phase::Waiting { response, .. } = &connection.phase
            && let Ok(bytes) = response.try_recv()
        {
            connection.phase = Phase::Writing {
                bytes,
                offset: 0,
                deadline: Instant::now() + RESPONSE_TIMEOUT,
            };
        }
    }
    connections.retain(|c| matches!(c.phase, Phase::Writing { .. }));
    let deadline = Instant::now() + Duration::from_millis(100);
    while !connections.is_empty() && Instant::now() < deadline {
        connections.retain_mut(|c| c.tick(&queue));
        thread::sleep(POLL);
    }
    Ok(())
}
pub struct Server {
    requests: Mutex<Receiver<Request>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
impl Server {
    pub fn bind(port: u16, state: Arc<State>) -> io::Result<Self> {
        let listener = TcpListener::bind(("0.0.0.0", port))?;
        listener.set_nonblocking(true)?;
        let (queue, requests) = mpsc::sync_channel(REQUEST_QUEUE);
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let worker = thread::Builder::new()
            .name("tablet-http-io".into())
            .spawn(move || {
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run(listener, queue, &thread_stop)
                })) {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => {
                        state.fail(Stage::Http, error.to_string());
                        STOP.store(true, Ordering::Relaxed);
                    }
                    Err(panic) => {
                        state.fail(Stage::Http, workers::panic_text(panic));
                        STOP.store(true, Ordering::Relaxed);
                    }
                }
            })?;
        Ok(Self {
            requests: Mutex::new(requests),
            stop,
            worker: Some(worker),
        })
    }
    pub fn recv_timeout(&self, timeout: Duration) -> Result<Request, RecvTimeoutError> {
        self.requests
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .recv_timeout(timeout)
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_ambiguous_framing_and_oversized_bodies() {
        for bytes in [b"POST /offer HTTP/1.1\r\nHost: local\r\nContent-Length: 1\r\nContent-Length: 1\r\n\r\n".as_slice(),b"POST /offer HTTP/1.1\r\nHost: local\r\nTransfer-Encoding: chunked\r\nContent-Length: 2\r\n\r\n",b"POST /offer HTTP/1.1\r\nHost: local\r\nContent-Length: -1\r\n\r\n"] {assert!(matches!(parse_head(bytes),Err(400)));}
        assert!(matches!(
            parse_head(b"POST /offer HTTP/1.1\r\nHost: local\r\nContent-Length: 65537\r\n\r\n"),
            Err(413)
        ));
        assert!(matches!(
            parse_head(b"POST /controls HTTP/1.1\r\nHost: local\r\nContent-Length: 513\r\n\r\n"),
            Err(413)
        ));
    }
    #[test]
    fn accepts_browser_and_supervisor_requests() {
        let parsed =
            parse_head(b"POST /offer HTTP/1.1\r\nHost: local\r\nContent-Length: 2\r\n\r\n{}")
                .unwrap()
                .unwrap();
        assert_eq!(parsed.length, 2);
        assert!(
            parse_head(b"GET /health HTTP/1.0\r\nHost: local\r\n\r\n")
                .unwrap()
                .is_some()
        );
        assert!(parse_head(b"GET / HTTP/1.1\r\nHost: lo").unwrap().is_none());
        assert!(matches!(
            parse_head(&vec![b'A'; MAX_HEADER]),
            Err(431) | Err(400)
        ));
    }
}
