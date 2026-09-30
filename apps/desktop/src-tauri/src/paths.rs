use std::ffi::OsString;
use std::path::{Path, PathBuf};
use crate::error::{AppError, Result};

/// Where Talkr keeps its data. Everything lives under one folder, `<home>/.talkr`, where `<home>`
/// is the user's home directory or, when set, the `TALKR_HOME` environment variable.
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
    /// Resolve the data folder, create its layout and check that it is writable.
    pub fn init() -> Result<Self> {
        let base = resolve_home(std::env::var_os("TALKR_HOME"), dirs::home_dir())?;
        Self::init_at(&base)
    }

    /// Like [`AppPaths::init`], with `base` in place of the home directory.
    pub fn init_at(base: &Path) -> Result<Self> {
        let paths = Self::under(base);
        paths.create_dirs()?;
        paths.check_writable()?;
        Ok(paths)
    }

    /// The layout under `base`, without touching the disk.
    pub fn under(base: &Path) -> Self {
        let talkr_dir = base.join(".talkr");
        Self {
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
        }
    }

    fn create_dirs(&self) -> Result<()> {
        let dirs = [
            &self.home,
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
            std::fs::create_dir_all(dir)
                .map_err(|e| AppError::Path(format!("Could not create the folder {}: {}", dir.display(), e)))?;
        }

        Ok(())
    }

    /// `create_dir_all` succeeds on a folder that already exists but is read-only, so probe it:
    /// failing here, with the path in the message, beats failing on the first download or save.
    fn check_writable(&self) -> Result<()> {
        let probe = self.home.join(format!(".write-test-{}", std::process::id()));
        std::fs::write(&probe, b"ok")
            .map_err(|e| AppError::Path(format!("The folder {} is not writable: {}", self.home.display(), e)))?;
        let _ = std::fs::remove_file(&probe);
        Ok(())
    }

    pub fn model_dir(&self, kind: &str, model_id: &str) -> PathBuf {
        match kind {
            "stt" => self.models_stt.join(model_id),
            "tts" => self.models_tts.join(model_id),
            _ => self.models.join(kind).join(model_id),
        }
    }

    /// A new, unique file path for generated or recorded audio: `audio/<year>/<month>/<uuid>.<ext>`.
    /// The month folder is not created here.
    pub fn audio_path(&self, ext: &str) -> PathBuf {
        let now = chrono::Utc::now();
        let year = now.format("%Y").to_string();
        let month = now.format("%m").to_string();
        let uuid = uuid::Uuid::new_v4().to_string();
        self.audio.join(year).join(month).join(format!("{}.{}", uuid, ext))
    }
}

