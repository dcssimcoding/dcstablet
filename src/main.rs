//! Executable entry point and fixed HTTP routes. A viewer lease serializes
//! WebRTC ownership; controls are released before that lease becomes available.
#[path = "../build_support.rs"]
mod build_support;
mod capture;
mod config;
mod controls;
mod display;
mod encode;
mod faults;
mod filesystem;
mod http;
mod mf_encode;
mod persistent;
mod profile;
mod rtc;
mod session;
mod state;
mod workers;

use config::Config;
use http::{Method, Request, Server};
use serde_json::{Value, json};
use state::{Lease, Stage, State, lock};
use std::sync::mpsc::RecvTimeoutError;
use std::{
    net::{IpAddr, Ipv4Addr, UdpSocket},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

pub static STOP: AtomicBool = AtomicBool::new(false);
const PAGE: &[u8] = include_bytes!("../web/index.html");
type SessionThread = Arc<Mutex<Option<JoinHandle<()>>>>;

#[link(name = "Kernel32")]
unsafe extern "system" {
    fn SetConsoleCtrlHandler(handler: Option<extern "system" fn(u32) -> i32>, add: i32) -> i32;
}
extern "system" fn console_control(code: u32) -> i32 {
    if matches!(code, 0 | 1 | 2 | 5 | 6) {
        STOP.store(true, Ordering::Relaxed);
        1
    } else {
        0
    }
}
fn respond(request: Request, status: u16, kind: &str, bytes: impl Into<Vec<u8>>) {
    request.respond(status, kind, bytes.into());
}
fn header(request: &Request, name: &'static str) -> Option<String> {
    request.header(name).map(str::to_owned)
}

fn handle(
    request: Request,
    cfg: &Arc<Config>,
    state: &Arc<State>,
    session: &SessionThread,
    controls: &Arc<controls::Controls>,
) {
    if let Some(origin) = header(&request, "Origin")
        && Some(origin) != header(&request, "Host").map(|host| format!("http://{host}"))
    {
        respond(request, 403, "text/plain", "Use the page served by this PC");
        return;
    }
    let path = request.url().split('?').next().unwrap_or("").to_owned();
    match (*request.method(), path.as_str()) {
        (Method::Get, "/") => respond(request, 200, "text/html; charset=utf-8", PAGE),
        (Method::Get, "/status" | "/health") => {
            let value = json!({"mode":if cfg.demo {"demo"} else {"capture"},"fps":cfg.fps,"size":cfg.size,
                "capture_source":state.capture_status(),"controls":controls.status(),"backend":"rust","codec":"Media Foundation H.264","active_viewers":u8::from(state.active.load(Ordering::Acquire)),
                "healthy":!STOP.load(Ordering::Relaxed),"process_id":std::process::id(),"version":env!("CARGO_PKG_VERSION"),
                "build_id":env!("DCS_TABLET_BUILD_ID"),"built_unix_ms":env!("DCS_TABLET_BUILD_UNIX_MS"),"build_date":env!("DCS_TABLET_BUILD_DATE"),
                "compiler":env!("DCS_TABLET_COMPILER"),"default_features":cfg!(feature="default"),"test_hooks":cfg!(feature="test-hooks"),"bitrate_target":cfg.bitrate,"last_error":state.failure_text(),"failure":state.failure_details()});
            respond(request, 200, "application/json", value.to_string());
        }
        (Method::Get, "/metrics") => {
            let mut value = state.snapshot();
            value["build_id"] = json!(env!("DCS_TABLET_BUILD_ID"));
            value["version"] = json!(env!("CARGO_PKG_VERSION"));
            value["configuration"] =
                json!({"fps":cfg.fps,"size":cfg.size,"bitrate_target":cfg.bitrate});
            respond(request, 200, "application/json", value.to_string());
        }
        (Method::Post, "/controls") => {
            if !state.active.load(Ordering::Acquire) {
                respond(request, 409, "text/plain", "Connect the display first");
                return;
            }
            let remote = request
                .remote_addr()
                .map(|a| a.ip())
                .unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED));
            let body = request.body();
            match serde_json::from_slice::<Value>(body) {
                Ok(value) => match controls.submit(&value, remote) {
                    Ok(()) => respond(request, 202, "application/json", r#"{"accepted":true}"#),
                    Err((code, error)) => respond(request, code, "text/plain", error),
                },
                Err(_) => respond(
                    request,
                    400,
                    "text/plain",
                    "Expected a JSON button snapshot",
                ),
            }
        }
        (Method::Post, "/shutdown") => {
            if !request.remote_addr().is_some_and(|a| a.ip().is_loopback()) {
                respond(request, 403, "text/plain", "Local PC only");
                return;
            }
            respond(request, 200, "application/json", r#"{"stopping":true}"#);
            STOP.store(true, Ordering::Relaxed);
        }
        (Method::Post, "/offer") => {
            let body = request.body();
            let offer: Value = match serde_json::from_slice(body) {
                Ok(offer) => offer,
                Err(_) => {
                    respond(request, 400, "text/plain", "Expected a JSON SDP offer");
                    return;
                }
            };
            if state
                .active
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
                .is_err()
            {
                respond(
                    request,
                    409,
                    "text/plain",
                    "A viewer is already connected. Disconnect it first.",
                );
                return;
            }
            let lease = Lease(state.clone());
            let remote = request
                .remote_addr()
                .map(|a| a.ip())
                .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
            let (peer, answer) = match rtc::prepare(offer, remote) {
                Ok(result) => result,
                Err(error) => {
                    drop(lease);
                    respond(request, 400, "text/plain", error);
                    return;
                }
            };
            let mut slot = lock(session);
            if let Some(previous) = slot.take()
                && let Err(panic) = previous.join()
            {
                state.fail(Stage::Session, workers::panic_text(panic));
            }
            state.begin_session();
            controls.begin(remote);
            let worker_controls = controls.clone();
            let worker_state = state.clone();
            let worker_cfg = cfg.clone();
            match thread::Builder::new()
                .name("ddi-webrtc".into())
                .spawn(move || {
                    struct EndControls(Arc<controls::Controls>);
                    impl Drop for EndControls {
                        fn drop(&mut self) {
                            self.0.end();
                        }
                    }
                    let _lease = lease;
                    let _controls = EndControls(worker_controls); // Release controls before releasing viewer ownership.
                    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        rtc::run(peer, worker_cfg, worker_state.clone())
                    })) {
                        Ok(Ok(())) => {}
                        Ok(Err(error)) => worker_state.fail(Stage::Session, error),
                        Err(panic) => {
                            worker_state.panics.fetch_add(1, Ordering::Relaxed);
                            worker_state.fail(Stage::Session, workers::panic_text(panic));
                        }
                    }
                }) {
                Ok(join) => {
                    *slot = Some(join);
                    respond(request, 200, "application/json", answer.to_string());
                }
                Err(error) => {
                    state.fail(
                        Stage::Session,
                        format!("Could not start session worker: {error}"),
                    );
                    respond(request, 500, "text/plain", error.to_string());
                }
            }
        }
        _ => respond(request, 404, "text/plain", "Not found"),
    }
}
fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    unsafe {
        SetConsoleCtrlHandler(Some(console_control), 1);
    }
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if session::command(&arguments)? {
        return Ok(());
    }
    display::dpi_aware();
    let mut args = arguments.into_iter();
    let mut path: Option<PathBuf> = None;
    let mut capture = false;
    let mut port = None;
    let mut bitrate = 1_000_000;
    #[cfg(feature = "test-hooks")]
    let mut test_fault_file = None;
    while let Some(argument) = args.next() {
        match argument.as_str() {
            #[cfg(feature = "test-hooks")]
            "--test-fault-file" => {
                test_fault_file = Some(PathBuf::from(args.next().ok_or("Missing fault file")?))
            }
            #[cfg(feature = "test-hooks")]
            "--test-supervised-demo" => {
                let port = args.next().ok_or("Missing test port")?.parse()?;
                let dir = PathBuf::from(args.next().ok_or("Missing test directory")?);
                session::supervised_demo_test(&dir, port)?;
                return Ok(());
            }
            "--capture" => capture = true,
            "--demo" => capture = false,
            "--config" => path = Some(args.next().ok_or("Missing config path")?.into()),
            "--port" => port = Some(args.next().ok_or("Missing port")?.parse()?),
            "--bitrate" => bitrate = args.next().ok_or("Missing bitrate")?.parse()?,
            "--build-info" => {
                println!(
                    "{}",
                    json!({"version":env!("CARGO_PKG_VERSION"),"build_id":env!("DCS_TABLET_BUILD_ID"),"compiler":env!("DCS_TABLET_COMPILER"),"target":env!("DCS_TABLET_TARGET"),"default_features":cfg!(feature="default"),"test_hooks":cfg!(feature="test-hooks")})
                );
                return Ok(());
            }
            "--verify-source" => {
                let root = PathBuf::from(args.next().ok_or("Missing source directory")?);
                let mut features = Vec::new();
                if cfg!(feature = "default") {
                    features.push(("CARGO_FEATURE_DEFAULT".into(), "1".into()));
                }
                if cfg!(feature = "test-hooks") {
                    features.push(("CARGO_FEATURE_TEST_HOOKS".into(), "1".into()));
                }
                let hash = build_support::fingerprint(&root, &features)?;
                if hash != env!("DCS_TABLET_BUILD_ID") {
                    return Err("Executable does not match the current source and feature set. Rebuild before packaging.".into());
                }
                println!("Source fingerprint verified: {hash}");
                return Ok(());
            }
            "--version" | "-V" => {
                println!(
                    "DCS Tablet v{} · Built {} UTC · {}",
                    env!("CARGO_PKG_VERSION"),
                    env!("DCS_TABLET_BUILD_DATE"),
                    env!("DCS_TABLET_BUILD_ID")
                );
                return Ok(());
            }
            "--help" | "-h" => {
                println!(
                    "DCS Tablet (Windows, landscape, non-VR)\nCommands: displays | setup [--dcs PATH] [--saved-games PATH] [--source-profile PATH] [--display DEVICE] [--port PORT] | launch | restore | uninstall-profile | doctor\nStreaming: dcs-tablet [--capture|--demo] [--config path] [--port 8090] [--bitrate 1000000]\nThreaded Windows Graphics Capture, Windows Media Foundation H.264, native Windows crypto, WebRTC.\nGET /status, GET /metrics; local-only POST /shutdown. No token required."
                );
                return Ok(());
            }
            _ => return Err(format!("Unknown argument: {argument}").into()),
        }
    }
    #[allow(unused_mut)]
    let mut cfg = if capture {
        Config::load(
            &path.unwrap_or(session::capture_path()?),
            false,
            port,
            bitrate,
        )?
    } else if let Some(path) = path {
        Config::load(&path, true, port, bitrate)?
    } else {
        Config::demo(port, bitrate)?
    };
    #[cfg(feature = "test-hooks")]
    {
        cfg.test_fault_file = test_fault_file;
    }
    let cfg = Arc::new(cfg);
    str0m::crypto::from_feature_flags().install_process_default();
    unsafe {
        SetConsoleCtrlHandler(Some(console_control), 1);
    }
    let state = Arc::new(State::default());
    let server = Arc::new(Server::bind(cfg.port, state.clone())?);
    let session: SessionThread = Arc::new(Mutex::new(None));
    println!(
        "DCS Tablet | {} | {} fps | Media Foundation H.264 | Ctrl+C to stop",
        if cfg.demo { "DEMO" } else { "CAPTURE" },
        cfg.fps
    );
    println!("Local: http://127.0.0.1:{}/", cfg.port);
    if let Ok(route) = UdpSocket::bind("0.0.0.0:0")
        && route.connect("192.0.2.1:9").is_ok()
        && let Ok(address) = route.local_addr()
    {
        println!("LAN: http://{}:{}/", address.ip(), cfg.port);
    }
    let controls = controls::Controls::new(!cfg.demo);
    let control_worker = if cfg.demo {
        None
    } else {
        Some(controls.spawn()?)
    };
    let mut workers = Vec::new();
    for n in 0..4 {
        let (server, cfg, state, session, controls) = (
            server.clone(),
            cfg.clone(),
            state.clone(),
            session.clone(),
            controls.clone(),
        );
        workers.push(
            thread::Builder::new()
                .name(format!("ddi-http-{n}"))
                .spawn(move || {
                    while !STOP.load(Ordering::Relaxed) {
                        match server.recv_timeout(Duration::from_millis(250)) {
                            Ok(request) => {
                                if STOP.load(Ordering::Relaxed) {
                                    break;
                                }
                                if let Err(panic) =
                                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                        handle(request, &cfg, &state, &session, &controls)
                                    }))
                                {
                                    state.panics.fetch_add(1, Ordering::Relaxed);
                                    state.fail(Stage::Http, workers::panic_text(panic));
                                }
                            }
                            Err(RecvTimeoutError::Timeout) => {}
                            Err(RecvTimeoutError::Disconnected) => {
                                break;
                            }
                        }
                    }
                })?,
        );
    }
    while !STOP.load(Ordering::Relaxed) {
        thread::sleep(Duration::from_millis(100));
    }
    for worker in workers {
        if let Err(panic) = worker.join() {
            state.fail(Stage::Shutdown, workers::panic_text(panic));
        }
    }
    let worker = lock(&session).take();
    if let Some(worker) = worker
        && let Err(panic) = worker.join()
    {
        state.fail(Stage::Shutdown, workers::panic_text(panic));
    }
    controls.end();
    if let Some(worker) = control_worker
        && worker.join().is_err()
    {
        eprintln!("Control worker panicked");
    }
    println!("Rust streamer stopped.");
    Ok(())
}
