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
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let deleted = prune_old_history(&paths, days)?;
        remove_owned_audio(&paths, &deleted.audio_paths);
        Ok(deleted.count)
    })
    .await?
}

/// Total size of the files under `path`. Entries that vanish or cannot be read mid-walk (a
/// download finishing, a file in use) are skipped rather than failing the whole count.
fn dir_size(path: &std::path::Path) -> Result<u64> {
    if !path.exists() {
        return Ok(0);
    }

    let size = walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum();
    Ok(size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dir_size_sums_nested_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a"), [0u8; 10]).unwrap();
        std::fs::create_dir_all(dir.path().join("x").join("y")).unwrap();
        std::fs::write(dir.path().join("x").join("y").join("b"), [0u8; 32]).unwrap();
        assert_eq!(dir_size(dir.path()).unwrap(), 42);
        assert_eq!(dir_size(&dir.path().join("missing")).unwrap(), 0);
    }
}
