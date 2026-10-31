//! One-shot failure injection for isolated developer tests. Production builds
//! compile out file access; commands never modify DCS profiles or display state.
use crate::config::Config;

pub fn take(cfg: &Config, name: &str) -> bool {
    #[cfg(feature = "test-hooks")]
    if let Some(path) = &cfg.test_fault_file {
        if std::fs::read_to_string(path).ok().as_deref() == Some(name) {
            return std::fs::remove_file(path).is_ok();
        }
    }
    let _ = (cfg, name);
    false
}
