pub mod dictation;
pub mod history;
pub mod models;
pub mod settings;
pub mod stt;
pub mod system;
pub mod tts;

pub use dictation::*;
pub use history::*;
pub use models::*;
pub use settings::*;
pub use stt::*;
pub use system::*;
pub use tts::*;

use std::path::{Path, PathBuf};
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use crate::commands::models::locate_model;
use crate::db::HistoryItem;
use crate::error::AppError;
use crate::paths::AppPaths;
use crate::AppState;

pub const EVENT_DOWNLOAD_PROGRESS: &str = "download://progress";
pub const EVENT_JOB_PROGRESS: &str = "job://progress";
pub const EVENT_JOB_ERROR: &str = "job://error";
pub const EVENT_TTS_DONE: &str = "tts://done";
pub const EVENT_STT_DONE: &str = "stt://done";
pub const EVENT_MIC_LEVEL: &str = "mic://level";
/// The microphone failed mid-recording (unplugged, or the recording could not be saved). Payload:
/// [`MicErrorEvent`]. What was captured is kept until the UI calls `stop_recording`.
pub const EVENT_MIC_ERROR: &str = "mic://error";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MicErrorEvent {
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobProgressEvent {
    pub job_id: String,
    pub progress: f32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobDoneEvent {
    pub job_id: String,
    pub history_item: HistoryItem,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobErrorEvent {
    pub job_id: String,
    pub error: String,
    pub cancelled: bool,
}

/// A progress callback that emits `job://progress` events for `job_id`.
pub(crate) fn progress_emitter(app: &AppHandle, job_id: &str) -> impl Fn(f32) {
    let app = app.clone();
    let job_id = job_id.to_string();
    move |progress| {
        let _ = app.emit(
            EVENT_JOB_PROGRESS,
            JobProgressEvent {
                job_id: job_id.clone(),
                progress: progress.clamp(0.0, 1.0),
            },
        );
    }
}

/// Emit the terminal event for a background job and unregister it.
pub(crate) fn finish_job(app: &AppHandle, state: &AppState, job_id: &str, done_event: &str, result: Result<HistoryItem, AppError>) {
    state.jobs.finish(job_id);
    match result {
        Ok(item) => {
            let _ = app.emit(
                done_event,
                JobDoneEvent {
                    job_id: job_id.to_string(),
                    history_item: item,
                },
            );
        }
        Err(e) => {
            if !matches!(e, AppError::Cancelled) {
                log::error!("Job {} failed: {}", job_id, e);
            }
            let _ = app.emit(
                EVENT_JOB_ERROR,
                JobErrorEvent {
                    job_id: job_id.to_string(),
                    cancelled: matches!(e, AppError::Cancelled),
                    error: e.to_string(),
                },
            );
        }
    }
}

/// Run a background job, turning a panic into an ordinary job error so a bug in one job shows up
/// as a message instead of taking the whole app down.
pub(crate) fn catch_panic<T>(job: impl FnOnce() -> Result<T, AppError>) -> Result<T, AppError> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(job)).unwrap_or_else(|payload| {
        let message = payload
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown error".into());
        Err(AppError::Engine(format!("The engine stopped unexpectedly: {}", message)))
    })
}

/// Check that a model in `dir` fits in the memory that is free right now, before handing it to the
/// engine. Running out mid-load does not fail cleanly (Linux's OOM killer, or an abort in
/// whisper.cpp), so say it up front. The engine runs in its own process, so even when this
/// estimate is wrong the app survives and engine_host explains what happened.
///
/// The engine keeps the last model it used loaded, and that memory is already missing from what
/// the system reports as free: it is not counted again for the same model, and is counted as
/// free for a different one (switching unloads it). `extra_bytes` is job memory on top of the
/// model, such as the decoded audio of a transcription.
pub(crate) fn ensure_memory_for_job(state: &AppState, dir: &Path, model_id: &str, extra_bytes: u64) -> Result<(), AppError> {
    let resident = match state.engine.resident_model() {
        None => Resident::Nothing,
        Some(id) if id == model_id => Resident::Same,
        Some(id) => Resident::Other(locate_model(&state.paths, &id).map(|(_, dir)| dir_size(&dir)).unwrap_or(0)),
    };
    check_memory(dir, model_id, extra_bytes, resident)
}

fn check_memory(dir: &Path, model_id: &str, extra_bytes: u64, resident: Resident) -> Result<(), AppError> {
    let model_bytes = dir_size(dir);
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    // Swap counts: it is slow, but it keeps the app alive.
    let available = sys.available_memory() + sys.free_swap();
    let Some(Shortfall { needed, free }) = memory_shortfall(model_bytes, extra_bytes, available, resident) else {
        return Ok(());
    };
    Err(AppError::Validation(format!(
        "Not enough free memory to run {}: it needs about {}, and only {} is free.          Close other apps, or pick a smaller or compressed model in Models.",
        model_id,
        format_size(needed),
        format_size(free)
    )))
}

/// The model the engine already holds in memory, relative to the one a job needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Resident {
    Nothing,
    /// The job's own model: its weights are loaded and already excluded from free memory.
    Same,
    /// Another model, with this many bytes on disk; switching frees about that much.
    Other(u64),
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Shortfall {
    pub needed: u64,
    pub free: u64,
}

/// The memory arithmetic, apart from asking the system. `available` is free RAM plus swap
/// (0 when unknown, which never blocks). Returns what is needed and free when it does not fit.
pub(crate) fn memory_shortfall(model_bytes: u64, extra_bytes: u64, available: u64, resident: Resident) -> Option<Shortfall> {
    if available == 0 {
        return None;
    }
    // Weights plus the engine's compute buffers, which run to about half the weights again.
    let weights = match resident {
        Resident::Same => 0,
        _ => model_bytes,
    };
    let needed = weights + model_bytes / 2 + extra_bytes + 150 * MB;
    let free = match resident {
        Resident::Other(bytes) => available.saturating_add(bytes),
        _ => available,
    };
    (free < needed).then_some(Shortfall { needed, free })
}

/// Total size of the files under `dir` (a model's weights and data).
fn dir_size(dir: &Path) -> u64 {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.metadata().ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum()
}

const MB: u64 = 1024 * 1024;

fn format_size(bytes: u64) -> String {
    if bytes >= 1024 * MB {
        format!("{:.1} GB", bytes as f64 / (1024 * MB) as f64)
    } else {
        format!("{} MB", bytes / MB)
    }
}

/// Resolve a path stored in history (relative to the Talkr home, or absolute).
pub(crate) fn resolve_path(paths: &AppPaths, p: &str) -> PathBuf {
    let path = Path::new(p);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        paths.home.join(path)
    }
}

