//! Validated streaming configuration. Desktop rectangles use physical pixels;
//! crop offsets are converted to monitor-local coordinates only in capture::combine.
use serde_json::Value;
use std::{fs, path::Path};

#[derive(Clone, Debug)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub width: usize,
    pub height: usize,
}

impl Rect {
    fn read(value: &Value) -> Result<Self, String> {
        let signed = |name| {
            value[name]
                .as_i64()
                .and_then(|v| i32::try_from(v).ok())
                .ok_or_else(|| format!("Invalid {name}"))
        };
        let dimension = |name| {
            value[name]
                .as_u64()
                .filter(|v| (1..=4096).contains(v))
                .map(|v| v as usize)
                .ok_or_else(|| format!("Invalid {name}"))
        };
        Ok(Self {
            left: signed("left")?,
            top: signed("top")?,
            width: dimension("width")?,
            height: dimension("height")?,
        })
    }

    pub fn inside(&self, outer: &Self) -> bool {
        i64::from(self.left) >= i64::from(outer.left)
            && i64::from(self.top) >= i64::from(outer.top)
            && i64::from(self.left) + self.width as i64
                <= i64::from(outer.left) + outer.width as i64
            && i64::from(self.top) + self.height as i64
                <= i64::from(outer.top) + outer.height as i64
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub fps: u32,
    pub size: usize,
    pub monitor_index: usize,
    pub monitor_device: Option<String>,
    pub monitor: Rect,
    pub left: Rect,
    pub right: Rect,
    pub demo: bool,
    pub bitrate: u32,
    #[cfg(feature = "test-hooks")]
    pub test_fault_file: Option<std::path::PathBuf>,
}

impl Config {
    pub fn demo(port: Option<u16>, bitrate: u32) -> Result<Self, String> {
        Self::parse(
            &serde_json::json!({"port":8090,"fps":30,"size":512}),
            true,
            port,
            bitrate,
        )
    }
    pub fn load(path: &Path, demo: bool, port: Option<u16>, bitrate: u32) -> Result<Self, String> {
        let value: Value = serde_json::from_str(
            &fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?,
        )
        .map_err(|e| e.to_string())?;
        Self::parse(&value, demo, port, bitrate)
    }

    fn parse(
        value: &Value,
        demo: bool,
        override_port: Option<u16>,
        bitrate: u32,
    ) -> Result<Self, String> {
        let port = override_port
            .map(u64::from)
            .or_else(|| value["port"].as_u64())
            .filter(|v| (1024..=65535).contains(v))
            .ok_or("port must be 1024..65535")? as u16;
        let fps = value["fps"]
            .as_u64()
            .filter(|v| (1..=60).contains(v))
            .ok_or("fps must be 1..60")? as u32;
        // Baseline Level 3.1 interoperability with the existing Safari/aiortc client.
        let size = value["size"]
            .as_u64()
            .filter(|v| (128..=512).contains(v) && v % 2 == 0)
            .ok_or("Rust size must be even and 128..512")? as usize;
        if (size * 2).div_ceil(16) * size.div_ceil(16) * fps as usize > 108_000 {
            return Err("Size and frame rate exceed H.264 Level 3.1; use 512px at 30 fps or a smaller frame".into());
        }
        if !(100_000..=10_000_000).contains(&bitrate) {
            return Err("bitrate must be 100000..10000000".into());
        }
        let (monitor_index, monitor, left, right) = if demo {
            (
                0,
                Rect {
                    left: 0,
                    top: 0,
                    width: size * 2,
                    height: size,
                },
                Rect {
                    left: 0,
                    top: 0,
                    width: size,
                    height: size,
                },
                Rect {
                    left: size as i32,
                    top: 0,
                    width: size,
                    height: size,
                },
            )
        } else {
            if value["capture_backend"] != "wgc" {
                return Err(
                    "Rust capture requires the generated Windows Graphics Capture configuration"
                        .into(),
                );
            }
            let index = value["capture_monitor_index"]
                .as_u64()
                .filter(|v| (1..=16).contains(v))
                .ok_or("Invalid capture_monitor_index")? as usize;
            let monitor = Rect::read(&value["capture_monitor_rect"])?;
            let left = Rect::read(&value["left"])?;
            let right = Rect::read(&value["right"])?;
            if !left.inside(&monitor) || !right.inside(&monitor) {
                return Err("DDI rectangle is outside its capture monitor".into());
            }
            (index, monitor, left, right)
        };
        Ok(Self {
            port,
            fps,
            size,
            monitor_device: value["capture_monitor_device"].as_str().map(str::to_owned),
            monitor_index,
            monitor,
            left,
            right,
            demo,
            bitrate,
            #[cfg(feature = "test-hooks")]
            test_fault_file: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rejects_outside_rectangle_and_invalid_size() {
        let mut cfg = json!({"port":8092,"fps":30,"size":512,"capture_backend":"wgc","capture_monitor_index":2,
            "capture_monitor_rect":{"left":4096,"top":0,"width":1024,"height":512},
            "left":{"left":4096,"top":0,"width":512,"height":512},
            "right":{"left":4608,"top":0,"width":512,"height":512}});
        assert!(Config::parse(&cfg, false, None, 1_000_000).is_ok());
        cfg["right"]["left"] = json!(4609);
        assert!(Config::parse(&cfg, false, None, 1_000_000).is_err());
        cfg["size"] = json!(513);
        assert!(Config::parse(&cfg, true, None, 1_000_000).is_err());
    }
}
