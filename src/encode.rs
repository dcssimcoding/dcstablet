//! A viewer-owned capture/encoder pipeline. The encoder creates and destroys
//! its COM runtime on its own thread; cancellation wakes pacing waits before join.
//! Raw frames are sampled after pacing; encoded output uses a bounded IDR-aware FIFO.
use crate::mf_encode::{Encoder, bgra_to_nv12, has_nal};
use crate::{
    capture::{Source, combine},
    config::Config,
    faults,
    state::{Cancellation, EncodedFrame, EncodedQueue, Latest, RawFrame, Stage, State},
    workers,
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

pub struct Pipeline {
    source: Source,
    encoder: Option<JoinHandle<()>>,
    state: Arc<State>,
    cancel: Arc<Cancellation>,
    pub keyframe: Arc<AtomicBool>,
    pub encoded: Arc<EncodedQueue>,
}
impl Pipeline {
    pub fn start(cfg: Arc<Config>, state: Arc<State>) -> Result<Self, String> {
        let frames = Arc::new(Latest::<RawFrame>::default());
        let encoded = Arc::new(EncodedQueue::default());
        let cancel = Arc::new(Cancellation::default());
        let keyframe = Arc::new(AtomicBool::new(true));
        let source = Source::start(cfg.clone(), frames.clone(), state.clone(), cancel.clone())?;
        let (worker_cancel, worker_keyframe, worker_encoded, worker_state) = (
            cancel.clone(),
            keyframe.clone(),
            encoded.clone(),
            state.clone(),
        );
        let encoder = workers::spawn(
            "ddi-encode",
            Stage::Encode,
            state.clone(),
            cancel.clone(),
            move || {
                encode_loop(
                    &cfg,
                    &frames,
                    &worker_encoded,
                    &worker_cancel,
                    &worker_keyframe,
                    &worker_state,
                )
            },
        )
        .map_err(|e| {
            state.fail(
                Stage::Encode,
                format!("Could not start encoder worker: {e}"),
            );
            e.to_string()
        })?;
        Ok(Self {
            source,
            encoder: Some(encoder),
            state,
            cancel,
            keyframe,
            encoded,
        })
    }
    pub fn check_health(&mut self) -> Result<(), String> {
        if !self.cancel.is_cancelled() {
            if let Some(error) = self.source.poll_failure() {
                self.state.fail(Stage::Capture, error);
                self.cancel.cancel();
            }
            if self.encoder.as_ref().is_some_and(|join| join.is_finished())
                && !self.cancel.is_cancelled()
            {
                self.state
                    .fail(Stage::Encode, "Encoder worker exited unexpectedly");
                self.cancel.cancel();
            }
        }
        if self.cancel.is_cancelled() {
            Err(self
                .state
                .failure_text()
                .unwrap_or_else(|| "Pipeline stopped".into()))
        } else {
            Ok(())
        }
    }
    pub fn shutdown(&mut self) {
        self.cancel.cancel();
        self.source.shutdown();
        if let Some(join) = self.encoder.take()
            && let Err(panic) = join.join()
        {
            self.state.fail(Stage::Shutdown, workers::panic_text(panic));
        }
    }
}
impl Drop for Pipeline {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn sample_after_wait<T>(frames: &Latest<T>, wait: impl FnOnce() -> bool) -> Option<Arc<T>> {
    if wait() { None } else { frames.get() }
}

fn encode_loop(
    cfg: &Config,
    frames: &Latest<RawFrame>,
    output: &EncodedQueue,
    cancel: &Cancellation,
    keyframe: &AtomicBool,
    state: &State,
) -> Result<(), String> {
    let mut encoder = Encoder::new(cfg.size * 2, cfg.size, cfg.fps, cfg.bitrate)
        .map_err(|e| format!("Windows H.264 encoder unavailable: {e}. Install the Media Feature Pack on Windows N editions."))?;
    let mut yuv = Vec::new();
    let mut combined = Vec::with_capacity(cfg.size * cfg.size * 8);
    let origin = Instant::now();
    let period = Duration::from_secs_f64(1.0 / f64::from(cfg.fps));
    let mut next = origin;
    let mut sequence = 0;
    let mut last_source = None;
    while !cancel.is_cancelled() {
        // Pace first, then read the newest frame. Never hold an old frame during the wait.
        let Some(frame) = sample_after_wait(frames, || cancel.wait_until(next)) else {
            if cancel.is_cancelled() {
                break;
            }
            if origin.elapsed() > Duration::from_secs(8) {
                return Err("No frames from the configured monitor".into());
            }
            cancel.wait_until(Instant::now() + Duration::from_millis(5));
            continue;
        };
        if faults::take(cfg, "encoder-error") {
            return Err("Injected encoder failure".into());
        }
        if faults::take(cfg, "encoder-panic") {
            panic!("Injected encoder panic");
        }
        if let Some(previous) = last_source
            && frame.sequence > previous + 1
        {
            state
                .skipped_source
                .fetch_add(frame.sequence - previous - 1, Ordering::Relaxed);
        }
        last_source = Some(frame.sequence);
        state.source_age.add(frame.captured_at.elapsed());
        let prepare = Instant::now();
        combine(&frame, cfg, &mut combined)?;
        bgra_to_nv12(&combined, cfg.size * 2, cfg.size, &mut yuv)?;
        state.prepare_time.add(prepare.elapsed());
        let force_keyframe = keyframe.swap(false, Ordering::Relaxed);
        let at = Instant::now();
        let bytes: Arc<[u8]> = encoder
            .encode(
                &yuv,
                origin.elapsed().as_nanos() as i64 / 100,
                force_keyframe,
            )
            .map_err(|e| e.to_string())?
            .into();
        state.encode_time.add(at.elapsed());
        if !bytes.is_empty() {
            sequence += 1;
            let accepted = output.put(EncodedFrame {
                keyframe: has_nal(&bytes, 5),
                bytes,
                sequence,
                at,
                source_at: frame.captured_at,
            });
            if !accepted {
                keyframe.store(true, Ordering::Relaxed);
            }
            state.encoded.fetch_add(1, Ordering::Relaxed);
        } else {
            state.skipped_codec.fetch_add(1, Ordering::Relaxed);
        }
        next += period;
        if next < Instant::now() {
            next = Instant::now() + period;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn samples_frame_after_pacing_not_before() {
        let latest = Latest::default();
        latest.put(1);
        let selected = sample_after_wait(&latest, || {
            latest.put(2);
            false
        });
        assert_eq!(*selected.unwrap(), 2);
    }
    #[test]
    fn cancelled_wait_never_returns_an_old_frame() {
        let latest = Latest::default();
        latest.put(1);
        assert!(sample_after_wait(&latest, || true).is_none());
    }
}
