//! Shared session diagnostics and bounded frame storage.
//! Raw capture keeps only the newest image. Encoded frames preserve decoding order
//! until overload, when dependent frames are suppressed until a new keyframe.
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    sync::{
        Arc, Condvar, Mutex, MutexGuard,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

// These mutexes protect only internal scalar/collection updates. No application callbacks
// or destructors execute under their guards, so recovery preserves their invariants.
pub fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poison) => {
            eprintln!("Recovering an internal poisoned mutex");
            mutex.clear_poison();
            poison.into_inner()
        }
    }
}

#[derive(Default)]
pub struct Cancellation {
    stopped: AtomicBool,
    gate: Mutex<()>,
    wake: Condvar,
}
impl Cancellation {
    pub fn cancel(&self) {
        let _guard = lock(&self.gate);
        self.stopped.store(true, Ordering::Release);
        self.wake.notify_all();
    }
    pub fn is_cancelled(&self) -> bool {
        self.stopped.load(Ordering::Acquire)
    }
    pub fn wait_until(&self, deadline: Instant) -> bool {
        let guard = lock(&self.gate);
        let _guard = self
            .wake
            .wait_timeout_while(
                guard,
                deadline.saturating_duration_since(Instant::now()),
                |_| !self.is_cancelled(),
            )
            .unwrap_or_else(|poison| poison.into_inner());
        self.is_cancelled()
    }
}

pub struct Latest<T>(Mutex<Option<Arc<T>>>);
impl<T> Default for Latest<T> {
    fn default() -> Self {
        Self(Mutex::new(None))
    }
}
impl<T> Latest<T> {
    pub fn put(&self, value: T) {
        let value = Arc::new(value);
        let old = lock(&self.0).replace(value);
        drop(old);
    }
    pub fn get(&self) -> Option<Arc<T>> {
        lock(&self.0).clone()
    }
}
pub struct RawFrame {
    pub pixels: Vec<u8>,
    pub width: usize,
    pub height: usize,
    pub stride: usize,
    pub captured_at: Instant,
    pub sequence: u64,
}
pub struct EncodedFrame {
    pub keyframe: bool,
    pub bytes: Arc<[u8]>,
    pub sequence: u64,
    pub at: Instant,
    pub source_at: Instant,
}

/// A short FIFO of complete H.264 access units. Overflow invalidates the
/// reference chain; callers must request an IDR when `put` returns false.
#[derive(Default)]
pub struct EncodedQueue(Mutex<EncodedQueueInner>);
struct EncodedQueueInner {
    frames: VecDeque<Arc<EncodedFrame>>,
    needs_keyframe: bool,
}
impl Default for EncodedQueueInner {
    fn default() -> Self {
        Self {
            frames: VecDeque::new(),
            needs_keyframe: true,
        }
    }
}
impl EncodedQueue {
    pub fn put(&self, frame: EncodedFrame) -> bool {
        let mut queue = lock(&self.0);
        if queue.frames.len() >= 4 {
            queue.frames.clear();
            queue.needs_keyframe = true;
        }
        if queue.needs_keyframe && !frame.keyframe {
            return false;
        }
        queue.needs_keyframe = false;
        queue.frames.push_back(Arc::new(frame));
        true
    }
    pub fn take(&self) -> Option<Arc<EncodedFrame>> {
        lock(&self.0).frames.pop_front()
    }
}

#[derive(Default)]
pub struct Timings(Mutex<VecDeque<f64>>);
impl Timings {
    pub fn add(&self, elapsed: Duration) {
        let mut rows = lock(&self.0);
        if rows.len() >= 900 {
            rows.pop_front();
        }
        rows.push_back(elapsed.as_secs_f64() * 1000.0);
    }
    fn snapshot(&self) -> Value {
        let mut rows: Vec<_> = lock(&self.0).iter().copied().collect();
        rows.sort_by(f64::total_cmp);
        if rows.is_empty() {
            return json!(null);
        }
        json!({"count":rows.len(),"mean_ms":rows.iter().sum::<f64>()/rows.len() as f64,"p95_ms":rows[(rows.len()*95).div_ceil(100).saturating_sub(1)]})
    }
}
#[derive(Clone, Copy, Debug)]
pub enum Stage {
    Capture,
    Encode,
    Session,
    Http,
    Shutdown,
}
impl Stage {
    fn name(self) -> &'static str {
        match self {
            Self::Capture => "capture",
            Self::Encode => "encode",
            Self::Session => "session",
            Self::Http => "http",
            Self::Shutdown => "shutdown",
        }
    }
}

