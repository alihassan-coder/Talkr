use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use crate::error::{AppError, Result};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

/// Allowed range for `speechRate`.
pub const SPEECH_RATE_RANGE: std::ops::RangeInclusive<f32> = 0.25..=4.0;
/// Upper bound for `cpuThreads` (0 = automatic).
pub const MAX_CPU_THREADS: usize = 256;
/// Upper bound for `historyRetentionDays` (0 = keep forever): 100 years.
pub const MAX_RETENTION_DAYS: u32 = 36_500;

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
    /// Read `config.json`. A missing file gives the defaults. A file that is not valid settings
    /// JSON is moved aside to `config.json.bad-<timestamp>` (so the next save cannot destroy
    /// what the user had) and the defaults are used; out-of-range values are reset. Only an I/O
    /// error reading the file fails. What happened is returned as notices for the caller to log,
    /// because at startup the settings are read before the logger exists.
    pub fn load_with_notices(paths: &crate::paths::AppPaths) -> Result<(Self, Vec<String>)> {
        let mut notices = Vec::new();
        let settings = Self::load_from(&paths.config_file, &mut notices)?;
        Ok((settings, notices))
    }

    fn load_from(file: &Path, notices: &mut Vec<String>) -> Result<Self> {
        let content = match fs::read_to_string(file) {
            Ok(content) => content,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e.into()),
        };

        let mut settings: Settings = match serde_json::from_str(&content) {
            Ok(settings) => settings,
            Err(parse_error) => {
                notices.push(match backup_corrupt(file) {
                    Ok(backup) => format!(
                        "Settings file {} is not valid ({}); moved it to {} and using the defaults",
                        file.display(),
                        parse_error,
                        backup.display()
                    ),
                    Err(e) => format!(
                        "Settings file {} is not valid ({}) and could not be backed up ({}); using the defaults",
                        file.display(),
                        parse_error,
                        e
                    ),
                });
                return Ok(Self::default());
            }
        };

        if settings.version < 1 {
            settings = Self::migrate(settings);
        }
        settings.sanitize(notices);
        Ok(settings)
    }

    /// Write `config.json` atomically: a temporary file in the same folder, flushed to disk, then
    /// renamed over the old one. A crash or full disk mid-save leaves the previous file intact.
    pub fn save(&self, paths: &crate::paths::AppPaths) -> Result<()> {
        self.save_to(&paths.config_file)
    }

    fn save_to(&self, file: &Path) -> Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        write_atomic(file, content.as_bytes())
    }

    fn migrate(mut settings: Self) -> Self {
        settings.version = 1;
        settings
    }

    /// Replace out-of-range values (from a hand-edited file) with the defaults.
    fn sanitize(&mut self, notices: &mut Vec<String>) {
        let defaults = Self::default();
        if !SPEECH_RATE_RANGE.contains(&self.speech_rate) {
            notices.push(format!("Settings: speechRate {} is out of range; using {}", self.speech_rate, defaults.speech_rate));
            self.speech_rate = defaults.speech_rate;
        }
        if self.cpu_threads > MAX_CPU_THREADS {
            notices.push(format!("Settings: cpuThreads {} is out of range; using automatic", self.cpu_threads));
            self.cpu_threads = defaults.cpu_threads;
        }
        if self.history_retention_days > MAX_RETENTION_DAYS {
            notices.push(format!("Settings: historyRetentionDays {} is out of range; keeping history forever", self.history_retention_days));
            self.history_retention_days = defaults.history_retention_days;
        }
        if self.stt_language.trim().is_empty() {
            notices.push("Settings: sttLanguage is empty; using auto".to_string());
            self.stt_language = defaults.stt_language;
        }
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

impl PartialSettings {
    /// Reject values the app cannot use, with a message fit for the user.
    pub fn validate(&self) -> Result<()> {
        if let Some(rate) = self.speech_rate {
            if !SPEECH_RATE_RANGE.contains(&rate) {
                return Err(AppError::Validation("speechRate must be between 0.25 and 4.0".into()));
            }
        }
        if let Some(threads) = self.cpu_threads {
            if threads > MAX_CPU_THREADS {
                return Err(AppError::Validation(format!("cpuThreads must be between 0 and {}", MAX_CPU_THREADS)));
            }
        }
        if let Some(days) = self.history_retention_days {
            if days > MAX_RETENTION_DAYS {
                return Err(AppError::Validation(format!(
                    "historyRetentionDays must be between 0 and {}",
                    MAX_RETENTION_DAYS
                )));
            }
        }
        if let Some(language) = &self.stt_language {
            if language.trim().is_empty() {
                return Err(AppError::Validation("sttLanguage must not be empty".into()));
            }
        }
        Ok(())
    }
}

/// Move a corrupt file to `<name>.bad-<UTC timestamp>` and return the new path.
fn backup_corrupt(file: &Path) -> std::io::Result<PathBuf> {
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ");
    let mut name = file.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".bad-{}", stamp));
    let backup = file.with_file_name(name);
    fs::rename(file, &backup)?;
    Ok(backup)
}