/// The folder that holds `.talkr`: `TALKR_HOME` when set and non-empty, else the home directory.
/// A relative `TALKR_HOME` is taken relative to the working directory, because stored paths are
/// resolved against the data folder and must not change meaning with the working directory.
fn resolve_home(talkr_home: Option<OsString>, home_dir: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(value) = talkr_home.filter(|v| !v.is_empty()) {
        let path = PathBuf::from(value);
        if path.is_absolute() {
            return Ok(path);
        }
        let cwd = std::env::current_dir()
            .map_err(|e| AppError::Path(format!("TALKR_HOME is relative and the working directory is unknown: {}", e)))?;
        return Ok(cwd.join(path));
    }

    home_dir.ok_or_else(|| {
        AppError::Path("Could not determine your home directory. Set TALKR_HOME to a writable folder.".into())
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn talkr_home_overrides_the_home_directory() {
        let dir = tempfile::tempdir().unwrap();
        let base = resolve_home(Some(dir.path().as_os_str().to_owned()), Some(PathBuf::from("/nowhere"))).unwrap();
        assert_eq!(base, dir.path());
    }

    #[test]
    fn empty_talkr_home_falls_back_to_the_home_directory() {
        let base = resolve_home(Some(OsString::new()), Some(PathBuf::from("/users/me"))).unwrap();
        assert_eq!(base, PathBuf::from("/users/me"));
        let base = resolve_home(None, Some(PathBuf::from("/users/me"))).unwrap();
        assert_eq!(base, PathBuf::from("/users/me"));
    }

    #[test]
    fn relative_talkr_home_is_made_absolute() {
        let base = resolve_home(Some(OsString::from("data")), None).unwrap();
        assert!(base.is_absolute());
        assert!(base.ends_with("data"));
    }

    #[test]
    fn missing_home_is_a_clear_error() {
        let err = resolve_home(None, None).unwrap_err().to_string();
        assert!(err.contains("TALKR_HOME"), "{err}");
    }

    #[test]
    fn layout_lives_under_dot_talkr() {
        let base = Path::new("base");
        let p = AppPaths::under(base);
        let home = base.join(".talkr");
        assert_eq!(p.home, home);
        assert_eq!(p.models, home.join("models"));
        assert_eq!(p.models_stt, home.join("models").join("stt"));
        assert_eq!(p.models_tts, home.join("models").join("tts"));
        assert_eq!(p.audio, home.join("audio"));
        assert_eq!(p.cache_downloads, home.join("cache").join("downloads"));
        assert_eq!(p.logs, home.join("logs"));
        assert_eq!(p.config_file, home.join("config.json"));
        assert_eq!(p.db_file, home.join("history").join("talkr.db"));
    }

    #[test]
    fn init_at_creates_every_folder() {
        let dir = tempfile::tempdir().unwrap();
        let p = AppPaths::init_at(dir.path()).unwrap();
        for d in [&p.home, &p.models_stt, &p.models_tts, &p.audio, &p.history, &p.cache_downloads, &p.logs] {
            assert!(d.is_dir(), "{} missing", d.display());
        }
        // The write probe cleans up after itself.
        let leftovers: Vec<_> = std::fs::read_dir(&p.home)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with(".write-test"))
            .collect();
        assert!(leftovers.is_empty());
        // Running it again on an existing layout is fine.
        AppPaths::init_at(dir.path()).unwrap();
    }

    #[test]
    fn init_at_reports_a_blocked_folder() {
        let dir = tempfile::tempdir().unwrap();
        // A file where the data folder should be: nothing can be created under it.
        std::fs::write(dir.path().join(".talkr"), b"not a folder").unwrap();
        let err = AppPaths::init_at(dir.path()).unwrap_err().to_string();
        assert!(err.contains(".talkr"), "{err}");
    }

    #[test]
    fn model_dir_by_kind() {
        let p = AppPaths::under(Path::new("base"));
        assert_eq!(p.model_dir("stt", "whisper-base"), p.models_stt.join("whisper-base"));
        assert_eq!(p.model_dir("tts", "kokoro"), p.models_tts.join("kokoro"));
        assert_eq!(p.model_dir("vad", "silero"), p.models.join("vad").join("silero"));
    }

    #[test]
    fn audio_path_is_unique_and_dated() {
        let p = AppPaths::under(Path::new("base"));
        let a = p.audio_path("wav");
        let b = p.audio_path("wav");
        assert_ne!(a, b);
        assert!(a.starts_with(&p.audio));
        assert_eq!(a.extension().unwrap(), "wav");
        let rel: Vec<String> = a
            .strip_prefix(&p.audio)
            .unwrap()
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        assert_eq!(rel.len(), 3, "{rel:?}");
        assert_eq!(rel[0].len(), 4);
        assert!(rel[0].chars().all(|c| c.is_ascii_digit()));
        let month: u32 = rel[1].parse().unwrap();
        assert!((1..=12).contains(&month));
        assert_eq!(rel[1].len(), 2);
        assert!(uuid::Uuid::parse_str(rel[2].trim_end_matches(".wav")).is_ok());
    }

    #[test]
    fn serializes_with_camel_case_keys() {
        let p = AppPaths::under(Path::new("base"));
        let v = serde_json::to_value(&p).unwrap();
        for key in ["home", "modelsStt", "modelsTts", "cacheDownloads", "configFile", "dbFile"] {
            assert!(v.get(key).is_some(), "missing {key}");
        }
    }
}
