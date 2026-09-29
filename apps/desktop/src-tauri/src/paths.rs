use std::path::PathBuf;
use crate::error::{AppError, Result};

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub home: PathBuf,
    pub models: PathBuf,
    pub models_stt: PathBuf,
    pub models_tts: PathBuf,
    pub audio: PathBuf,
    pub history: PathBuf,
    pub cache: PathBuf,
    pub cache_downloads: PathBuf,
    pub logs: PathBuf,
    pub config_file: PathBuf,
    pub db_file: PathBuf,
}

impl AppPaths {
    pub fn init() -> Result<Self> {
        let home = Self::resolve_home()?;
        let talkr_dir = home.join(".talkr");

        let paths = Self {
            home: talkr_dir.clone(),
            models: talkr_dir.join("models"),
            models_stt: talkr_dir.join("models").join("stt"),
            models_tts: talkr_dir.join("models").join("tts"),
            audio: talkr_dir.join("audio"),
            history: talkr_dir.join("history"),
            cache: talkr_dir.join("cache"),
            cache_downloads: talkr_dir.join("cache").join("downloads"),
            logs: talkr_dir.join("logs"),
            config_file: talkr_dir.join("config.json"),
            db_file: talkr_dir.join("history").join("talkr.db"),
        };

        paths.create_dirs()?;
        Ok(paths)
    }

    fn resolve_home() -> Result<PathBuf> {
        if let Ok(override_path) = std::env::var("TALKR_HOME") {
            return Ok(PathBuf::from(override_path));
        }

        dirs::home_dir()
            .ok_or_else(|| AppError::Path("Could not determine home directory".into()))
    }

    fn create_dirs(&self) -> Result<()> {
        let dirs = [
            &self.models,
            &self.models_stt,
            &self.models_tts,
            &self.audio,
            &self.history,
            &self.cache,
            &self.cache_downloads,
            &self.logs,
        ];

        for dir in dirs {
            std::fs::create_dir_all(dir)?;
        }

        Ok(())
    }

    pub fn model_dir(&self, kind: &str, model_id: &str) -> PathBuf {
        match kind {
            "stt" => self.models_stt.join(model_id),
            "tts" => self.models_tts.join(model_id),
            _ => self.models.join(kind).join(model_id),
        }
    }

    pub fn audio_path(&self, ext: &str) -> PathBuf {
        let now = chrono::Utc::now();
        let year = now.format("%Y").to_string();
        let month = now.format("%m").to_string();
        let uuid = uuid::Uuid::new_v4().to_string();
        self.audio.join(year).join(month).join(format!("{}.{}", uuid, ext))
    }
}

impl serde::Serialize for AppPaths {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("AppPaths", 11)?;
        state.serialize_field("home", &self.home.to_string_lossy())?;
        state.serialize_field("models", &self.models.to_string_lossy())?;
        state.serialize_field("modelsStt", &self.models_stt.to_string_lossy())?;
        state.serialize_field("modelsTts", &self.models_tts.to_string_lossy())?;
        state.serialize_field("audio", &self.audio.to_string_lossy())?;
        state.serialize_field("history", &self.history.to_string_lossy())?;
        state.serialize_field("cache", &self.cache.to_string_lossy())?;
        state.serialize_field("cacheDownloads", &self.cache_downloads.to_string_lossy())?;
        state.serialize_field("logs", &self.logs.to_string_lossy())?;
        state.serialize_field("configFile", &self.config_file.to_string_lossy())?;
        state.serialize_field("dbFile", &self.db_file.to_string_lossy())?;
        state.end()
    }
}