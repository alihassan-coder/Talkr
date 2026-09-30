use tauri::{command, AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;
use crate::config::{PartialSettings, Settings};
use crate::error::Result;
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