/// Write `bytes` to `file` via a temporary sibling and a rename, which replaces the target in
/// one step on every platform (MoveFileEx with MOVEFILE_REPLACE_EXISTING on Windows).
pub(crate) fn write_atomic(file: &Path, bytes: &[u8]) -> Result<()> {
    let mut tmp_name = file.file_name().unwrap_or_default().to_os_string();
    tmp_name.push(format!(".tmp-{}", uuid::Uuid::new_v4().simple()));
    let tmp = file.with_file_name(tmp_name);

    let result = (|| -> std::io::Result<()> {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, file)
    })();

    if let Err(e) = result {
        let _ = fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::AppPaths;

    fn temp_paths() -> (tempfile::TempDir, AppPaths) {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::init_at(dir.path()).unwrap();
        (dir, paths)
    }

    fn load(paths: &AppPaths) -> Result<Settings> {
        Settings::load_with_notices(paths).map(|(s, _)| s)
    }

    fn files_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn defaults() {
        let s = Settings::default();
        assert_eq!(s.version, 1);
        assert_eq!(s.device, DevicePreference::Auto);
        assert_eq!(s.cpu_threads, 0);
        assert_eq!(s.stt_language, "auto");
        assert_eq!(s.speech_rate, 1.0);
        assert_eq!(s.history_retention_days, 0);
        assert!(s.save_recordings);
        assert!(s.default_tts_model.is_none() && s.default_voice.is_none() && s.default_stt_model.is_none());
    }

    #[test]
    fn missing_file_loads_defaults() {
        let (_dir, paths) = temp_paths();
        assert_eq!(load(&paths).unwrap(), Settings::default());
    }

    #[test]
    fn round_trip() {
        let (_dir, paths) = temp_paths();
        let s = Settings {
            device: DevicePreference::Gpu,
            cpu_threads: 6,
            default_tts_model: Some("kokoro".into()),
            default_voice: Some("af_heart".into()),
            default_stt_model: Some("whisper-small".into()),
            stt_language: "de".into(),
            speech_rate: 1.5,
            history_retention_days: 30,
            save_recordings: false,
            ..Settings::default()
        };
        s.save(&paths).unwrap();
        assert_eq!(load(&paths).unwrap(), s);
    }

    #[test]
    fn file_uses_camel_case_and_tolerates_missing_and_unknown_fields() {
        let (_dir, paths) = temp_paths();
        Settings::default().save(&paths).unwrap();
        let raw = fs::read_to_string(&paths.config_file).unwrap();
        assert!(raw.contains("\"speechRate\"") && raw.contains("\"historyRetentionDays\""), "{raw}");

        fs::write(&paths.config_file, r#"{"device":"cpu","someFutureKey":42}"#).unwrap();
        let s = load(&paths).unwrap();
        assert_eq!(s.device, DevicePreference::Cpu);
        assert_eq!(s.stt_language, "auto");
    }

    #[test]
    fn old_version_is_migrated() {
        let (_dir, paths) = temp_paths();
        fs::write(&paths.config_file, r#"{"version":0,"cpuThreads":2}"#).unwrap();
        let s = load(&paths).unwrap();
        assert_eq!(s.version, 1);
        assert_eq!(s.cpu_threads, 2);
    }

    #[test]
    fn out_of_range_values_are_reset() {
        let (_dir, paths) = temp_paths();
        fs::write(
            &paths.config_file,
            r#"{"speechRate":99,"cpuThreads":100000,"historyRetentionDays":4000000,"sttLanguage":"  "}"#,
        )
        .unwrap();
        let (s, notices) = Settings::load_with_notices(&paths).unwrap();
        assert_eq!(notices.len(), 4, "{notices:?}");
        assert_eq!(s.speech_rate, 1.0);
        assert_eq!(s.cpu_threads, 0);
        assert_eq!(s.history_retention_days, 0);
        assert_eq!(s.stt_language, "auto");
    }

    #[test]
    fn corrupt_file_is_backed_up_not_overwritten() {
        let (_dir, paths) = temp_paths();
        let garbage = "{ this is not json";
        fs::write(&paths.config_file, garbage).unwrap();

        let (s, notices) = Settings::load_with_notices(&paths).unwrap();
        assert_eq!(s, Settings::default());
        assert_eq!(notices.len(), 1);
        assert!(notices[0].contains("config.json.bad-"), "{notices:?}");
        assert!(!paths.config_file.exists(), "the corrupt file should have been moved aside");

        let names = files_in(&paths.home);
        let backups: Vec<&String> = names.iter().filter(|n| n.starts_with("config.json.bad-")).collect();
        assert_eq!(backups.len(), 1, "{names:?}");
        assert_eq!(fs::read_to_string(paths.home.join(backups[0])).unwrap(), garbage);

        // Saving afterwards writes a fresh file and leaves the backup alone.
        s.save(&paths).unwrap();
        assert_eq!(load(&paths).unwrap(), Settings::default());
        assert_eq!(fs::read_to_string(paths.home.join(backups[0])).unwrap(), garbage);
    }

    #[test]
    fn wrong_types_count_as_corrupt() {
        let (_dir, paths) = temp_paths();
        fs::write(&paths.config_file, r#"{"device":"quantum"}"#).unwrap();
        assert_eq!(load(&paths).unwrap(), Settings::default());
        assert!(files_in(&paths.home).iter().any(|n| n.starts_with("config.json.bad-")));
    }

    #[test]
    fn unreadable_file_is_an_error_not_a_reset() {
        let (_dir, paths) = temp_paths();
        // A folder where the file should be: reading fails with an I/O error other than NotFound.
        fs::create_dir(&paths.config_file).unwrap();
        assert!(load(&paths).is_err());
        assert!(paths.config_file.is_dir(), "nothing should be moved on an I/O error");
    }

    #[test]
    fn save_is_atomic_and_leaves_no_temp_files() {
        let (_dir, paths) = temp_paths();
        let mut s = Settings::default();
        for i in 0..5 {
            s.cpu_threads = i;
            s.save(&paths).unwrap();
        }
        assert_eq!(load(&paths).unwrap().cpu_threads, 4);
        let names = files_in(&paths.home);
        assert_eq!(names, vec!["config.json".to_string()], "{names:?}");
    }

    #[test]
    fn failed_save_keeps_the_previous_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.json");
        Settings::default().save_to(&file).unwrap();
        // The target's folder is gone: the write fails, and nothing half-written is left behind.
        let missing = dir.path().join("missing").join("config.json");
        assert!(Settings::default().save_to(&missing).is_err());
        assert_eq!(Settings::load_from(&file, &mut Vec::new()).unwrap(), Settings::default());
        assert_eq!(files_in(dir.path()), vec!["config.json".to_string()]);
    }

    #[test]
    fn write_atomic_replaces_existing_content() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("x.json");
        write_atomic(&file, b"first, and longer").unwrap();
        write_atomic(&file, b"second").unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"second");
        assert_eq!(files_in(dir.path()), vec!["x.json".to_string()]);
    }

    #[test]
    fn merge_applies_only_given_fields() {
        let mut s = Settings::default();
        s.merge(PartialSettings {
            device: Some(DevicePreference::Cpu),
            speech_rate: Some(0.5),
            default_voice: Some("bf_emma".into()),
            ..Default::default()
        });
        assert_eq!(s.device, DevicePreference::Cpu);
        assert_eq!(s.speech_rate, 0.5);
        assert_eq!(s.default_voice.as_deref(), Some("bf_emma"));
        assert_eq!(s.cpu_threads, 0);
        assert_eq!(s.stt_language, "auto");
        assert!(s.save_recordings);

        let before = s.clone();
        s.merge(PartialSettings::default());
        assert_eq!(s, before);

        s.merge(PartialSettings {
            cpu_threads: Some(3),
            default_tts_model: Some("piper".into()),
            default_stt_model: Some("whisper".into()),
            stt_language: Some("fr".into()),
            history_retention_days: Some(7),
            save_recordings: Some(false),
            ..Default::default()
        });
        assert_eq!(s.cpu_threads, 3);
        assert_eq!(s.default_tts_model.as_deref(), Some("piper"));
        assert_eq!(s.default_stt_model.as_deref(), Some("whisper"));
        assert_eq!(s.stt_language, "fr");
        assert_eq!(s.history_retention_days, 7);
        assert!(!s.save_recordings);
    }

    #[test]
    fn partial_settings_deserialize_from_camel_case() {
        let p: PartialSettings = serde_json::from_str(r#"{"speechRate":2.0,"saveRecordings":false}"#).unwrap();
        assert_eq!(p.speech_rate, Some(2.0));
        assert_eq!(p.save_recordings, Some(false));
        assert!(p.device.is_none());
    }

    #[test]
    fn validate_rejects_bad_values() {
        let ok = PartialSettings { speech_rate: Some(0.25), cpu_threads: Some(8), ..Default::default() };
        assert!(ok.validate().is_ok());
        assert!(PartialSettings { speech_rate: Some(4.0), ..Default::default() }.validate().is_ok());
        for bad in [
            PartialSettings { speech_rate: Some(0.1), ..Default::default() },
            PartialSettings { speech_rate: Some(f32::NAN), ..Default::default() },
            PartialSettings { speech_rate: Some(4.5), ..Default::default() },
            PartialSettings { cpu_threads: Some(MAX_CPU_THREADS + 1), ..Default::default() },
            PartialSettings { history_retention_days: Some(MAX_RETENTION_DAYS + 1), ..Default::default() },
            PartialSettings { stt_language: Some(" ".into()), ..Default::default() },
        ] {
            assert!(matches!(bad.validate(), Err(AppError::Validation(_))), "{bad:?}");
        }
    }
}
