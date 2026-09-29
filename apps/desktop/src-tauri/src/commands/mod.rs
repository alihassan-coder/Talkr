pub mod history;
pub mod models;
pub mod settings;
pub mod stt;
pub mod system;
pub mod tts;

pub use history::*;
pub use models::*;
pub use settings::*;
pub use stt::*;
pub use system::*;
pub use tts::*;

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use crate::db::HistoryItem;
use crate::engines::JobControl;
use crate::error::AppError;
use crate::paths::AppPaths;
use crate::AppState;

pub const EVENT_DOWNLOAD_PROGRESS: &str = "download://progress";
pub const EVENT_JOB_PROGRESS: &str = "job://progress";
pub const EVENT_JOB_ERROR: &str = "job://error";
pub const EVENT_TTS_DONE: &str = "tts://done";
pub const EVENT_STT_DONE: &str = "stt://done";
pub const EVENT_MIC_LEVEL: &str = "mic://level";

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

/// Build a `JobControl` that emits `job://progress` events for `job_id`.
pub(crate) fn job_control(app: &AppHandle, job_id: &str, cancel: Arc<AtomicBool>) -> JobControl {
    let app = app.clone();
    let job_id = job_id.to_string();
    JobControl::new(
        Arc::new(move |progress| {
            let _ = app.emit(
                EVENT_JOB_PROGRESS,
                JobProgressEvent {
                    job_id: job_id.clone(),
                    progress,
                },
            );
        }),
        cancel,
    )
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

/// Delete audio files referenced by history, but only those Talkr owns (inside `~/.talkr/audio`),
/// never user files that were merely transcribed.
pub(crate) fn remove_owned_audio(paths: &AppPaths, stored: &[String]) {
    for p in stored {
        let full = resolve_path(paths, p);
        if full.starts_with(&paths.audio) && full.is_file() {
            if let Err(e) = std::fs::remove_file(&full) {
                log::warn!("Failed to delete {}: {}", full.display(), e);
            }
        }
    }
}
