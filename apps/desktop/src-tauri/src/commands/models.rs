use tauri::{command, State, AppHandle, Emitter};
use crate::lib::AppState;
use crate::error::{AppError, Result};
use crate::catalog::{Catalog, CatalogModel, ModelKind, InstalledModel, ModelManifest};
use crate::downloader::{DownloadManager, DownloadProgress, DownloadState, DownloadJob};
use crate::paths::AppPaths;
use std::sync::Arc;
use std::path::Path;
use uuid::Uuid;
use chrono::Utc;

type DownloadManagerState = Arc<Mutex<Option<DownloadManager>>>;

fn get_download_manager(state: &State<'_, AppState>) -> Arc<Mutex<DownloadManager>> {
    let mut guard = state.try_get::<DownloadManagerState>().expect("DownloadManager not initialized");
    let mut manager_opt = guard.lock().unwrap();
    if manager_opt.is_none() {
        *manager_opt = Some(DownloadManager::new());
    }
    Arc::new(Mutex::new(manager_opt.as_ref().unwrap().clone()))
}

#[command]
pub async fn list_catalog(state: State<'_, AppState>) -> Result<Vec<CatalogModel>> {
    let catalog = Catalog::load_embedded()?;
    let mut models = catalog.models;

    for model in &mut models {
        model.installed = model.is_installed(&state.paths);
        if let Some(manifest) = model.get_installed_manifest(&state.paths) {
            model.installed_version = Some(manifest.version);
        }
    }

    Ok(models)
}

#[command]
pub async fn list_installed_models(state: State<'_, AppState>) -> Result<Vec<InstalledModel>> {
    let catalog = Catalog::load_embedded()?;
    let mut installed = Vec::new();

    for model in catalog.models {
        if model.is_installed(&state.paths) {
            if let Some(manifest) = model.get_installed_manifest(&state.paths) {
                installed.push(InstalledModel {
                    id: model.id.clone(),
                    kind: model.kind,
                    name: model.name.clone(),
                    engine: model.engine.clone(),
                    path: state.paths.model_dir(
                        match model.kind {
                            ModelKind::Stt => "stt",
                            ModelKind::Tts => "tts",
                        },
                        &model.id
                    ).to_string_lossy().to_string(),
                    manifest,
                });
            }
        }
    }

    Ok(installed)
}

#[command]
pub async fn download_model(state: State<'_, AppState>, app: AppHandle, model_id: String) -> Result<String> {
    let catalog = Catalog::load_embedded()?;
    let model = catalog.get_model(&model_id)
        .ok_or_else(|| AppError::NotFound(format!("Model not found: {}", model_id)))?;

    if model.is_installed(&state.paths) {
        return Err(AppError::Model("Model already installed".into()));
    }

    if model.files.is_empty() {
        return Err(AppError::Model("No download files specified".into()));
    }

    let file = &model.files[0];
    let job_id = Uuid::new_v4().to_string();

    let (tx, rx) = flume::unbounded();
    let (cancel_tx, cancel_rx) = flume::unbounded();

    let job = DownloadJob {
        model_id: model_id.clone(),
        url: file.url.clone(),
        sha256: file.sha256.clone(),
        archive: file.archive.clone(),
        kind: format!("{:?}", model.kind).to_lowercase(),
        paths: state.paths.clone(),
        tx: tx.clone(),
        cancel_rx,
    };

    let app_handle = app.clone();
    tokio::spawn(async move {
        let _ = job.run().await;
    });

    tokio::spawn(async move {
        while let Ok(progress) = rx.recv_async().await {
            let _ = app_handle.emit("download://progress", &progress);
        }
    });

    Ok(job_id)
}

#[command]
pub async fn cancel_download(state: State<'_, AppState>, job_id: String) -> Result<()> {
    let manager = get_download_manager(&state);
    let mut manager = manager.lock().unwrap();
    manager.cancel(&job_id);
    Ok(())
}

#[command]
pub async fn delete_model(state: State<'_, AppState>, model_id: String) -> Result<()> {
    let catalog = Catalog::load_embedded()?;
    let model = catalog.get_model(&model_id)
        .ok_or_else(|| AppError::NotFound(format!("Model not found: {}", model_id)))?;

    let model_dir = state.paths.model_dir(
        match model.kind {
            ModelKind::Stt => "stt",
            ModelKind::Tts => "tts",
        },
        &model_id
    );

    if model_dir.exists() {
        std::fs::remove_dir_all(model_dir)?;
    }

    Ok(())
}

#[command]
pub async fn import_local_model(state: State<'_, AppState>, path: String, kind: String) -> Result<InstalledModel> {
    let src_path = Path::new(&path);
    if !src_path.exists() {
        return Err(AppError::NotFound("Source path does not exist".into()));
    }

    let model_id = src_path.file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| AppError::Validation("Invalid path".into()))?;

    let dest_dir = state.paths.model_dir(&kind, model_id);
    std::fs::create_dir_all(&dest_dir)?;

    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(src_path) {
        let entry = entry?;
        if entry.file_type().is_file() {
            let rel = entry.path().strip_prefix(src_path).unwrap();
            let dest = dest_dir.join(rel);
            std::fs::create_dir_all(dest.parent().unwrap())?;
            std::fs::copy(entry.path(), dest)?;
            files.push(rel.to_string_lossy().to_string());
        }
    }

    let manifest = ModelManifest {
        id: model_id.to_string(),
        version: "1.0".into(),
        sha256: String::new(),
        installed_at: Utc::now().timestamp_millis(),
        size_bytes: 0,
        files,
    };

    std::fs::write(dest_dir.join("manifest.json"), serde_json::to_string_pretty(&manifest)?)?;

    Ok(InstalledModel {
        id: model_id.to_string(),
        kind: match kind.as_str() {
            "stt" => ModelKind::Stt,
            "tts" => ModelKind::Tts,
            _ => return Err(AppError::Validation("Invalid kind".into())),
        },
        name: model_id.to_string(),
        engine: "custom".into(),
        path: dest_dir.to_string_lossy().to_string(),
        manifest,
    })
}