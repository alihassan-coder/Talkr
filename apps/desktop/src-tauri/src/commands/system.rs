use tauri::{command, ipc::Response, AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;
use crate::commands::resolve_path;
use crate::config::{DevicePreference, PartialSettings, Settings};
use crate::db::get_history;
use crate::error::{AppError, Result};
use crate::hardware::HardwareInfo;
use crate::paths::AppPaths;
use crate::AppState;

/// The detected hardware. Detection starts in the background at launch and is cached; if it is
/// still running, this waits for it on a blocking thread, never on the async runtime.
#[command]
pub async fn get_hardware_info(state: State<'_, AppState>) -> Result<HardwareInfo> {
    let probe = state.hardware.clone();
    if let Some(info) = probe.try_get() {
        return Ok(info);
    }
    Ok(tauri::async_runtime::spawn_blocking(move || probe.get()).await?)
}

/// What the speech engine can run on, for Settings.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    /// Compute devices the engine reported (always includes the CPU when the engine runs).
    pub devices: Vec<talkr_protocol::Device>,
    /// A GPU build or Metal is installed and has not failed.
    pub gpu_available: bool,
    /// The GPU engine crashed or could not start; speech to text uses the CPU until the compute
    /// setting changes or Talkr updates.
    pub gpu_failed: bool,
    /// Whether speech to text will use the GPU with the current setting.
    pub stt_uses_gpu: bool,
}

#[command]
pub async fn get_engine_status(app: AppHandle) -> Result<EngineStatus> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let devices = state.engine.devices();
        Ok(EngineStatus {
            devices,
            gpu_available: state.engine.gpu_available(),
            gpu_failed: state.engine.gpu_failed(),
            stt_uses_gpu: state.engine.should_use_gpu(state.gpu_policy()),
        })
    })
    .await?
}

#[command]
pub async fn get_app_paths(state: State<'_, AppState>) -> Result<AppPaths> {
    Ok(state.paths.clone())
}

/// Largest audio file Talkr will hand to the webview. The whole file crosses IPC and is held as a
/// Blob, so a multi-gigabyte import would stall or crash the window.
const MAX_PLAYABLE_AUDIO: u64 = 512 * 1024 * 1024;

/// Read the audio of a history item as a raw IPC response. WebKitGTK cannot reliably play media
/// from Tauri's custom asset protocol, while a Blob URL works in every desktop webview.
///
/// The webview names a history item, never a path: it can only read audio that Talkr itself
/// recorded, generated or was asked to transcribe, and the path comes from the database.
#[command]
pub async fn read_history_audio(app: AppHandle, id: String) -> Result<Response> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        Ok(Response::new(read_history_audio_bytes(&state.paths, &id, MAX_PLAYABLE_AUDIO)?))
    })
    .await?
}

fn read_history_audio_bytes(paths: &AppPaths, id: &str, max_bytes: u64) -> Result<Vec<u8>> {
    let item = get_history(paths, id)?.ok_or_else(|| AppError::NotFound("History item not found".into()))?;
    let stored = item
        .audio_path
        .ok_or_else(|| AppError::NotFound("This item has no audio".into()))?;
    // Canonicalizing resolves symlinks and `..`, so what is checked is what gets read.
    let canonical = resolve_path(paths, &stored)
        .canonicalize()
        .map_err(|_| AppError::NotFound("The audio file is no longer available. It may have been moved or deleted.".into()))?;
    let metadata = std::fs::metadata(&canonical)?;
    if !metadata.is_file() {
        return Err(AppError::NotFound("The audio file is no longer available. It may have been moved or deleted.".into()));
    }
    if metadata.len() > max_bytes {
        return Err(AppError::Validation(
            "This audio is too large to play in Talkr. Open it from its folder instead.".into(),
        ));
    }
    Ok(std::fs::read(canonical)?)
}

#[command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<Settings> {
    Ok(state.settings().clone())
}

