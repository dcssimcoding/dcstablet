//! Worker boundary: preserve the first root cause, cancel siblings on error or
//! panic, and let the owning pipeline perform joins outside shared-state locks.
use crate::state::{Cancellation, Stage, State};
use std::{
    any::Any,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
    thread::{self, JoinHandle},
};

pub fn panic_text(value: Box<dyn Any + Send>) -> String {
    if let Some(text) = value.downcast_ref::<&str>() {
        (*text).to_owned()
    } else if let Some(text) = value.downcast_ref::<String>() {
        text.clone()
    } else {
        "non-text Rust panic".to_owned()
    }
}

pub fn spawn(
    name: &str,
    stage: Stage,
    state: Arc<State>,
    cancel: Arc<Cancellation>,
    work: impl FnOnce() -> Result<(), String> + Send + 'static,
) -> std::io::Result<JoinHandle<()>> {
    thread::Builder::new().name(name.into()).spawn(move || {
        let failure = match catch_unwind(AssertUnwindSafe(work)) {
            Ok(Ok(())) if cancel.is_cancelled() => None,
            Ok(Ok(())) => Some("worker exited unexpectedly".to_owned()),
            Ok(Err(error)) => Some(error),
            Err(panic) => {
                state
                    .panics
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                Some(format!("Rust panic: {}", panic_text(panic)))
            }
        };
        if let Some(error) = failure {
            state.fail(stage, error);
            cancel.cancel();
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn panicking_worker_cancels_pipeline_and_preserves_root_cause() {
        let state = Arc::new(State::default());
        let cancel = Arc::new(Cancellation::default());
        spawn(
            "test-worker",
            Stage::Encode,
            state.clone(),
            cancel.clone(),
            || panic!("encoder fault"),
        )
        .unwrap()
        .join()
        .unwrap();
        assert!(cancel.is_cancelled());
        state.fail(Stage::Session, "pipeline stopped");
        let value = state.snapshot();
        assert_eq!(value["failure"]["stage"], "encode");
        assert!(
            value["last_error"]
                .as_str()
                .unwrap()
                .contains("encoder fault")
        );
        assert_eq!(value["worker_panics"], 1);
    }
    #[test]
    fn unexpected_normal_worker_exit_is_reported() {
        let state = Arc::new(State::default());
        let cancel = Arc::new(Cancellation::default());
        spawn(
            "test-worker",
            Stage::Capture,
            state.clone(),
            cancel.clone(),
            || Ok(()),
        )
        .unwrap()
        .join()
        .unwrap();
        assert!(cancel.is_cancelled());
        assert_eq!(state.snapshot()["failure"]["stage"], "capture");
    }
}