#[derive(Clone)]
struct Failure {
    stage: Stage,
    message: String,
    at: u128,
    since: Instant,
}
impl Failure {
    fn json(&self) -> Value {
        json!({"stage":self.stage.name(),"message":self.message,"unix_ms":self.at,"age_seconds":self.since.elapsed().as_secs_f64()})
    }
}
#[derive(Default)]
struct Failures {
    current: Option<Failure>,
    history: VecDeque<Failure>,
}

pub struct State {
    pub active: AtomicBool,
    capture_active: AtomicBool,
    capture_progress: Mutex<Option<Instant>>,
    pub captured: AtomicU64,
    pub encoded: AtomicU64,
    pub sent: AtomicU64,
    pub panics: AtomicU64,
    pub sessions: AtomicU64,
    pub failed_sessions: AtomicU64,
    pub skipped_source: AtomicU64,
    pub skipped_encoded: AtomicU64,
    pub skipped_codec: AtomicU64,
    pub capture_time: Timings,
    pub prepare_time: Timings,
    pub encode_time: Timings,
    pub source_age: Timings,
    pub encode_to_send: Timings,
    pub source_to_send: Timings,
    failures: Mutex<Failures>,
    pub started: Instant,
}
impl Default for State {
    fn default() -> Self {
        Self {
            active: AtomicBool::new(false),
            capture_active: AtomicBool::new(false),
            capture_progress: Mutex::new(None),
            captured: AtomicU64::new(0),
            encoded: AtomicU64::new(0),
            sent: AtomicU64::new(0),
            panics: AtomicU64::new(0),
            sessions: AtomicU64::new(0),
            failed_sessions: AtomicU64::new(0),
            skipped_source: AtomicU64::new(0),
            skipped_encoded: AtomicU64::new(0),
            skipped_codec: AtomicU64::new(0),
            capture_time: Timings::default(),
            prepare_time: Timings::default(),
            encode_time: Timings::default(),
            source_age: Timings::default(),
            encode_to_send: Timings::default(),
            source_to_send: Timings::default(),
            failures: Mutex::new(Failures::default()),
            started: Instant::now(),
        }
    }
}
impl State {
    pub fn capture_started(&self) {
        *lock(&self.capture_progress) = None;
        self.capture_active.store(true, Ordering::Release);
    }
    pub fn capture_stopped(&self) {
        self.capture_active.store(false, Ordering::Release);
    }
    pub fn capture_arrived(&self) {
        *lock(&self.capture_progress) = Some(Instant::now());
    }
    pub fn capture_status(&self) -> Value {
        self.capture_status_at(Instant::now())
    }
    fn capture_status_at(&self, now: Instant) -> Value {
        let age = lock(&self.capture_progress).map(|last| now.saturating_duration_since(last));
        let status = if !self.capture_active.load(Ordering::Acquire) {
            "idle"
        } else {
            match age {
                None => "waiting",
                Some(t) if t > Duration::from_secs(8) => "stale",
                _ => "fresh",
            }
        };
        json!({"state":status,"age_ms":age.map(|t|t.as_millis()),"stale_after_ms":8000})
    }
    pub fn begin_session(&self) {
        lock(&self.failures).current = None;
        self.sessions.fetch_add(1, Ordering::Relaxed);
    }
    pub fn fail(&self, stage: Stage, error: impl ToString) {
        let message = error.to_string();
        eprintln!("{:?}: {message}", stage);
        let failure = Failure {
            stage,
            message,
            since: Instant::now(),
            at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
        };
        let mut failures = lock(&self.failures);
        if failures.current.is_none() {
            failures.current = Some(failure.clone());
            self.failed_sessions.fetch_add(1, Ordering::Relaxed);
        }
        if failures.history.len() >= 16 {
            failures.history.pop_front();
        }
        failures.history.push_back(failure);
    }
    pub fn failure_details(&self) -> Option<Value> {
        lock(&self.failures).current.as_ref().map(Failure::json)
    }
    pub fn failure_text(&self) -> Option<String> {
        lock(&self.failures)
            .current
            .as_ref()
            .map(|f| f.message.clone())
    }
    pub fn snapshot(&self) -> Value {
        let failures = lock(&self.failures);
        let count = |v: &AtomicU64| v.load(Ordering::Relaxed);
        json!({"backend":"rust","codec":"Media Foundation H.264","uptime_seconds":self.started.elapsed().as_secs_f64(),
            "active_viewers":u8::from(self.active.load(Ordering::Acquire)),"captured_frames":count(&self.captured),
            "encoded_frames":count(&self.encoded),"sent_frames":count(&self.sent),"worker_panics":count(&self.panics),
            "sessions_started":count(&self.sessions),"failed_sessions":count(&self.failed_sessions),
            "source_frames_skipped":count(&self.skipped_source),"encoded_frames_skipped":count(&self.skipped_encoded),"codec_frames_skipped":count(&self.skipped_codec),
            "capture":self.capture_time.snapshot(),"prepare":self.prepare_time.snapshot(),"encode":self.encode_time.snapshot(),
            "source_callback_age":self.source_age.snapshot(),"encode_to_send":self.encode_to_send.snapshot(),"source_callback_to_send":self.source_to_send.snapshot(),
            "last_error":failures.current.as_ref().map(|f|f.message.clone()),"failure":failures.current.as_ref().map(Failure::json),
            "capture_source":self.capture_status(),"failure_history":failures.history.iter().map(Failure::json).collect::<Vec<_>>()})
    }
}
pub struct Lease(pub Arc<State>);
impl Drop for Lease {
    fn drop(&mut self) {
        self.0.active.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encoded_overflow_waits_for_a_new_keyframe() {
        let queue = EncodedQueue::default();
        let frame = |sequence, keyframe| EncodedFrame {
            sequence,
            keyframe,
            bytes: Arc::from([1u8]),
            at: Instant::now(),
            source_at: Instant::now(),
        };
        assert!(!queue.put(frame(1, false)));
        assert!(queue.put(frame(2, true)));
        assert!(queue.put(frame(3, false)));
        assert_eq!(queue.take().unwrap().sequence, 2);
        assert!(queue.put(frame(4, false)));
        assert!(queue.put(frame(5, false)));
        assert!(queue.put(frame(6, false)));
        assert!(!queue.put(frame(7, false)));
        assert!(queue.take().is_none());
        assert!(!queue.put(frame(8, false)));
        assert!(queue.put(frame(9, true)));
        assert_eq!(queue.take().unwrap().sequence, 9);
    }
    #[test]
    fn capture_freshness_is_separate_from_service_liveness() {
        let state = State::default();
        assert_eq!(state.capture_status()["state"], "idle");
        state.capture_started();
        assert_eq!(state.capture_status()["state"], "waiting");
        state.capture_arrived();
        assert_eq!(state.capture_status()["state"], "fresh");
        assert_eq!(
            state.capture_status_at(Instant::now() + Duration::from_secs(9))["state"],
            "stale"
        );
        state.capture_arrived();
        assert_eq!(state.capture_status()["state"], "fresh");
        state.capture_stopped();
        assert_eq!(state.capture_status()["state"], "idle");
    }
    #[test]
    fn cancellation_wakes_sleeping_worker() {
        let cancel = Arc::new(Cancellation::default());
        let worker_cancel = cancel.clone();
        let start = Instant::now();
        let join = std::thread::spawn(move || {
            worker_cancel.wait_until(Instant::now() + Duration::from_secs(10))
        });
        cancel.cancel();
        cancel.cancel();
        assert!(join.join().unwrap());
        assert!(start.elapsed() < Duration::from_secs(1));
    }
    #[test]
    fn history_survives_new_session_but_current_error_clears() {
        let state = State::default();
        state.fail(Stage::Capture, "original fault");
        state.fail(Stage::Session, "secondary error");
        assert_eq!(state.snapshot()["last_error"], "original fault");
        state.begin_session();
        assert!(state.snapshot()["last_error"].is_null());
        assert_eq!(
            state.snapshot()["failure_history"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }
}
