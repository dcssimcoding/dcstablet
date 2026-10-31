//! Capture owns the Windows Graphics Capture worker (or synthetic demo thread).
//! Callbacks publish raw images without waiting for encoding. Shutdown cancels
//! the source and joins it before the pipeline releases shared frame storage.
use crate::{
    config::Config,
    faults,
    state::{Cancellation, Latest, RawFrame, Stage, State},
    workers,
};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, atomic::Ordering},
    thread::JoinHandle,
    time::{Duration, Instant},
};
use windows_capture::{
    capture::{CaptureControl, Context, GraphicsCaptureApiHandler},
    frame::Frame,
    graphics_capture_api::InternalCaptureControl,
    monitor::Monitor,
    settings::{
        ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
        MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
    },
};

#[derive(Clone)]
struct Flags {
    cfg: Arc<Config>,
    frames: Arc<Latest<RawFrame>>,
    state: Arc<State>,
    cancel: Arc<Cancellation>,
}
struct Handler(Flags);
impl Handler {
    fn process_frame(
        &mut self,
        frame: &mut Frame,
        control: InternalCaptureControl,
    ) -> Result<(), String> {
        if self.0.cancel.is_cancelled() {
            control.stop();
            return Ok(());
        }
        if faults::take(&self.0.cfg, "capture-error") {
            return Err("Injected capture-device failure".into());
        }
        if faults::take(&self.0.cfg, "capture-panic") {
            panic!("Injected capture panic")
        }
        let started = Instant::now();
        let mut buffer = frame.buffer().map_err(|e| e.to_string())?;
        let width = buffer.width() as usize;
        let height = buffer.height() as usize;
        if width != self.0.cfg.monitor.width || height != self.0.cfg.monitor.height {
            return Err("Capture monitor dimensions changed. Restart the DDI launcher.".into());
        }
        let stride = buffer.row_pitch() as usize;
        let pixels = buffer.as_raw_buffer().to_vec();
        let sequence = self.0.state.captured.fetch_add(1, Ordering::Relaxed) + 1;
        self.0.state.capture_arrived();
        self.0.frames.put(RawFrame {
            pixels,
            width,
            height,
            stride,
            captured_at: started,
            sequence,
        });
        self.0.state.capture_time.add(started.elapsed());
        Ok(())
    }
}
impl GraphicsCaptureApiHandler for Handler {
    type Flags = Flags;
    type Error = String;
    fn new(ctx: Context<Flags>) -> Result<Self, String> {
        Ok(Self(ctx.flags))
    }
    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        control: InternalCaptureControl,
    ) -> Result<(), String> {
        let result = match catch_unwind(AssertUnwindSafe(|| self.process_frame(frame, control))) {
            Ok(result) => result,
            Err(panic) => {
                self.0.state.panics.fetch_add(1, Ordering::Relaxed);
                Err(format!("Rust panic: {}", workers::panic_text(panic)))
            }
        };
        if let Err(error) = &result {
            self.0.state.fail(Stage::Capture, error);
            self.0.cancel.cancel();
        }
        result
    }
    fn on_closed(&mut self) -> Result<(), String> {
        if !self.0.cancel.is_cancelled() {
            self.0
                .state
                .fail(Stage::Capture, "Virtual monitor capture closed");
            self.0.cancel.cancel();
        }
        Ok(())
    }
}

