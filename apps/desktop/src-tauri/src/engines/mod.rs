use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use serde::{Deserialize, Serialize};

pub mod stt_whisper;
pub mod tts_sherpa;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SttOptions {
    pub language: Option<String>,
    pub translate: bool,
    pub threads: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transcript {
    pub text: String,
    pub segments: Vec<TranscriptSegment>,
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptSegment {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TtsOptions {
    pub voice_id: String,
    pub speed: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Voice {
    pub id: String,
    pub name: String,
    pub language: String,
    pub gender: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AudioBuffer {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

/// Progress reporting + cooperative cancellation handed to engines.
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
}

pub trait SttEngine: Send + Sync {
    fn transcribe(&self, samples: &[f32], opts: &SttOptions, ctl: &JobControl) -> Result<Transcript>;
    fn model_id(&self) -> &str;
}

pub trait TtsEngine: Send + Sync {
    fn voices(&self) -> Vec<Voice>;
    fn synthesize(&self, text: &str, opts: &TtsOptions, ctl: &JobControl) -> Result<AudioBuffer>;
    fn model_id(&self) -> &str;
}

/// Caches the most recently used STT and TTS engine so models are not reloaded per request.
#[derive(Default)]
pub struct EngineRegistry {
    stt: Option<Arc<dyn SttEngine>>,
    tts: Option<Arc<dyn TtsEngine>>,
}

impl EngineRegistry {
    pub fn get_stt(&self, model_id: &str) -> Option<Arc<dyn SttEngine>> {
        self.stt.as_ref().filter(|e| e.model_id() == model_id).cloned()
    }

    pub fn get_tts(&self, model_id: &str) -> Option<Arc<dyn TtsEngine>> {
        self.tts.as_ref().filter(|e| e.model_id() == model_id).cloned()
    }

    pub fn set_stt(&mut self, engine: Arc<dyn SttEngine>) {
        self.stt = Some(engine);
    }

    pub fn set_tts(&mut self, engine: Arc<dyn TtsEngine>) {
        self.tts = Some(engine);
    }

    /// Drop any cached engine for `model_id` (e.g. before deleting its files).
    pub fn evict(&mut self, model_id: &str) {
        if self.stt.as_ref().is_some_and(|e| e.model_id() == model_id) {
            self.stt = None;
        }
        if self.tts.as_ref().is_some_and(|e| e.model_id() == model_id) {
            self.tts = None;
        }
    }
}

pub use crate::error::{AppError, Result};
