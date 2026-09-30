//! Talkr's engine process: runs whisper.cpp (speech to text) and sherpa-onnx (text to speech)
//! for the app, answering requests from `talkr_protocol` over stdin/stdout. See `worker`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub mod audio;
pub mod devices;
pub mod error;
pub mod logger;
pub mod stt_whisper;
pub mod tts_sherpa;
pub mod worker;

pub use error::{EngineError, Result};

/// Progress reporting and cooperative cancellation handed to engines.
#[derive(Clone)]
pub struct JobControl {
    progress: Arc<dyn Fn(f32) + Send + Sync>,
    cancel: Arc<AtomicBool>,
}

impl JobControl {
    pub fn new(progress: Arc<dyn Fn(f32) + Send + Sync>, cancel: Arc<AtomicBool>) -> Self {
        Self { progress, cancel }
    }

    pub fn progress(&self, p: f32) {
        (self.progress)(p.clamp(0.0, 1.0));
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// The raw cancel flag, for native engines that poll it from C.
    pub fn cancel_flag(&self) -> Arc<AtomicBool> {
        self.cancel.clone()
    }

    pub fn check_cancelled(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(EngineError::Cancelled)
        } else {
            Ok(())
        }
    }
}