/// Store paths inside the Talkr home as relative (forward slashes) so the data folder is portable.
pub(crate) fn to_stored_path(paths: &AppPaths, p: &Path) -> String {
    match p.strip_prefix(&paths.home) {
        Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
        Err(_) => p.to_string_lossy().to_string(),
    }
}

/// Whether `path` really lies inside `~/.talkr/audio`. A plain `starts_with` would accept
/// `audio/../../Documents/x.wav`, and callers delete what this approves.
pub(crate) fn is_owned_audio(paths: &AppPaths, path: &Path) -> bool {
    match (path.canonicalize(), paths.audio.canonicalize()) {
        (Ok(path), Ok(audio)) => path.starts_with(audio),
        _ => false,
    }
}

/// Delete audio files referenced by history, but only those Talkr owns (inside `~/.talkr/audio`),
/// never user files that were merely transcribed.
pub(crate) fn remove_owned_audio(paths: &AppPaths, stored: &[String]) {
    for p in stored {
        let full = resolve_path(paths, p);
        if is_owned_audio(paths, &full) && full.is_file() {
            if let Err(e) = std::fs::remove_file(&full) {
                log::warn!("Failed to delete {}: {}", full.display(), e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_paths() -> (tempfile::TempDir, AppPaths) {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::init_at(dir.path()).unwrap();
        (dir, paths)
    }

    fn write(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"x").unwrap();
    }

    #[test]
    fn resolve_relative_and_absolute() {
        let (dir, paths) = temp_paths();
        assert_eq!(resolve_path(&paths, "audio/2026/01/a.wav"), paths.home.join("audio/2026/01/a.wav"));
        let abs = dir.path().join("elsewhere.wav");
        assert_eq!(resolve_path(&paths, abs.to_str().unwrap()), abs);
    }

    #[test]
    fn stored_paths_are_relative_with_forward_slashes() {
        let (dir, paths) = temp_paths();
        let inside = paths.audio.join("2026").join("01").join("a.wav");
        assert_eq!(to_stored_path(&paths, &inside), "audio/2026/01/a.wav");
        let outside = dir.path().join("user.wav");
        assert_eq!(to_stored_path(&paths, &outside), outside.to_string_lossy());
        // And back again.
        assert_eq!(resolve_path(&paths, &to_stored_path(&paths, &inside)), paths.home.join("audio/2026/01/a.wav"));
        assert_eq!(resolve_path(&paths, &to_stored_path(&paths, &outside)), outside);
    }

    #[test]
    fn owned_audio_must_really_be_inside_the_audio_folder() {
        let (dir, paths) = temp_paths();
        let inside = paths.audio.join("2026").join("01").join("a.wav");
        write(&inside);
        assert!(is_owned_audio(&paths, &inside));

        // Files that exist but are outside.
        let user_file = dir.path().join("Documents").join("x.wav");
        write(&user_file);
        assert!(!is_owned_audio(&paths, &user_file));
        let sibling = paths.home.join("config.json");
        write(&sibling);
        assert!(!is_owned_audio(&paths, &sibling));

        // `..` traversal that starts inside the audio folder and climbs out.
        let traversal = paths.audio.join("..").join("..").join("Documents").join("x.wav");
        assert!(traversal.starts_with(&paths.audio), "a naive prefix check would accept this");
        assert!(!is_owned_audio(&paths, &traversal));
        let traversal_rel = resolve_path(&paths, "audio/../../Documents/x.wav");
        assert!(!is_owned_audio(&paths, &traversal_rel));

        // A folder whose name merely starts with "audio".
        let lookalike = paths.home.join("audio-evil").join("x.wav");
        write(&lookalike);
        assert!(!is_owned_audio(&paths, &lookalike));

        // Missing files are not owned (canonicalize fails).
        assert!(!is_owned_audio(&paths, &paths.audio.join("missing.wav")));
    }

    #[test]
    fn remove_owned_audio_deletes_only_owned_files() {
        let (dir, paths) = temp_paths();
        let owned = paths.audio.join("2026").join("01").join("a.wav");
        write(&owned);
        let user_file = dir.path().join("Documents").join("x.wav");
        write(&user_file);
        let folder = paths.audio.join("2026").join("02");
        std::fs::create_dir_all(&folder).unwrap();

        remove_owned_audio(
            &paths,
            &[
                "audio/2026/01/a.wav".to_string(),
                user_file.to_string_lossy().into_owned(),
                "audio/../../Documents/x.wav".to_string(),
                "audio/2026/02".to_string(),
                "audio/missing.wav".to_string(),
            ],
        );

        assert!(!owned.exists());
        assert!(user_file.exists(), "user files must never be deleted");
        assert!(folder.is_dir(), "folders are not files");
    }

    const GB: u64 = 1024 * MB;

    #[test]
    fn memory_fits_or_reports_the_shortfall() {
        // 1 GB of weights needs 1.5 GB plus 150 MB of headroom.
        assert_eq!(memory_shortfall(GB, 0, 2 * GB, Resident::Nothing), None);
        assert_eq!(
            memory_shortfall(GB, 0, GB, Resident::Nothing),
            Some(Shortfall { needed: GB + GB / 2 + 150 * MB, free: GB })
        );
        // Unknown free memory never blocks.
        assert_eq!(memory_shortfall(100 * GB, 0, 0, Resident::Nothing), None);
    }

    #[test]
    fn memory_counts_the_resident_model_once() {
        // Already loaded: only the compute buffers and headroom are new.
        assert_eq!(memory_shortfall(GB, 0, GB, Resident::Same), None);
        assert_eq!(
            memory_shortfall(GB, 0, 600 * MB, Resident::Same),
            Some(Shortfall { needed: GB / 2 + 150 * MB, free: 600 * MB })
        );
        // Another model is loaded: switching frees it.
        assert_eq!(memory_shortfall(GB, 0, GB, Resident::Other(GB)), None);
        assert_eq!(
            memory_shortfall(2 * GB, 0, GB, Resident::Other(GB)),
            Some(Shortfall { needed: 3 * GB + 150 * MB, free: 2 * GB })
        );
    }

    #[test]
    fn memory_includes_job_extras() {
        // An hour of decoded 16 kHz mono f32 audio is about 220 MB.
        let audio = 3600 * 64_000;
        assert_eq!(memory_shortfall(GB, 0, 1700 * MB, Resident::Nothing), None);
        assert!(memory_shortfall(GB, audio, 1700 * MB, Resident::Nothing).is_some());
        assert_eq!(memory_shortfall(GB, audio, 1700 * MB, Resident::Same), None);
    }

    #[test]
    fn dir_size_sums_nested_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("a/b")).unwrap();
        std::fs::write(dir.path().join("x.bin"), [0u8; 10]).unwrap();
        std::fs::write(dir.path().join("a/b/y.bin"), [0u8; 5]).unwrap();
        assert_eq!(dir_size(dir.path()), 15);
        assert_eq!(dir_size(&dir.path().join("missing")), 0);
    }

    #[test]
    fn sizes() {
        assert_eq!(format_size(0), "0 MB");
        assert_eq!(format_size(5 * MB + 1), "5 MB");
        assert_eq!(format_size(1023 * MB), "1023 MB");
        assert_eq!(format_size(1024 * MB), "1.0 GB");
        assert_eq!(format_size(1536 * MB), "1.5 GB");
    }
}
