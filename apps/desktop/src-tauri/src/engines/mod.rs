use std::sync::Arc;
use crate::error::{AppError, Result};

pub mod stt_whisper;
pub mod tts_sherpa;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SttOptions {
    pub language: Option<String>,
    pub translate: bool,
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

pub trait SttEngine: Send + Sync {
    fn transcribe(&self, samples: &[f32], opts: &SttOptions, on_progress: &dyn Fn(f32)) -> Result<Transcript>;
    fn model_id(&self) -> &str;
}

pub trait TtsEngine: Send + Sync {
    fn voices(&self) -> Vec<Voice>;
    fn synthesize(&self, text: &str, opts: &TtsOptions, on_progress: &dyn Fn(f32)) -> Result<AudioBuffer>;
    fn model_id(&self) -> &str;
}

pub struct EngineRegistry {
    stt: Option<Arc<dyn SttEngine>>,
    tts: Option<Arc<dyn TtsEngine>>,
}

impl EngineRegistry {
    pub fn new() -> Self {
        Self { stt: None, tts: None }
    }

    pub fn set_stt(&mut self, engine: Arc<dyn SttEngine>) {
        self.stt = Some(engine);
    }

    pub fn set_tts(&mut self, engine: Arc<dyn TtsEngine>) {
        self.tts = Some(engine);
    }

    pub fn get_stt(&self) -> Option<Arc<dyn SttEngine>> {
        self.stt.clone()
    }

    pub fn get_tts(&self) -> Option<Arc<dyn TtsEngine>> {
        self.tts.clone()
    }

    pub fn clear_stt(&mut self) {
        self.stt = None;
    }

    pub fn clear_tts(&mut self) {
        self.tts = None;
    }
}

impl Default for EngineRegistry {
    fn default() -> Self {
        Self::new()
    }
}