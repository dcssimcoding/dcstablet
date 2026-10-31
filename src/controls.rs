//! Bounded, ordered button snapshots. UDP stop-and-wait makes press/release loss recoverable.
//! One worker owns the loopback socket. Browser revisions order submissions within
//! a viewer session; worker epoch/serial order UDP retries independently. New epochs
//! begin with released masks. A 700-ms freshness limit complements Lua's 750-ms
//! release watchdog and two-second hold cap; see docs/control-protocol.md.
use crate::{STOP, state::lock};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    net::{IpAddr, UdpSocket},
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
const MASK: u64 = (1 << 20) - 1;
const FRESH: Duration = Duration::from_millis(700);
#[derive(Clone)]
struct Update {
    masks: [u32; 2],
    at: Instant,
}
struct Inner {
    session: u64,
    owner: Option<IpAddr>,
    revision: u64,
    pending: VecDeque<Update>,
    last_input: Instant,
    acknowledged: Option<Instant>,
    ready: bool,
}
pub struct Controls {
    inner: Mutex<Inner>,
    enabled: bool,
}
impl Controls {
    pub fn new(enabled: bool) -> Arc<Self> {
        Arc::new(Self {
            enabled,
            inner: Mutex::new(Inner {
                session: 0,
                owner: None,
                revision: 0,
                pending: VecDeque::new(),
                last_input: Instant::now(),
                acknowledged: None,
                ready: false,
            }),
        })
    }
    pub fn begin(&self, owner: IpAddr) {
        let mut s = lock(&self.inner);
        s.session += 1;
        s.owner = Some(owner);
        s.revision = 0;
        s.pending.clear();
        s.last_input = Instant::now();
    }
    pub fn end(&self) {
        let mut s = lock(&self.inner);
        s.owner = None;
        s.pending.clear();
    }
    pub fn status(&self) -> Value {
        let s = lock(&self.inner);
        let bridge_connected = s.acknowledged.is_some_and(|t| t.elapsed() < FRESH);
        json!({"enabled":self.enabled,"ready":self.enabled && s.ready && bridge_connected,
            "bridge_connected":self.enabled && bridge_connected,"ack_age_ms":s.acknowledged.map(|t| t.elapsed().as_millis()),
            "session":s.session,"queued":s.pending.len(),"max_hold_ms":2000})
    }
    pub fn submit(&self, value: &Value, remote: IpAddr) -> Result<(), (u16, &'static str)> {
        let number = |name| {
            value[name]
                .as_u64()
                .ok_or((400, "Expected integer button snapshot fields"))
        };
        let session = number("session")?;
        let revision = number("revision")?;
        let left = number("left")?;
        let right = number("right")?;
        if left > MASK || right > MASK || revision == 0 {
            return Err((400, "Invalid button mask or revision"));
        }
        let mut s = lock(&self.inner);
        if !self.enabled {
            return Err((409, "Controls are disabled in demo mode"));
        }
        if s.owner != Some(remote) || session != s.session {
            return Err((409, "The control session changed; reconnect"));
        }
        if revision <= s.revision {
            return Err((409, "Stale button snapshot"));
        }
        if !s.ready || !s.acknowledged.is_some_and(|t| t.elapsed() < FRESH) {
            return Err((503, "DCS Hornet controls are unavailable"));
        }
        if s.pending.len() >= 64 {
            return Err((429, "Button queue is full; release and retry"));
        }
        let now = Instant::now();
        s.pending.push_back(Update {
            masks: [left as u32, right as u32],
            at: now,
        });
        s.revision = revision;
        s.last_input = now;
        Ok(())
    }
    pub fn spawn(self: &Arc<Self>) -> std::io::Result<JoinHandle<()>> {
        let this = self.clone();
        let socket = UdpSocket::bind("127.0.0.1:0")?;
        socket.connect("127.0.0.1:17881")?;
        socket.set_nonblocking(true)?;
        thread::Builder::new()
            .name("ddi-controls".into())
            .spawn(move || this.run(socket, &STOP))
    }
    fn run(&self, socket: UdpSocket, stop: &std::sync::atomic::AtomicBool) {
        let epoch = format!(
            "{}{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        );
        let mut serial = 1u64;
        let mut session = 0;
        let mut masks = [0u32; 2];
        let mut acknowledged = true;
        let mut last_input = Instant::now();
        let mut buffer = [0u8; 128];
        while !stop.load(std::sync::atomic::Ordering::Relaxed) {
            let idle;
            {
                let mut s = lock(&self.inner);
                idle = s.owner.is_none();
                if session != s.session || s.owner.is_none() || last_input.elapsed() > FRESH {
                    if masks != [0, 0] || session != s.session {
                        masks = [0, 0];
                        serial += 1;
                        acknowledged = false;
                    }
                    session = s.session;
                    if s.owner.is_none() || s.last_input.elapsed() > FRESH {
                        s.pending.clear();
                    }
                }
                if acknowledged && let Some(update) = s.pending.pop_front() {
                    masks = if update.at.elapsed() < FRESH {
                        update.masks
                    } else {
                        [0, 0]
                    };
                    last_input = update.at;
                    serial += 1;
                    acknowledged = false;
                }
            }
            let ping = idle && acknowledged && masks == [0, 0];
            let packet = if ping {
                "HDDI1 PING".to_owned()
            } else {
                format!("HDDI1 {epoch} {serial} {} {}", masks[0], masks[1])
            };
            let _ = socket.send(packet.as_bytes());
            let prefix = if ping {
                "HDDI1 PONG ".to_owned()
            } else {
                format!("HDDI1 {epoch} {serial} ")
            };
            // Retries can produce a burst of replies when DCS resumes. Reading
            // only one reply per send preserves that backlog indefinitely and
            // puts old serials ahead of the ACK for a new button press.
            // Drain a bounded batch without waiting for another datagram.
            for _ in 0..256 {
                let Ok(n) = socket.recv(&mut buffer) else {
                    break;
                };
                let Ok(reply) = std::str::from_utf8(&buffer[..n]) else {
                    continue;
                };
                if let Some(status) = reply.strip_prefix(&prefix)
                    && matches!(status, "READY" | "WAIT")
                {
                    acknowledged = true;
                    let mut s = lock(&self.inner);
                    s.acknowledged = Some(Instant::now());
                    s.ready = status == "READY";
                }
            }
            // An unavailable or very fast bridge must not turn this into a busy loop.
            thread::sleep(Duration::from_millis(20));
        }
        let _ = socket.send(format!("HDDI1 {epoch} {} 0 0", serial + 1).as_bytes());
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn queued_duplicate_acks_do_not_delay_a_new_press_and_release() {
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            mpsc,
        };
        let bridge = UdpSocket::bind("127.0.0.1:0").unwrap();
        bridge
            .set_read_timeout(Some(Duration::from_millis(20)))
            .unwrap();
        let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
        sender.connect(bridge.local_addr().unwrap()).unwrap();
        sender.set_nonblocking(true).unwrap();
        let c = Controls::new(true);
        let ip = "127.0.0.1".parse().unwrap();
        c.begin(ip);
        let stop = Arc::new(AtomicBool::new(false));
        let (released, received) = mpsc::channel();
        let peer_stop = stop.clone();
        let peer = thread::spawn(move || {
            let mut buffer = [0u8; 128];
            let mut first = true;
            let mut pressed = false;
            while !peer_stop.load(Ordering::Relaxed) {
                let Ok((n, from)) = bridge.recv_from(&mut buffer) else {
                    continue;
                };
                let text = std::str::from_utf8(&buffer[..n]).unwrap();
                let fields: Vec<_> = text.split_whitespace().collect();
                let reply = format!("HDDI1 {} {} READY", fields[1], fields[2]);
                // DCS can flush many replies together after a pause. They must
                // not remain ahead of acknowledgements for the next button.
                for _ in 0..if first { 120 } else { 1 } {
                    bridge.send_to(reply.as_bytes(), from).unwrap();
                }
                first = false;
                if fields[3] == "1" {
                    pressed = true;
                }
                if pressed && fields[3] == "0" {
                    let _ = released.send(());
                }
            }
        });
        let worker = {
            let c = c.clone();
            let stop = stop.clone();
            thread::spawn(move || c.run(sender, &stop))
        };
        let deadline = Instant::now() + Duration::from_secs(2);
        while c.status()["ready"] != true && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(2));
        }
        let press = c.submit(&json!({"session":1,"revision":1,"left":1,"right":0}), ip);
        let release = c.submit(&json!({"session":1,"revision":2,"left":0,"right":0}), ip);
        let delivered = received.recv_timeout(Duration::from_millis(500));
        stop.store(true, Ordering::Relaxed);
        worker.join().unwrap();
        peer.join().unwrap();
        assert!(press.is_ok() && release.is_ok());
        assert!(
            delivered.is_ok(),
            "Queued duplicate ACKs delayed the new button release"
        );
    }
    #[test]
    fn retries_lost_ack_preserves_taps_and_releases_on_contact_loss() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let bridge = UdpSocket::bind("127.0.0.1:0").unwrap();
        bridge
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
        sender.connect(bridge.local_addr().unwrap()).unwrap();
        sender.set_nonblocking(true).unwrap();
        let c = Controls::new(true);
        let ip = "127.0.0.1".parse().unwrap();
        c.begin(ip);
        let stop = Arc::new(AtomicBool::new(false));
        let worker = {
            let c = c.clone();
            let stop = stop.clone();
            thread::spawn(move || c.run(sender, &stop))
        };
        let receive = || {
            let mut b = [0u8; 128];
            let (n, from) = bridge.recv_from(&mut b).unwrap();
            (String::from_utf8(b[..n].to_vec()).unwrap(), from)
        };
        let ack = |text: &str, from| {
            let fields: Vec<_> = text.split_whitespace().collect();
            bridge
                .send_to(
                    format!("HDDI1 {} {} READY", fields[1], fields[2]).as_bytes(),
                    from,
                )
                .unwrap();
        };
        let (hello, from) = receive();
        ack(&hello, from);
        for _ in 0..100 {
            if c.status()["ready"] == true {
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
        assert!(
            c.submit(
                &json!({"session":1,"revision":1,"left":1,"right":524288}),
                ip
            )
            .is_ok()
        );
        assert!(
            c.submit(&json!({"session":1,"revision":2,"left":0,"right":0}), ip)
                .is_ok()
        );
        let press = loop {
            let (p, f) = receive();
            if p.ends_with(" 1 524288") {
                break (p, f);
            }
            ack(&p, f);
        };
        let retry = receive();
        assert_eq!(press.0, retry.0, "Lost ACK must not skip the press");
        ack(&retry.0, retry.1);
        let deadline = Instant::now() + Duration::from_secs(2);
        let release = loop {
            let packet = receive();
            if packet.0.ends_with(" 0 0") {
                break packet;
            }
            assert_eq!(
                packet.0, press.0,
                "Only retries of the held press may precede release"
            );
            assert!(Instant::now() < deadline, "Button release was not sent");
            ack(&packet.0, packet.1);
        };
        ack(&release.0, release.1);
        assert!(
            c.submit(&json!({"session":1,"revision":3,"left":2,"right":0}), ip)
                .is_ok()
        );
        loop {
            let (p, f) = receive();
            ack(&p, f);
            if p.ends_with(" 2 0") {
                break;
            }
        }
        let started = Instant::now();
        loop {
            let (p, f) = receive();
            ack(&p, f);
            if p.ends_with(" 0 0") {
                break;
            }
        }
        assert!(
            started.elapsed() < Duration::from_millis(1100),
            "Lost browser heartbeat must release buttons"
        );
        stop.store(true, Ordering::Relaxed);
        worker.join().unwrap();
    }
    #[test]
    fn validates_owner_generation_masks_order_and_capacity() {
        let c = Controls::new(true);
        let ip = "127.0.0.1".parse().unwrap();
        c.begin(ip);
        {
            let mut s = lock(&c.inner);
            s.ready = true;
            s.acknowledged = Some(Instant::now());
        }
        let mut v = json!({"session":1,"revision":1,"left":1,"right":0});
        assert!(c.submit(&v, ip).is_ok());
        assert_eq!(c.submit(&v, ip).unwrap_err().0, 409);
        v["revision"] = json!(2);
        v["left"] = json!(1 << 20);
        assert_eq!(c.submit(&v, ip).unwrap_err().0, 400);
        v["left"] = json!(0);
        assert_eq!(
            c.submit(&v, "127.0.0.2".parse().unwrap()).unwrap_err().0,
            409
        );
        for i in 2..=64 {
            v["revision"] = json!(i);
            assert!(c.submit(&v, ip).is_ok());
        }
        v["revision"] = json!(65);
        assert_eq!(c.submit(&v, ip).unwrap_err().0, 429);
        c.end();
        assert!(c.submit(&v, ip).is_err());
        c.begin(ip);
        assert_eq!(c.submit(&v, ip).unwrap_err().0, 409);
    }
}
