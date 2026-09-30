use tauri::{command, ipc::Response, AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;
use crate::commands::resolve_path;
use crate::config::{PartialSettings, Settings};
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

/// Read audio owned by Talkr as a raw IPC response. WebKitGTK cannot reliably play media from
/// Tauri's custom asset protocol, while a Blob URL works in every desktop webview. Canonicalizing
/// both paths prevents `..` traversal and symlinks from exposing arbitrary user files.
#[command]
pub async fn read_audio_file(app: AppHandle, path: String) -> Result<Response> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        Ok(Response::new(read_owned_audio(&state.paths, &path)?))
    })
    .await?
}

fn read_owned_audio(paths: &AppPaths, path: &str) -> Result<Vec<u8>> {
    let requested = resolve_path(paths, path);
    let audio_root = paths
        .audio
        .canonicalize()
        .map_err(|_| AppError::NotFound("The Talkr audio folder is not available".into()))?;
    let canonical = requested
        .canonicalize()
        .map_err(|_| AppError::NotFound("The audio file is not available".into()))?;
    if !canonical.starts_with(&audio_root) || !canonical.is_file() {
        return Err(AppError::Validation("Talkr can only play files from its audio folder".into()));
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
        }
        *current = updated.clone();
        Ok(updated)
    })
    .await?
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

    #[test]
    fn reads_only_files_inside_the_audio_folder() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::init_at(dir.path()).unwrap();
        let audio = paths.audio.join("2026/09/example.wav");
        std::fs::create_dir_all(audio.parent().unwrap()).unwrap();
        std::fs::write(&audio, b"RIFF-test").unwrap();

        assert_eq!(
            read_owned_audio(&paths, "audio/2026/09/example.wav").unwrap(),
            b"RIFF-test"
        );

        let outside = dir.path().join("private.wav");
        std::fs::write(&outside, b"private").unwrap();
        assert!(matches!(
            read_owned_audio(&paths, outside.to_str().unwrap()),
            Err(AppError::Validation(_))
        ));
        assert!(read_owned_audio(&paths, "audio/../../private.wav").is_err());
        assert!(matches!(
            read_owned_audio(&paths, "audio/missing.wav"),
            Err(AppError::NotFound(_))
        ));
    }
}
