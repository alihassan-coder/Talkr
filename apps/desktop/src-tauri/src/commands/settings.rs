use tauri::{command, State};
use crate::lib::AppState;
use crate::error::{AppError, Result};
use crate::db::prune_old_history;
use crate::paths::AppPaths;

#[command]
pub async fn get_storage_usage(state: State<'_, AppState>) -> Result<StorageUsage> {
    let paths = &state.paths;

    let models_size = dir_size(&paths.models)?;
    let audio_size = dir_size(&paths.audio)?;
    let db_size = paths.db_file.metadata().map(|m| m.len()).unwrap_or(0);

    Ok(StorageUsage {
        models_bytes: models_size,
        audio_bytes: audio_size,
        db_bytes: db_size,
        total_bytes: models_size + audio_size + db_size,
    })
}

#[command]
pub async fn run_retention(state: State<'_, AppState>) -> Result<usize> {
    let settings = state.settings.lock().unwrap();
    let deleted = prune_old_history(&state.paths, settings.history_retention_days)?;
    Ok(deleted)
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageUsage {
    pub models_bytes: u64,
    pub audio_bytes: u64,
    pub db_bytes: u64,
    pub total_bytes: u64,
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