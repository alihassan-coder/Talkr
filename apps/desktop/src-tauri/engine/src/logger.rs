//! The engine logs to stderr, which the app forwards into talkr.log. whisper.cpp and ggml report
//! allocation failures only through their log, so the last error line is also kept to explain a
//! failed model load (see `last_error`).

use std::io::Write;
use std::sync::Mutex;
use log::{Level, LevelFilter, Log, Metadata, Record};

static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);

struct StderrLogger;

impl Log for StderrLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        // whisper.cpp narrates every model load at info level; keep warnings and errors from it.
        metadata.level() <= Level::Warn || !metadata.target().starts_with("whisper_rs")
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let message = record.args().to_string();
        let message = message.trim_end();
        if record.level() == Level::Error {
            *LAST_ERROR.lock().unwrap_or_else(|e| e.into_inner()) = Some(message.to_string());
        }
        // A closed stderr (the app went away) is not worth failing over.
        let _ = writeln!(std::io::stderr().lock(), "{} {}", record.level(), message);
    }

    fn flush(&self) {}
}

pub fn init() {
    if log::set_logger(&StderrLogger).is_ok() {
        log::set_max_level(LevelFilter::Info);
    }
    whisper_rs::install_logging_hooks();
}

/// Take the most recent error-level log line, clearing it.
pub fn take_last_error() -> Option<String> {
    LAST_ERROR.lock().unwrap_or_else(|e| e.into_inner()).take()
}

/// Whether a native error message is an allocation failure.
pub fn is_allocation_failure(message: &str) -> bool {
    let m = message.to_ascii_lowercase();
    m.contains("failed to allocate")
        || m.contains("out of memory")
        // Vulkan: ErrorOutOfDeviceMemory / ErrorOutOfHostMemory
        || m.contains("outofdevicememory")
        || m.contains("outofhostmemory")
        || m.contains("bad_alloc")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_allocation_failures() {
        assert!(is_allocation_failure("whisper_kv_cache_init: failed to allocate memory for the kv cache"));
        assert!(is_allocation_failure("ggml_vulkan: Device memory allocation of size 1 failed: ErrorOutOfDeviceMemory"));
        assert!(is_allocation_failure("std::bad_alloc"));
        assert!(!is_allocation_failure("failed to encode"));
    }
}