/// Apply a partial update and persist it. The save (an fsync'd atomic write) runs on a blocking
/// thread; the settings lock is held across it so concurrent updates cannot interleave and the
/// file always matches memory. If the save fails, memory keeps the old settings.
#[command]
pub async fn update_settings(app: AppHandle, settings: PartialSettings) -> Result<Settings> {
    let patch = settings;
    patch.validate()?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let mut current = state.settings();
        let mut updated = current.clone();
        updated.merge(patch);
        updated.save(&state.paths)?;
        if updated.device != current.device {
            // A new compute choice deserves a fresh try on the GPU, even after it failed before.
            state.engine.reset_gpu();
            if updated.device == DevicePreference::Cpu {
                state.engine.stop_gpu_engine_if_idle();
            }
        }
        let dictation_changed = updated.dictation != current.dictation;
        *current = updated.clone();
        drop(current);
        if dictation_changed {
            app.state::<crate::dictation::Dictation>().reconfigure();
            #[cfg(windows)]
            crate::tray::sync(&app);
        }
        Ok(updated)
    })
    .await?
}

/// Stop the engine process before an update installs: on Windows the installer must replace
/// talkr-engine.exe, which it cannot do while the engine is running. The next job starts it again.
#[command]
pub async fn stop_engine(app: AppHandle) -> Result<()> {
    tauri::async_runtime::spawn_blocking(move || app.state::<AppState>().engine.shutdown()).await?;
    Ok(())
}

#[command]
pub async fn open_data_folder(state: State<'_, AppState>, app: AppHandle) -> Result<()> {
    let path = state.paths.home.to_string_lossy().to_string();
    app.opener().open_path(path, None::<&str>)?;
    Ok(())
}

#[cfg(test)]
mod audio_file_tests {
    use super::*;
    use crate::db::{init, insert_history, HistoryItem, HistoryKind};

    fn item(id: &str, audio_path: Option<String>) -> HistoryItem {
        HistoryItem {
            id: id.into(),
            kind: HistoryKind::Stt,
            created_at: 1,
            title: "t".into(),
            text: "x".into(),
            audio_path,
            duration_ms: None,
            model_id: "m".into(),
            voice_id: None,
            language: None,
            device: "cpu".into(),
            processing_ms: 0,
            favorite: false,
            segments_json: None,
        }
    }

    #[test]
    fn reads_only_audio_that_history_points_to() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::init_at(dir.path()).unwrap();
        init(&paths).unwrap();

        // A recording inside the audio folder, stored relative.
        let owned = paths.audio.join("2026/09/example.wav");
        std::fs::create_dir_all(owned.parent().unwrap()).unwrap();
        std::fs::write(&owned, b"RIFF-owned").unwrap();
        insert_history(&paths, &item("rec", Some("audio/2026/09/example.wav".into()))).unwrap();

        // An imported file elsewhere, stored absolute: playable because history points to it.
        let imported = dir.path().join("Music").join("talk.mp3");
        std::fs::create_dir_all(imported.parent().unwrap()).unwrap();
        std::fs::write(&imported, b"ID3-imported").unwrap();
        insert_history(&paths, &item("import", Some(imported.to_string_lossy().into_owned()))).unwrap();

        insert_history(&paths, &item("silent", None)).unwrap();
        insert_history(&paths, &item("gone", Some("audio/missing.wav".into()))).unwrap();
        insert_history(&paths, &item("folder", Some("audio/2026".into()))).unwrap();

        assert_eq!(read_history_audio_bytes(&paths, "rec", 1024).unwrap(), b"RIFF-owned");
        assert_eq!(read_history_audio_bytes(&paths, "import", 1024).unwrap(), b"ID3-imported");
        for id in ["silent", "gone", "folder", "no-such-id"] {
            assert!(
                matches!(read_history_audio_bytes(&paths, id, 1024), Err(AppError::NotFound(_))),
                "{id}"
            );
        }
    }

    #[test]
    fn refuses_audio_over_the_size_cap() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::init_at(dir.path()).unwrap();
        init(&paths).unwrap();
        let big = paths.audio.join("big.wav");
        std::fs::write(&big, [0u8; 100]).unwrap();
        insert_history(&paths, &item("big", Some("audio/big.wav".into()))).unwrap();

        assert_eq!(read_history_audio_bytes(&paths, "big", 100).unwrap().len(), 100);
        match read_history_audio_bytes(&paths, "big", 99) {
            Err(AppError::Validation(message)) => assert!(message.contains("too large to play")),
            other => panic!("expected a validation error, got {:?}", other.map(|b| b.len())),
        }
    }
}
