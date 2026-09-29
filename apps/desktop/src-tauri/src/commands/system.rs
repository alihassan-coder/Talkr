use tauri::{command, AppHandle, State};
use tauri_plugin_opener::OpenerExt;
use crate::config::{PartialSettings, Settings};
use crate::error::{AppError, Result};
use crate::hardware::HardwareInfo;
use crate::paths::AppPaths;
use crate::AppState;

#[command]
pub async fn get_hardware_info(state: State<'_, AppState>) -> Result<HardwareInfo> {
    Ok(state.hardware.clone())
}

#[command]
pub async fn get_app_paths(state: State<'_, AppState>) -> Result<AppPaths> {
    Ok(state.paths.clone())
}

#[command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<Settings> {
    Ok(state.settings().clone())
}

#[command]
pub async fn update_settings(state: State<'_, AppState>, settings: PartialSettings) -> Result<Settings> {
    let patch = settings;
    if let Some(rate) = patch.speech_rate {
        if !(0.25..=4.0).contains(&rate) {
            return Err(AppError::Validation("speechRate must be between 0.25 and 4.0".into()));
        }
    }
    let mut current = state.settings();
    let mut updated = current.clone();
    updated.merge(patch);
    updated.save(&state.paths)?;
    *current = updated.clone();
    Ok(updated)
}

#[command]
pub async fn open_data_folder(state: State<'_, AppState>, app: AppHandle) -> Result<()> {
    let path = state.paths.home.to_string_lossy().to_string();
    app.opener().open_path(path, None::<&str>)?;
    Ok(())
}
