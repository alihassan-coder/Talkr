use std::path::Path;
use sherpa_rs::{OfflineTts, OfflineTtsConfig, Voice as SherpaVoice};
use crate::engines::{TtsEngine, TtsOptions, Voice, AudioBuffer, Result, AppError};

pub struct SherpaTtsEngine {
    tts: OfflineTts,
    model_id: String,
    voices: Vec<Voice>,
}

impl SherpaTtsEngine {
    pub fn new(model_dir: &Path, model_id: String) -> Result<Self> {
        let model_path = model_dir.join("model.onnx");
        let tokens_path = model_dir.join("tokens.txt");
        let data_dir = model_dir.join("espeak-ng-data");

        let config = OfflineTtsConfig::new()
            .set_model(model_path.to_str().unwrap())
            .set_tokens(tokens_path.to_str().unwrap())
            .set_data_dir(data_dir.to_str().unwrap())
            .set_num_threads(num_cpus::get() as i32);

        let tts = OfflineTts::new(config)
            .map_err(|e| AppError::Engine(format!("Failed to load Sherpa TTS model: {}", e)))?;

        let voices = tts.list_voices()
            .iter()
            .map(|v| Voice {
                id: v.name.clone(),
                name: v.name.clone(),
                language: v.language.clone(),
                gender: v.gender.clone(),
            })
            .collect();

        Ok(Self { tts, model_id, voices })
    }
}

impl TtsEngine for SherpaTtsEngine {
    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn voices(&self) -> Vec<Voice> {
        self.voices.clone()
    }

    fn synthesize(&self, text: &str, opts: &TtsOptions, on_progress: &dyn Fn(f32)) -> Result<AudioBuffer> {
        let sentences: Vec<&str> = text.split(['.', '!', '?'])
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        let mut all_samples = Vec::new();
        let total = sentences.len() as f32;

        for (i, sentence) in sentences.iter().enumerate() {
            on_progress(i as f32 / total);

            let audio = self.tts.generate(sentence, &opts.voice_id, opts.speed)
                .map_err(|e| AppError::Engine(e.to_string()))?;

            all_samples.extend_from_slice(audio.samples());
        }

        on_progress(1.0);

        Ok(AudioBuffer {
            samples: all_samples,
            sample_rate: self.tts.sample_rate() as u32,
        })
    }
}