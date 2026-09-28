use tauri::{command, State, AppHandle, Emitter};
use crate::lib::AppState;
use crate::error::{AppError, Result};
use crate::hardware::HardwareInfo;
use crate::paths::AppPaths;
use crate::config::Settings;
use crate::catalog::{Catalog, CatalogModel, ModelKind, InstalledModel, ModelManifest};
use crate::downloader::{DownloadManager, DownloadProgress, DownloadState, DownloadJob};
use crate::engines::{EngineRegistry, SttOptions, TtsOptions};
use crate::audio::{record, decode, resample, wav};
use crate::db::{insert_history, list_history, get_history, delete_history, toggle_favorite, clear_history, prune_old_history, HistoryItem, HistoryKind, HistoryListResult};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use uuid::Uuid;
use chrono::Utc;

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
    let settings = state.settings.lock().unwrap();
    Ok(settings.clone())
}

#[command]
pub async fn update_settings(state: State<'_, AppState>, patch: crate::config::PartialSettings) -> Result<Settings> {
    let mut settings = state.settings.lock().unwrap();
    settings.merge(patch);
    settings.save(&state.paths)?;
    Ok(settings.clone())
}

#[command]
pub async fn open_data_folder(state: State<'_, AppState>, app: AppHandle) -> Result<()> {
    let path = &state.paths.home;
    app.opener().open_path(path, None::<&str>)?;
    Ok(())
}