pub struct Source {
    desktop: Option<CaptureControl<Handler, String>>,
    demo: Option<JoinHandle<()>>,
    cancel: Arc<Cancellation>,
    state: Arc<State>,
}
impl Source {
    pub fn start(
        cfg: Arc<Config>,
        frames: Arc<Latest<RawFrame>>,
        state: Arc<State>,
        cancel: Arc<Cancellation>,
    ) -> Result<Self, String> {
        if cfg.demo {
            let (worker_cancel, worker_state) = (cancel.clone(), state.clone());
            let join = workers::spawn(
                "ddi-demo",
                Stage::Capture,
                state.clone(),
                cancel.clone(),
                move || {
                    let period = Duration::from_secs_f64(1.0 / f64::from(cfg.fps));
                    let mut next = Instant::now();
                    while !worker_cancel.is_cancelled() {
                        if faults::take(&cfg, "capture-error") {
                            return Err("Injected capture-device failure".into());
                        }
                        if faults::take(&cfg, "capture-panic") {
                            panic!("Injected capture panic")
                        }
                        if faults::take(&cfg, "capture-exit") {
                            return Ok(());
                        }
                        if faults::take(&cfg, "capture-stall")
                            && worker_cancel.wait_until(Instant::now() + Duration::from_secs(12))
                        {
                            break;
                        }
                        let sequence = worker_state.captured.fetch_add(1, Ordering::Relaxed) + 1;
                        worker_state.capture_arrived();
                        frames.put(demo_frame(cfg.size, sequence as usize));
                        next += period;
                        if worker_cancel.wait_until(next) {
                            break;
                        }
                    }
                    Ok(())
                },
            )
            .map_err(|e| e.to_string())?;
            state.capture_started();
            return Ok(Self {
                desktop: None,
                demo: Some(join),
                cancel,
                state,
            });
        }
        let monitor = Monitor::from_index(cfg.monitor_index).map_err(|e| e.to_string())?;
        if cfg
            .monitor_device
            .as_ref()
            .is_some_and(|name| monitor.device_name().ok().as_ref() != Some(name))
        {
            return Err(
                "Capture display identity changed. Close DCS and relaunch DCS Tablet.".into(),
            );
        }
        if monitor.width().map_err(|e| e.to_string())? as usize != cfg.monitor.width
            || monitor.height().map_err(|e| e.to_string())? as usize != cfg.monitor.height
        {
            return Err("Configured virtual monitor does not match its actual dimensions".into());
        }
        let flags = Flags {
            cfg,
            frames,
            state: state.clone(),
            cancel: cancel.clone(),
        };
        let settings = Settings::new(
            monitor,
            CursorCaptureSettings::WithoutCursor,
            DrawBorderSettings::WithoutBorder,
            SecondaryWindowSettings::Default,
            MinimumUpdateIntervalSettings::Default,
            DirtyRegionSettings::Default,
            ColorFormat::Bgra8,
            flags,
        );
        let control = Handler::start_free_threaded(settings).map_err(|e| e.to_string())?;
        state.capture_started();
        Ok(Self {
            desktop: Some(control),
            demo: None,
            cancel,
            state,
        })
    }
    pub fn poll_failure(&mut self) -> Option<String> {
        if self
            .desktop
            .as_ref()
            .is_some_and(|control| control.is_finished())
        {
            let control = self.desktop.take()?;
            return Some(match control.wait() {
                Ok(()) => "Capture worker exited unexpectedly".into(),
                Err(error) => error.to_string(),
            });
        }
        None
    }
    pub fn shutdown(&mut self) {
        self.state.capture_stopped();
        self.cancel.cancel();
        if let Some(control) = self.desktop.take() {
            match catch_unwind(AssertUnwindSafe(|| control.stop())) {
                Ok(Ok(())) => {}
                Ok(Err(error)) => self
                    .state
                    .fail(Stage::Shutdown, format!("Capture shutdown: {error}")),
                Err(panic) => self.state.fail(
                    Stage::Shutdown,
                    format!("Capture shutdown panic: {}", workers::panic_text(panic)),
                ),
            }
        }
        if let Some(join) = self.demo.take()
            && let Err(panic) = join.join()
        {
            self.state.fail(Stage::Shutdown, workers::panic_text(panic));
        }
    }
}
impl Drop for Source {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn demo_frame(size: usize, sequence: usize) -> RawFrame {
    let width = size * 2;
    let mut pixels = vec![0; width * size * 4];
    for y in 0..size {
        for x in 0..width {
            let i = (y * width + x) * 4;
            let color = if x < size {
                [0, 180, 0, 255]
            } else {
                [180, 40, 0, 255]
            };
            pixels[i..i + 4].copy_from_slice(&color);
            if y == sequence % size || x % size == size / 2 || y == size / 2 {
                pixels[i..i + 4].copy_from_slice(&[230, 255, 230, 255]);
            }
        }
    }
    RawFrame {
        pixels,
        width,
        height: size,
        stride: width * 4,
        captured_at: Instant::now(),
        sequence: sequence as u64,
    }
}

pub fn combine(frame: &RawFrame, cfg: &Config, output: &mut Vec<u8>) -> Result<(), String> {
    if frame.pixels.len() < frame.stride * frame.height || frame.stride < frame.width * 4 {
        return Err("Invalid capture buffer".into());
    }
    let n = cfg.size;
    output.resize(n * n * 8, 0);
    for (side, rect) in [&cfg.left, &cfg.right].into_iter().enumerate() {
        let sx = (rect.left - cfg.monitor.left) as usize;
        let sy = (rect.top - cfg.monitor.top) as usize;
        for y in 0..n {
            let source_y = sy + y * rect.height / n;
            if rect.width == n {
                let source = source_y * frame.stride + sx * 4;
                let target = (y * n * 2 + side * n) * 4;
                output[target..target + n * 4]
                    .copy_from_slice(&frame.pixels[source..source + n * 4]);
            } else {
                for x in 0..n {
                    let source = source_y * frame.stride + (sx + x * rect.width / n) * 4;
                    let target = (y * n * 2 + side * n + x) * 4;
                    output[target..target + 4].copy_from_slice(&frame.pixels[source..source + 4]);
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Rect;
    #[test]
    fn combines_padded_bgra_rows_in_requested_order() {
        let raw = RawFrame {
            pixels: vec![1, 2, 3, 255, 4, 5, 6, 255, 99, 99, 99, 99],
            width: 2,
            height: 1,
            stride: 12,
            captured_at: Instant::now(),
            sequence: 1,
        };
        let cfg = Config {
            port: 8090,
            fps: 30,
            size: 1,
            monitor_index: 1,
            monitor_device: None,
            demo: false,
            bitrate: 1_000_000,
            #[cfg(feature = "test-hooks")]
            test_fault_file: None,
            monitor: Rect {
                left: 10,
                top: 20,
                width: 2,
                height: 1,
            },
            left: Rect {
                left: 11,
                top: 20,
                width: 1,
                height: 1,
            },
            right: Rect {
                left: 10,
                top: 20,
                width: 1,
                height: 1,
            },
        };
        let mut output = vec![];
        combine(&raw, &cfg, &mut output).unwrap();
        assert_eq!(output, vec![4, 5, 6, 255, 1, 2, 3, 255]);
    }
}
