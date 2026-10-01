//! Talkr's engine process: runs whisper.cpp (speech to text) and sherpa-onnx (text to speech)
//! for the app, answering requests from `talkr_protocol` over stdin/stdout. See `worker`.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
///
/// Every progress report and cancellation check also counts as a sign of life (see
/// [`JobControl::alive`]): the worker's heartbeat only vouches for a job whose engine keeps
/// calling back, so native code that truly hangs still trips the app's watchdog.
#[derive(Clone)]
pub struct JobControl {
    progress: Arc<dyn Fn(f32) + Send + Sync>,
    cancel: Arc<AtomicBool>,
    alive: Arc<AtomicU64>,
}

impl JobControl {
    pub fn new(progress: Arc<dyn Fn(f32) + Send + Sync>, cancel: Arc<AtomicBool>) -> Self {
        Self { progress, cancel, alive: Arc::default() }
    }

    pub fn progress(&self, p: f32) {
        self.touch();
        (self.progress)(p.clamp(0.0, 1.0));
    }

    pub fn is_cancelled(&self) -> bool {
        self.touch();
        self.cancel.load(Ordering::Relaxed)
    }

    /// The raw cancel flag, for native engines that poll it from C.
    pub fn cancel_flag(&self) -> Arc<AtomicBool> {
        self.cancel.clone()
    }

    /// A counter that goes up whenever the engine calls back (progress, cancellation checks, or
    /// native code bumping it directly). It only ever grows while the job is making headway.
    pub fn alive(&self) -> Arc<AtomicU64> {
        self.alive.clone()
    }

    fn touch(&self) {
        self.alive.fetch_add(1, Ordering::Relaxed);
    }

    pub fn check_cancelled(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(EngineError::Cancelled)
        } else {
            Ok(())
        }
    }
}
