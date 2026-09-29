use std::fs;
use serde::{Deserialize, Serialize};
use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub version: u32,
    pub device: DevicePreference,
    pub cpu_threads: usize,
    pub default_tts_model: Option<String>,
    pub default_voice: Option<String>,
    pub default_stt_model: Option<String>,
    pub stt_language: String,
    pub speech_rate: f32,
    pub history_retention_days: u32,
    pub save_recordings: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DevicePreference {
    Auto,
    Gpu,
    Cpu,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            device: DevicePreference::Auto,
            cpu_threads: 0,
            default_tts_model: None,
            default_voice: None,
            default_stt_model: None,
            stt_language: "auto".into(),
            speech_rate: 1.0,
            history_retention_days: 0,
            save_recordings: true,
        }
    }
}

impl Settings {
    pub fn load(paths: &crate::paths::AppPaths) -> Result<Self> {
        if !paths.config_file.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&paths.config_file)?;
        let mut settings: Settings = serde_json::from_str(&content)?;

        if settings.version < 1 {
            settings = Self::migrate(settings);
        }

        Ok(settings)
    }

    pub fn save(&self, paths: &crate::paths::AppPaths) -> Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        fs::write(&paths.config_file, content)?;
        Ok(())
    }

    fn migrate(mut settings: Self) -> Self {
        settings.version = 1;
        settings
    }

    pub fn merge(&mut self, patch: PartialSettings) {
        if let Some(device) = patch.device {
            self.device = device;
        }
        if let Some(cpu_threads) = patch.cpu_threads {
            self.cpu_threads = cpu_threads;
        }
        if let Some(default_tts_model) = patch.default_tts_model {
            self.default_tts_model = Some(default_tts_model);
        }
        if let Some(default_voice) = patch.default_voice {
            self.default_voice = Some(default_voice);
        }
        if let Some(default_stt_model) = patch.default_stt_model {
            self.default_stt_model = Some(default_stt_model);
        }
        if let Some(stt_language) = patch.stt_language {
            self.stt_language = stt_language;
        }
        if let Some(speech_rate) = patch.speech_rate {
            self.speech_rate = speech_rate;
        }
        if let Some(history_retention_days) = patch.history_retention_days {
            self.history_retention_days = history_retention_days;
        }
        if let Some(save_recordings) = patch.save_recordings {
            self.save_recordings = save_recordings;
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartialSettings {
    pub device: Option<DevicePreference>,
    pub cpu_threads: Option<usize>,
    pub default_tts_model: Option<String>,
    pub default_voice: Option<String>,
    pub default_stt_model: Option<String>,
    pub stt_language: Option<String>,
    pub speech_rate: Option<f32>,
    pub history_retention_days: Option<u32>,
    pub save_recordings: Option<bool>,
}