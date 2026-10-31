//! Setup and supervised non-VR launch. An exclusive file lock owns persistent
//! session state. Save display recovery before mutation; stop only the owned
//! streamer child, and defer restoration while DCS remains alive.
//! LocalAppData records are validated by persistent before applying their values.
use crate::{
    STOP, display,
    persistent::{DisplayRecovery, Settings},
    profile,
};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex, atomic::Ordering},
    thread,
    time::{Duration, Instant},
};
use windows::Win32::{Foundation::CloseHandle, System::Diagnostics::ToolHelp::*};
pub fn data_dir() -> Result<PathBuf, String> {
    Ok(
        PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is unavailable")?)
            .join("DCS Tablet"),
    )
}
fn load(path: &Path) -> Result<Value, String> {
    serde_json::from_slice(&fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?)
        .map_err(|e| e.to_string())
}
pub fn capture_path() -> Result<PathBuf, String> {
    Ok(data_dir()?.join("capture.json"))
}
pub(crate) fn lock(dir: &Path) -> Result<File, String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    crate::filesystem::reject_shared_file(&dir.join("session.lock"))?;
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(0)
        .open(dir.join("session.lock"))
        .map_err(|_| "Another DCS Tablet session is running".into())
}
pub fn dcs_running() -> Result<bool, String> {
    // SAFETY: initialized PROCESSENTRY32W has the required size; the snapshot handle
    // remains valid through enumeration and is closed after the last access.
    unsafe {
        let handle = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).map_err(|e| e.to_string())?;
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut next = Process32FirstW(handle, &mut entry);
        let mut found = false;
        while next.is_ok() {
            let end = entry
                .szExeFile
                .iter()
                .position(|c| *c == 0)
                .unwrap_or(entry.szExeFile.len());
            if String::from_utf16_lossy(&entry.szExeFile[..end]).eq_ignore_ascii_case("DCS.exe") {
                found = true;
                break;
            }
            next = Process32NextW(handle, &mut entry);
        }
        let _ = CloseHandle(handle);
        Ok(found)
    }
}
fn require_closed() -> Result<(), String> {
    if dcs_running()? {
        Err("Close DCS before setup, recovery, or profile removal".into())
    } else {
        Ok(())
    }
}
fn option(args: &[String], key: &str) -> Result<Option<String>, String> {
    if let Some(i) = args.iter().position(|a| a == key) {
        Ok(Some(
            args.get(i + 1)
                .ok_or_else(|| format!("Missing value for {key}"))?
                .clone(),
        ))
    } else {
        Ok(None)
    }
}
fn saved_games() -> Result<PathBuf, String> {
    use windows::Win32::{
        System::Com::CoTaskMemFree,
        UI::Shell::{FOLDERID_SavedGames, KF_FLAG_DEFAULT, SHGetKnownFolderPath},
    };
    // SAFETY: the API allocates a terminated UTF-16 string. Convert it while live,
    // then release that allocation with CoTaskMemFree even if conversion fails.
    let pointer = unsafe { SHGetKnownFolderPath(&FOLDERID_SavedGames, KF_FLAG_DEFAULT, None) }
        .map_err(|e| e.to_string())?;
    let value = unsafe { pointer.to_string() }.map_err(|e| e.to_string());
    unsafe {
        CoTaskMemFree(Some(pointer.0.cast()));
    }
    value.map(PathBuf::from)
}
/// DCS -w selects a name below Windows Saved Games. Refuse a mistaken custom
/// root (including an installation directory) instead of writing an unused profile.
fn validate_saved_games_path(configured: &Path, actual: &Path) -> Result<(), String> {
    let configured =
        fs::canonicalize(configured).map_err(|e| format!("Cannot locate Saved Games: {e}"))?;
    let actual =
        fs::canonicalize(actual).map_err(|e| format!("Cannot locate Windows Saved Games: {e}"))?;
    if !configured
        .as_os_str()
        .eq_ignore_ascii_case(actual.as_os_str())
    {
        return Err("--saved-games must be the actual Windows Saved Games folder used by DCS, not a DCS installation or profile directory.".into());
    }
    Ok(())
}
fn discover_dcs() -> Option<PathBuf> {
    [
        "C:\\Program Files\\Eagle Dynamics\\DCS World\\bin\\DCS.exe",
        "C:\\Program Files\\Eagle Dynamics\\DCS World OpenBeta\\bin\\DCS.exe",
        "C:\\Program Files (x86)\\Steam\\steamapps\\common\\DCSWorld\\bin\\DCS.exe",
    ]
    .into_iter()
    .map(PathBuf::from)
    .find(|p| p.is_file())
}
pub fn command(args: &[String]) -> Result<bool, String> {
    let Some(action) = args.first().map(String::as_str) else {
        return Ok(false);
    };
    if ![
        "displays",
        "setup",
        "launch",
        "restore",
        "uninstall-profile",
        "doctor",
    ]
    .contains(&action)
    {
        return Ok(false);
    }
    display::dpi_aware();
    let dir = data_dir()?;
    crate::filesystem::reject_link(&dir)?;
    let _guard = lock(&dir)?;
    if action == "displays" {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &display::list()?
                    .iter()
                    .map(|d| d.json())
                    .collect::<Vec<_>>()
            )
            .unwrap()
        );
        return Ok(true);
    }
    if action == "doctor" {
        println!(
            "DCS Tablet v{} | Built {} UTC | {}",
            env!("CARGO_PKG_VERSION"),
            env!("DCS_TABLET_BUILD_DATE"),
            env!("DCS_TABLET_BUILD_ID")
        );
        println!(
            "Data: {}\nDCS running: {}\nRecovery pending: {}",
            dir.display(),
            dcs_running()?,
            dir.join("display-recovery.json").exists()
        );
        println!(
            "{}",
            serde_json::to_string_pretty(
                &display::list()?
                    .iter()
                    .map(|d| d.json())
                    .collect::<Vec<_>>()
            )
            .unwrap()
        );
        return Ok(true);
    }
    require_closed()?;
    let settings_path = dir.join("settings.json");
    if action == "setup" {
        for pair in args[1..].chunks(2) {
            if pair.len() != 2
                || ![
                    "--dcs",
                    "--saved-games",
                    "--source-profile",
                    "--display",
                    "--port",
                ]
                .contains(&pair[0].as_str())
            {
                return Err("setup options: --dcs PATH --saved-games PATH --source-profile PATH --display DEVICE --port NUMBER".into());
            }
        }
        let old = if settings_path.exists() {
            Settings::from_value(load(&settings_path)?)?.value()?
        } else {
            json!({})
        };
        let saved = option(args, "--saved-games")?
            .map(PathBuf::from)
            .or_else(|| old["saved_games"].as_str().map(PathBuf::from))
            .map(Ok)
            .unwrap_or_else(saved_games)?;
        validate_saved_games_path(&saved, &saved_games()?)?;
        let source = option(args, "--source-profile")?
            .map(PathBuf::from)
            .or_else(|| old["source_profile"].as_str().map(PathBuf::from))
            .unwrap_or(saved.join("DCS"));
        let dcs = option(args, "--dcs")?
            .map(PathBuf::from)
            .or_else(|| old["dcs"].as_str().map(PathBuf::from))
            .or_else(discover_dcs)
            .ok_or("DCS not found. Pass setup --dcs <path to DCS.exe>")?;
        if !dcs.is_file()
            || !dcs
                .file_name()
                .is_some_and(|n| n.eq_ignore_ascii_case("DCS.exe"))
        {
            return Err("--dcs must point to DCS.exe".into());
        }
        let displays = display::list()?;
        let device = option(args, "--display")?
            .or_else(|| old["display"].as_str().map(str::to_owned))
            .or_else(|| {
                let candidates: Vec<_> = displays
                    .iter()
                    .filter(|d| !d.primary && d.description.to_lowercase().contains("virtual"))
                    .collect();
                if candidates.len() == 1 {
                    Some(candidates[0].device.clone())
                } else {
                    None
                }
            })
            .ok_or(
                "Install a virtual display, run `displays`, and choose it with --display DEVICE",
            )?;
        display::selected(&device)?;
        let port = option(args, "--port")?
            .map(|v| v.parse::<u16>())
            .transpose()
            .map_err(|e| e.to_string())?
            .unwrap_or(old["port"].as_u64().unwrap_or(8090) as u16);
        if port < 1024 {
            return Err("Port must be 1024..65535".into());
        }
        let settings = json!({"version":1,"dcs":dcs,"saved_games":saved,"source_profile":source,"display":device,"port":port});
        let settings = Settings::from_value(settings)?.value()?;
        let primary = displays
            .iter()
            .find(|d| d.primary && d.active)
            .ok_or("No primary display")?;
        profile::prepare(&settings, primary)?;
        profile::write_json(&settings_path, &settings)?;
        println!("Setup complete. Run `dcs-tablet launch`. Firewall setup: docs/network.md");
    } else if action == "restore" {
        restore(&dir)?;
    } else {
        let settings = Settings::from_value(
            load(&settings_path).map_err(|e| format!("Run setup first. {e}"))?,
        )?
        .value()?;
        validate_saved_games_path(
            Path::new(
                settings["saved_games"]
                    .as_str()
                    .ok_or("Missing Saved Games")?,
            ),
            &saved_games()?,
        )?;
        if action == "uninstall-profile" {
            let _archive = profile::uninstall(&settings)?;
        } else {
            launch(&dir, &settings)?;
        }
    }
    Ok(true)
}
fn restore(dir: &Path) -> Result<(), String> {
    let path = dir.join("display-recovery.json");
    if !path.exists() {
        println!("No display recovery pending.");
        return Ok(());
    }
    let state = DisplayRecovery::from_value(load(&path)?)?;
    let current = display::selected(&state.before.device)?;
    if current != state.before && current != state.after {
        return Err("Display layout changed after launch. Preserve it and restore the virtual display manually in Windows Settings; recovery record retained.".into());
    }
    if current != state.before {
        display::restore(&state.before.json())?;
    }
    if display::selected(&state.before.device)? != state.before {
        return Err("Display restoration could not be verified; recovery record retained. Inspect Windows Display Settings.".into());
    }
    fs::remove_file(path).map_err(|e| e.to_string())?;
    println!("Previous virtual-display state restored.");
    Ok(())
}
fn health(port: u16) -> Result<Value, String> {
    let address: SocketAddr = ([127, 0, 0, 1], port).into();
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_millis(400))
        .map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(1)))
        .map_err(|e| e.to_string())?;
    write!(
        stream,
        "GET /health HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    )
    .map_err(|e| e.to_string())?;
    let mut bytes = String::new();
    stream
        .take(65536)
        .read_to_string(&mut bytes)
        .map_err(|e| e.to_string())?;
    let (_, body) = bytes
        .split_once("\r\n\r\n")
        .ok_or("Invalid health response")?;
    serde_json::from_str(body).map_err(|e| e.to_string())
}
struct OwnedStream {
    child: Child,
    port: u16,
    logs: Vec<thread::JoinHandle<()>>,
}
impl std::ops::Deref for OwnedStream {
    type Target = Child;
    fn deref(&self) -> &Child {
        &self.child
    }
}
impl std::ops::DerefMut for OwnedStream {
    fn deref_mut(&mut self) -> &mut Child {
        &mut self.child
    }
}
impl Drop for OwnedStream {
    fn drop(&mut self) {
        stop_stream(&mut self.child, self.port);
        for reader in self.logs.drain(..) {
            let _ = reader.join();
        }
    }
}
/// Keep at most three prior logs. The caller closes its file before rotation;
/// child processes own pipes rather than inheriting this Windows file handle.
fn rotate_log(path: &Path, limit: u64) -> Result<(), String> {
    crate::filesystem::reject_shared_file(path)?;
    for n in 1..=3 {
        crate::filesystem::reject_shared_file(&path.with_extension(format!("log.{n}")))?;
    }
    if !path.exists() || fs::metadata(path).map_err(|e| e.to_string())?.len() < limit {
        return Ok(());
    }
    let archive = |n| path.with_extension(format!("log.{n}"));
    if archive(3).exists() {
        fs::remove_file(archive(3)).map_err(|e| e.to_string())?;
    }
    for n in (1..3).rev() {
        if archive(n).exists() {
            fs::rename(archive(n), archive(n + 1)).map_err(|e| e.to_string())?;
        }
    }
    fs::rename(path, archive(1)).map_err(|e| e.to_string())
}
/// Both child pipes share this bounded writer. Only the parent owns the file,
/// so rotation can close the Windows handle before renaming while DCS runs.
struct RotatingLog {
    path: PathBuf,
    file: Option<File>,
    length: u64,
    limit: u64,
}
impl RotatingLog {
    fn open(path: PathBuf, limit: u64) -> Result<Self, String> {
        if limit == 0 {
            return Err("Log limit must be positive".into());
        }
        rotate_log(&path, limit)?;
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| e.to_string())?;
        let length = file.metadata().map_err(|e| e.to_string())?.len();
        Ok(Self {
            path,
            file: Some(file),
            length,
            limit,
        })
    }
    fn write(&mut self, mut bytes: &[u8]) -> Result<(), String> {
        while !bytes.is_empty() {
            if self.length >= self.limit {
                drop(self.file.take());
                rotate_log(&self.path, self.limit)?;
                self.file = Some(
                    OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&self.path)
                        .map_err(|e| e.to_string())?,
                );
                self.length = 0;
            }
            let count = bytes.len().min((self.limit - self.length) as usize);
            self.file
                .as_mut()
                .ok_or("Log writer is closed")?
                .write_all(&bytes[..count])
                .map_err(|e| e.to_string())?;
            self.length += count as u64;
            bytes = &bytes[count..];
        }
        Ok(())
    }
}
fn copy_log(mut reader: impl Read, log: &Mutex<RotatingLog>) -> Result<(), String> {
    let mut buffer = [0u8; 8192];
    loop {
        let count = match reader.read(&mut buffer) {
            Ok(0) => return Ok(()),
            Ok(count) => count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.to_string()),
        };
        crate::state::lock(log).write(&buffer[..count])?;
    }
}
fn log_reader(
    reader: impl Read + Send + 'static,
    log: Arc<Mutex<RotatingLog>>,
) -> Result<thread::JoinHandle<()>, String> {
    thread::Builder::new()
        .name("tablet-log".into())
        .spawn(move || {
            if let Err(error) = copy_log(reader, &log) {
                eprintln!("Stream log writer: {error}");
            }
        })
        .map_err(|e| e.to_string())
}
fn start_stream(dir: &Path, port: u16) -> Result<OwnedStream, String> {
    start_stream_mode(dir, port, "--capture")
}
#[cfg(feature = "test-hooks")]
pub fn supervised_demo_test(dir: &Path, port: u16) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    profile::write_json(
        &dir.join("capture.json"),
        &json!({"port":port,"fps":30,"size":512}),
    )?;
    let mut stream = start_stream_mode(dir, port, "--demo")?;
    profile::write_json(
        &dir.join("owned-ready.json"),
        &json!({"process_id":stream.id()}),
    )?;
    stream.wait().map_err(|e| e.to_string())?;
    // Drop joins the pipe readers after the owned child has closed its handles.
    Ok(())
}
fn start_stream_mode(dir: &Path, port: u16, mode: &str) -> Result<OwnedStream, String> {
    let mut log = RotatingLog::open(dir.join("stream.log"), 2 * 1024 * 1024)?;
    log.write(
        format!(
            "\n--- Session {:?} | v{} | build {} ---\n",
            std::time::SystemTime::now(),
            env!("CARGO_PKG_VERSION"),
            env!("DCS_TABLET_BUILD_ID")
        )
        .as_bytes(),
    )?;
    let log = Arc::new(Mutex::new(log));
    let child = Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
        .args([mode, "--config"])
        .arg(dir.join("capture.json"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut child = OwnedStream {
        child,
        port,
        logs: Vec::new(),
    };
    let stdout = child.stdout.take().ok_or("Missing streamer stdout pipe")?;
    let stderr = child.stderr.take().ok_or("Missing streamer stderr pipe")?;
    child.logs.push(log_reader(stdout, log.clone())?);
    child.logs.push(log_reader(stderr, log)?);
    for _ in 0..50 {
        if child.try_wait().map_err(|e| e.to_string())?.is_some() {
            return Err("Streamer exited; inspect stream.log".into());
        }
        if health(port).is_ok_and(|s| {
            s["process_id"].as_u64() == Some(u64::from(child.id())) && s["healthy"] == true
        }) {
            return Ok(child);
        }
        if STOP.load(Ordering::Relaxed) {
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    let _ = child.wait();
    Err("Streamer failed to become healthy; inspect stream.log".into())
}
fn stop_stream(child: &mut Child, port: u16) {
    // Request orderly cleanup only after verifying the listener belongs to our child.
    if child.try_wait().ok().flatten().is_none()
        && health(port).is_ok_and(|s| s["process_id"].as_u64() == Some(u64::from(child.id())))
    {
        if let Ok(mut socket) =
            TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_millis(300))
        {
            let _ = socket.set_write_timeout(Some(Duration::from_millis(500)));
            let _ = write!(
                socket,
                "POST /shutdown HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\nContent-Length: 0\r\n\r\n"
            );
        }
        for _ in 0..30 {
            if child.try_wait().ok().flatten().is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(100));
        }
    }
    // Terminate only the owned child if orderly cleanup did not complete.
    if child.try_wait().ok().flatten().is_none() {
        let _ = child.kill();
    }
    let _ = child.wait();
}
/// At most three restart attempts in a rolling minute; the caller owns sleeping.
#[derive(Default)]
struct RestartBudget(VecDeque<Instant>);
impl RestartBudget {
    fn next_delay(&mut self, now: Instant) -> Result<Duration, String> {
        while self
            .0
            .front()
            .is_some_and(|t| now.duration_since(*t) > Duration::from_secs(60))
        {
            self.0.pop_front();
        }
        if self.0.len() >= 3 {
            return Err("Streamer recovery limit reached. Close DCS, restore the display, and inspect stream.log.".into());
        }
        let delay = Duration::from_secs(1 << self.0.len());
        self.0.push_back(now);
        Ok(delay)
    }
}
fn launch(dir: &Path, settings: &Value) -> Result<(), String> {
    if dir.join("display-recovery.json").exists() {
        return Err("Previous session needs recovery. Run `dcs-tablet restore` first.".into());
    }
    let port = u16::try_from(settings["port"].as_u64().ok_or("Invalid port")?)
        .map_err(|_| "Port out of range")?;
    let port_check = TcpListener::bind(("0.0.0.0", port))
        .map_err(|_| format!("Port {port} is already in use"))?;
    drop(port_check);
    let device = settings["display"].as_str().ok_or("Missing display")?;
    let before = display::selected(device)?;
    let displays = display::list()?;
    let primary = displays
        .iter()
        .find(|d| d.primary && d.active)
        .ok_or("No primary display")?;
    profile::prepare(settings, primary)?;
    let mut after = before.clone();
    after.active = true;
    after.x = primary.x + primary.width as i32;
    after.y = primary.y;
    after.width = 1024;
    after.height = 512;
    profile::write_json(
        &dir.join("display-recovery.json"),
        &json!({"version":1,"before":before.json(),"after":after.json()}),
    )?;
    let result = (|| {
        display::configure(device, true)?;
        let actual = display::selected(device)?;
        profile::write_json(
            &dir.join("display-recovery.json"),
            &json!({"version":1,"before":before.json(),"after":actual.json()}),
        )?;
        profile::write_json(
            &dir.join("capture.json"),
            &display::capture_config(device, port)?,
        )?;
        let mut stream = start_stream(dir, port)?;
        let dcs_path = PathBuf::from(settings["dcs"].as_str().ok_or("Missing DCS path")?);
        let game_result = Command::new(&dcs_path)
            .current_dir(dcs_path.parent().ok_or("Invalid DCS path")?)
            .args(["--no-launcher", "--force_disable_VR", "-w", "DCS.Tablet"])
            .spawn();
        let mut game = match game_result {
            Ok(game) => game,
            Err(e) => {
                stop_stream(&mut stream, port);
                return Err(e.to_string());
            }
        };
        println!(
            "DCS Tablet session started. Open http://<PC-LAN-IP>:{port}/ in landscape. Keep this console open. Logs: {}",
            dir.join("stream.log").display()
        );
        let mut attempts = RestartBudget::default();
        let mut failures = 0;
        let mut last_health = Instant::now();
        let mut placed = false;
        let running = (|| {
            while game.try_wait().map_err(|e| e.to_string())?.is_none() {
                if STOP.load(Ordering::Relaxed) {
                    stop_stream(&mut stream, port);
                    println!("Streaming stopped. Close DCS normally to restore the display.");
                    while game.try_wait().map_err(|e| e.to_string())?.is_none() {
                        thread::sleep(Duration::from_millis(250));
                    }
                    break;
                }
                if !placed {
                    placed = display::place_window(game.id())?;
                }
                if last_health.elapsed() >= Duration::from_secs(2) {
                    last_health = Instant::now();
                    let healthy = stream.try_wait().map_err(|e| e.to_string())?.is_none()
                        && health(port).is_ok_and(|s| {
                            s["process_id"].as_u64() == Some(u64::from(stream.id()))
                                && s["healthy"] == true
                        });
                    failures = if healthy { 0 } else { failures + 1 };
                    if failures >= 2 {
                        stop_stream(&mut stream, port);
                        thread::sleep(attempts.next_delay(Instant::now())?);
                        stream = start_stream(dir, port)?;
                        failures = 0;
                        println!("Streamer recovered. Tap Connect on the tablet.");
                    }
                }
                thread::sleep(Duration::from_millis(250));
            }
            Ok(())
        })();
        stop_stream(&mut stream, port);
        if game.try_wait().ok().flatten().is_none() {
            println!("DCS remains running. Close it before running `dcs-tablet restore`.");
            return running.and(Err("Display recovery deferred while DCS is running".into()));
        }
        running
    })();
    if !dcs_running()? {
        let cleanup = restore(dir);
        result.and(cleanup)
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn setup_rejects_wrong_saved_games_roots_without_writes() {
        let dir = std::env::temp_dir().join(format!("dcs-root-check-{}", std::process::id()));
        let actual = dir.join("Saved Games");
        let wrong = dir.join("DCS World");
        fs::create_dir_all(&actual).unwrap();
        fs::create_dir_all(&wrong).unwrap();
        fs::write(wrong.join("DCS.exe"), b"installation sentinel").unwrap();
        assert!(validate_saved_games_path(&actual, &actual).is_ok());
        assert!(validate_saved_games_path(&wrong, &actual).is_err());
        assert!(validate_saved_games_path(&dir.join("missing"), &actual).is_err());
        assert_eq!(
            fs::read(wrong.join("DCS.exe")).unwrap(),
            b"installation sentinel"
        );
        assert_eq!(fs::read_dir(&wrong).unwrap().count(), 1);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn an_active_session_cannot_grow_logs_past_the_byte_budget() {
        let dir = std::env::temp_dir().join(
            format!(
                "dcstablet-active-log-{}-{:?}",
                std::process::id(),
                Instant::now()
            )
            .replace(':', ""),
        );
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("stream.log");
        let log = Mutex::new(RotatingLog::open(path.clone(), 64).unwrap());
        let input: Vec<_> = (0..=255u8).cycle().take(1000).collect();
        copy_log(std::io::Cursor::new(&input), &log).unwrap();
        drop(log);
        for entry in fs::read_dir(&dir).unwrap() {
            assert!(entry.unwrap().metadata().unwrap().len() <= 64);
        }
        let current = fs::read(&path).unwrap();
        assert_eq!(current, input[960..]);
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 4);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn restart_budget_bounds_attempts_and_recovers_after_the_window() {
        let now = Instant::now();
        let mut budget = RestartBudget::default();
        for seconds in [1, 2, 4] {
            assert_eq!(
                budget.next_delay(now).unwrap(),
                Duration::from_secs(seconds)
            );
        }
        assert!(budget.next_delay(now + Duration::from_secs(60)).is_err());
        assert_eq!(
            budget.next_delay(now + Duration::from_secs(61)).unwrap(),
            Duration::from_secs(1)
        );
    }
    #[test]
    fn rotates_logs_without_overwriting_recent_history() {
        let dir = std::env::temp_dir().join(
            format!("dcstablet-logs-{}-{:?}", std::process::id(), Instant::now()).replace(':', ""),
        );
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("stream.log");
        for n in 0..5 {
            fs::write(&path, n.to_string()).unwrap();
            rotate_log(&path, 1).unwrap();
        }
        assert!(!path.exists());
        assert_eq!(fs::read_to_string(dir.join("stream.log.1")).unwrap(), "4");
        assert_eq!(fs::read_to_string(dir.join("stream.log.3")).unwrap(), "2");
        assert!(!dir.join("stream.log.4").exists());
        fs::remove_dir_all(dir).unwrap();
    }
}
