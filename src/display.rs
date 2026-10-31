//! Native Windows display management. Only the explicitly selected virtual device is changed.
use serde_json::{Value, json};
use std::{mem::size_of, thread, time::Duration};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, RECT},
        Graphics::Gdi::*,
        UI::{HiDpi::*, WindowsAndMessaging::*},
    },
    core::{BOOL, PCWSTR},
};
use windows_capture::monitor::Monitor;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn text(s: &[u16]) -> String {
    String::from_utf16_lossy(&s[..s.iter().position(|c| *c == 0).unwrap_or(s.len())])
}
pub fn dpi_aware() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Display {
    pub device: String,
    pub description: String,
    pub active: bool,
    pub primary: bool,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub frequency: u32,
}
impl Display {
    pub fn validate(&self) -> Result<(), String> {
        if self.device.is_empty()
            || self.primary
            || !self.description.to_lowercase().contains("virtual")
        {
            return Err("Recovery requires a non-primary virtual display".into());
        }
        if self.active
            && (self.width == 0
                || self.height == 0
                || self.width > 65535
                || self.height > 65535
                || self.frequency == 0
                || self.frequency > 1000)
        {
            return Err("Invalid active display mode".into());
        }
        Ok(())
    }
    pub fn json(&self) -> Value {
        json!({"device":self.device,"description":self.description,"active":self.active,"primary":self.primary,"x":self.x,"y":self.y,"width":self.width,"height":self.height,"frequency":self.frequency})
    }
}
fn mode(device: &str) -> Result<DEVMODEW, String> {
    let name = wide(device);
    let mut m = DEVMODEW {
        dmSize: size_of::<DEVMODEW>() as u16,
        ..Default::default()
    };
    if !unsafe { EnumDisplaySettingsW(PCWSTR(name.as_ptr()), ENUM_CURRENT_SETTINGS, &mut m) }
        .as_bool()
    {
        return Err(format!("Cannot read display mode: {device}"));
    }
    Ok(m)
}
pub fn list() -> Result<Vec<Display>, String> {
    let mut result = Vec::new();
    for n in 0..64 {
        let mut d = DISPLAY_DEVICEW {
            cb: size_of::<DISPLAY_DEVICEW>() as u32,
            ..Default::default()
        };
        if !unsafe { EnumDisplayDevicesW(None, n, &mut d, 0) }.as_bool() {
            break;
        }
        if d.StateFlags.contains(DISPLAY_DEVICE_MIRRORING_DRIVER) {
            continue;
        }
        let device = text(&d.DeviceName);
        let m = mode(&device).unwrap_or_default();
        let point = unsafe { m.Anonymous1.Anonymous2.dmPosition };
        result.push(Display {
            device,
            description: text(&d.DeviceString),
            active: d.StateFlags.contains(DISPLAY_DEVICE_ATTACHED_TO_DESKTOP),
            primary: d.StateFlags.contains(DISPLAY_DEVICE_PRIMARY_DEVICE),
            x: point.x,
            y: point.y,
            width: m.dmPelsWidth,
            height: m.dmPelsHeight,
            frequency: m.dmDisplayFrequency,
        });
    }
    Ok(result)
}
pub fn selected(device: &str) -> Result<Display, String> {
    let d = list()?
        .into_iter()
        .find(|d| d.device == device)
        .ok_or("Selected display no longer exists")?;
    if d.primary || !d.description.to_lowercase().contains("virtual") {
        return Err("Select a non-primary virtual display from `dcs-tablet displays`.".into());
    }
    Ok(d)
}
fn apply(device: &str, m: &DEVMODEW) -> Result<(), String> {
    let name = wide(device);
    let code = unsafe {
        ChangeDisplaySettingsExW(
            PCWSTR(name.as_ptr()),
            Some(m),
            None,
            CDS_UPDATEREGISTRY | CDS_NORESET,
            None,
        )
    };
    if code != DISP_CHANGE_SUCCESSFUL {
        return Err(format!("Display change failed: {}", code.0));
    }
    let code = unsafe { ChangeDisplaySettingsExW(None, None, None, CDS_TYPE(0), None) };
    if code != DISP_CHANGE_SUCCESSFUL {
        return Err(format!("Applying display changes failed: {}", code.0));
    }
    Ok(())
}
pub fn configure(device: &str, enabled: bool) -> Result<(), String> {
    let before = list()?;
    let primary = before
        .iter()
        .find(|d| d.primary && d.active)
        .ok_or("No active primary display")?;
    selected(device)?;
    let x = primary
        .x
        .checked_add(primary.width as i32)
        .ok_or("Display position overflow")?;
    if enabled
        && before.iter().any(|d| {
            d.active && d.device != device && !d.primary && overlaps(x, primary.y, 1024, 512, d)
        })
    {
        return Err(
            "The virtual display would overlap another monitor. Reposition that monitor first."
                .into(),
        );
    }
    let mut m = mode(device).unwrap_or(DEVMODEW {
        dmSize: size_of::<DEVMODEW>() as u16,
        ..Default::default()
    });
    m.dmFields = DM_POSITION | DM_PELSWIDTH | DM_PELSHEIGHT;
    m.Anonymous1.Anonymous2.dmPosition.x = x;
    m.Anonymous1.Anonymous2.dmPosition.y = primary.y;
    m.dmPelsWidth = if enabled { 1024 } else { 0 };
    m.dmPelsHeight = if enabled { 512 } else { 0 };
    apply(device, &m)?;
    for _ in 0..30 {
        let after = list()?;
        if after.iter().find(|d| d.device == primary.device) != Some(primary) {
            return Err(
                "Primary display changed unexpectedly; inspect Windows display settings.".into(),
            );
        }
        if after.iter().any(|d| {
            d.device == device
                && d.active == enabled
                && (!enabled
                    || (d.x == x && d.y == primary.y && d.width == 1024 && d.height == 512))
        }) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err("Virtual display did not reach its requested geometry".into())
}
fn overlaps(x: i32, y: i32, w: u32, h: u32, d: &Display) -> bool {
    i64::from(x) < i64::from(d.x) + i64::from(d.width)
        && i64::from(d.x) < i64::from(x) + i64::from(w)
        && i64::from(y) < i64::from(d.y) + i64::from(d.height)
        && i64::from(d.y) < i64::from(y) + i64::from(h)
}
pub fn restore(saved: &Value) -> Result<(), String> {
    let saved: Display =
        serde_json::from_value(saved.clone()).map_err(|e| format!("Invalid saved display: {e}"))?;
    saved.validate()?;
    let device = saved.device.as_str();
    selected(device)?;
    if saved.active
        && list()?.iter().any(|d| {
            d.active
                && d.device != device
                && overlaps(saved.x, saved.y, saved.width, saved.height, d)
        })
    {
        return Err("The saved virtual display position now overlaps another display. Recovery record retained; restore manually.".into());
    }
    let mut m = mode(device).unwrap_or(DEVMODEW {
        dmSize: size_of::<DEVMODEW>() as u16,
        ..Default::default()
    });
    m.dmFields = DM_POSITION | DM_PELSWIDTH | DM_PELSHEIGHT | DM_DISPLAYFREQUENCY;
    m.Anonymous1.Anonymous2.dmPosition.x = saved.x;
    m.Anonymous1.Anonymous2.dmPosition.y = saved.y;
    m.dmPelsWidth = if saved.active { saved.width } else { 0 };
    m.dmPelsHeight = if saved.active { saved.height } else { 0 };
    m.dmDisplayFrequency = saved.frequency;
    if !saved.active {
        m.dmFields = DM_POSITION | DM_PELSWIDTH | DM_PELSHEIGHT;
    }
    apply(device, &m)?;
    for _ in 0..30 {
        if selected(device)? == saved {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err(
        "Virtual display restoration did not reach its saved state; recovery record retained."
            .into(),
    )
}
pub fn capture_config(device: &str, port: u16) -> Result<Value, String> {
    let d = selected(device)?;
    if !d.active || d.width != 1024 || d.height != 512 {
        return Err("Virtual display must be enabled at 1024 × 512".into());
    }
    let monitors = Monitor::enumerate().map_err(|e| e.to_string())?;
    let monitor_index = monitors
        .iter()
        .position(|m| m.device_name().ok().as_deref() == Some(device))
        .ok_or("Virtual display unavailable for Windows Graphics Capture")?
        + 1;
    Ok(
        json!({"port":port,"fps":30,"size":512,"capture_backend":"wgc","capture_monitor_index":monitor_index,"capture_monitor_device":device,"capture_monitor_rect":{"left":d.x,"top":d.y,"width":d.width,"height":d.height},"left":{"left":d.x,"top":d.y,"width":512,"height":512},"right":{"left":d.x+512,"top":d.y,"width":512,"height":512}}),
    )
}
struct WindowSearch {
    pid: u32,
    best: Option<(i64, HWND)>,
}
unsafe extern "system" fn window_callback(hwnd: HWND, param: LPARAM) -> BOOL {
    // SAFETY: place_window passes its live, exclusively borrowed WindowSearch.
    // EnumWindows invokes this callback synchronously and does not retain LPARAM.
    let search = unsafe { &mut *(param.0 as *mut WindowSearch) };
    let mut pid = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    if pid == search.pid && unsafe { IsWindowVisible(hwnd) }.as_bool() {
        let mut r = RECT::default();
        if unsafe { GetClientRect(hwnd, &mut r) }.is_ok() {
            let area = i64::from(r.right - r.left) * i64::from(r.bottom - r.top);
            if area > 250_000 && search.best.is_none_or(|(a, _)| area > a) {
                search.best = Some((area, hwnd));
            }
        }
    }
    true.into()
}
pub fn place_window(pid: u32) -> Result<bool, String> {
    let primary = list()?
        .into_iter()
        .find(|d| d.primary && d.active)
        .ok_or("No primary display")?;
    let mut search = WindowSearch { pid, best: None };
    // SAFETY: search lives until synchronous enumeration returns; the callback
    // does not retain its pointer. No concurrent access occurs during enumeration.
    unsafe {
        EnumWindows(
            Some(window_callback),
            LPARAM(&mut search as *mut _ as isize),
        )
    }
    .map_err(|e| e.to_string())?;
    let Some((_, hwnd)) = search.best else {
        return Ok(false);
    };
    // SAFETY: hwnd was selected from visible windows owned by the DCS process;
    // these APIs validate a window that disappears between enumeration and use.
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        SetWindowLongPtrW(hwnd, GWL_STYLE, style & !(WS_OVERLAPPEDWINDOW.0 as isize));
        SetWindowPos(
            hwnd,
            None,
            primary.x,
            primary.y,
            primary.width as i32 + 1024,
            primary.height.max(512) as i32,
            SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(true)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn display_edges_may_touch_but_not_overlap() {
        let d = Display {
            device: String::new(),
            description: String::new(),
            active: true,
            primary: false,
            x: 2048,
            y: 0,
            width: 1024,
            height: 512,
            frequency: 60,
        };
        assert!(!overlaps(1024, 0, 1024, 512, &d));
        assert!(overlaps(1025, 0, 1024, 512, &d));
        assert!(!overlaps(2048, -512, 1024, 512, &d));
    }
    #[test]
    #[ignore = "requires an installed, inactive virtual display; temporarily enables it"]
    fn virtual_display_roundtrip() {
        assert!(!crate::session::dcs_running().unwrap(), "Close DCS first");
        let dir = crate::session::data_dir().unwrap();
        let _lock = crate::session::lock(&dir).unwrap();
        let recovery = dir.join("display-recovery.json");
        assert!(!recovery.exists(), "Recover previous session first");
        let before = list().unwrap();
        let candidates: Vec<_> = before
            .iter()
            .filter(|d| !d.primary && !d.active && d.description.to_lowercase().contains("virtual"))
            .collect();
        assert_eq!(
            candidates.len(),
            1,
            "Requires exactly one inactive virtual display"
        );
        let saved = candidates[0].json();
        let device = candidates[0].device.clone();
        struct Restore(Value);
        impl Drop for Restore {
            fn drop(&mut self) {
                if let Err(e) = restore(&self.0) {
                    eprintln!("Display restore failed: {e}");
                }
            }
        }
        let _restore = Restore(saved.clone());
        // The same recovery record used by the launcher survives forced test termination.
        let primary = before.iter().find(|d| d.primary && d.active).unwrap();
        let mut expected = saved.clone();
        expected["active"] = json!(true);
        expected["x"] = json!(primary.x + primary.width as i32);
        expected["y"] = json!(primary.y);
        expected["width"] = json!(1024);
        expected["height"] = json!(512);
        crate::profile::write_json(&recovery, &json!({"before":saved,"after":expected})).unwrap();
        configure(&device, true).unwrap();
        let config = capture_config(&device, 18093).unwrap();
        let index = config["capture_monitor_index"].as_u64().unwrap() as usize;
        assert_eq!(
            Monitor::from_index(index).unwrap().device_name().unwrap(),
            device
        );
        assert_eq!(config["capture_monitor_device"], device);
        restore(&saved).unwrap();
        let after = list().unwrap();
        assert!(!selected(&device).unwrap().active);
        assert_eq!(
            before.iter().find(|d| d.primary),
            after.iter().find(|d| d.primary)
        );
        std::fs::remove_file(recovery).unwrap();
    }
}
