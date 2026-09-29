use serde::Serialize;
use tauri::{command, State};
use crate::commands::remove_owned_audio;
use crate::db::prune_old_history;
use crate::error::Result;
use crate::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageUsage {
    pub models_bytes: u64,
    pub audio_bytes: u64,
    pub db_bytes: u64,
    pub total_bytes: u64,
}

#[command]
pub async fn get_storage_usage(state: State<'_, AppState>) -> Result<StorageUsage> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let models_bytes = dir_size(&paths.models)?;
        let audio_bytes = dir_size(&paths.audio)?;
        // Include SQLite WAL/SHM side files.
        let db_bytes = dir_size(&paths.history)?;
        Ok(StorageUsage {
            models_bytes,
            audio_bytes,
            db_bytes,
            total_bytes: models_bytes + audio_bytes + db_bytes,
        })
    })
    .await?
}

/// Apply the history retention policy now. Returns the number of deleted items.
#[command]
pub async fn run_retention(state: State<'_, AppState>) -> Result<usize> {
    let days = state.settings().history_retention_days;
    let deleted = prune_old_history(&state.paths, days)?;
    remove_owned_audio(&state.paths, &deleted.audio_paths);
    Ok(deleted.count)
}

fn dir_size(path: &std::path::Path) -> Result<u64> {
    if !path.exists() {
        return Ok(0);
    }

    let mut size = 0u64;
    for entry in walkdir::WalkDir::new(path) {
        let entry = entry?;
        if entry.file_type().is_file() {
            size += entry.metadata()?.len();
        }
    }
    Ok(size)
}
