use std::path::{Path, PathBuf};
use chrono::Utc;
use tauri::{command, AppHandle, Emitter, Manager, State};
use uuid::Uuid;
use crate::catalog::{Catalog, CatalogModel, InstalledModel, ModelKind, ModelManifest};
use crate::commands::EVENT_DOWNLOAD_PROGRESS;
use crate::downloader::{list_dir_files, DownloadJob};
use crate::error::{AppError, Result};
use crate::paths::AppPaths;
use crate::AppState;

fn default_engine(kind: ModelKind) -> &'static str {
    match kind {
        ModelKind::Stt => "whisper",
        ModelKind::Tts => "sherpa-onnx",
    }
}

/// Find where an installed model lives and what kind it is (catalog or imported).
pub(crate) fn locate_model(paths: &AppPaths, model_id: &str) -> Option<(ModelKind, PathBuf)> {
    if let Ok(catalog) = Catalog::load_embedded() {
        if let Some(m) = catalog.get_model(model_id) {
            return Some((m.kind, m.dir(paths)));
        }
    }
    [ModelKind::Stt, ModelKind::Tts]
        .into_iter()
        .map(|k| (k, paths.model_dir(k.as_str(), model_id)))
        .find(|(_, dir)| dir.join("manifest.json").is_file())
}

#[command]
pub async fn list_catalog(state: State<'_, AppState>) -> Result<Vec<CatalogModel>> {
    let catalog = Catalog::load_embedded()?;
    let mut models = catalog.models;

    for model in &mut models {
        let manifest = model.get_installed_manifest(&state.paths);
        model.installed = manifest.is_some();
        model.installed_version = manifest.map(|m| m.version);
    }

    Ok(models)
}

#[command]
pub async fn list_installed_models(state: State<'_, AppState>) -> Result<Vec<InstalledModel>> {
    let catalog = Catalog::load_embedded()?;
    let mut installed = Vec::new();

    for model in &catalog.models {
        if let Some(manifest) = model.get_installed_manifest(&state.paths) {
            installed.push(InstalledModel {
                id: model.id.clone(),
                kind: model.kind,
                name: model.name.clone(),
                engine: model.engine.clone(),
                path: model.dir(&state.paths).to_string_lossy().to_string(),
                manifest,
            });
        }
    }

    // Locally imported models (not in the catalog).
    for kind in [ModelKind::Stt, ModelKind::Tts] {
        let root = match kind {
            ModelKind::Stt => &state.paths.models_stt,
            ModelKind::Tts => &state.paths.models_tts,
        };
        let Ok(entries) = std::fs::read_dir(root) else { continue };
        for entry in entries.flatten() {
            let dir = entry.path();
            let id = entry.file_name().to_string_lossy().to_string();
            if !dir.is_dir() || catalog.get_model(&id).is_some() {
                continue;
            }
            if let Some(manifest) = ModelManifest::read(&dir) {
                installed.push(InstalledModel {
                    id: id.clone(),
                    kind,
                    name: id,
                    engine: default_engine(kind).into(),
                    path: dir.to_string_lossy().to_string(),
                    manifest,
                });
            }
        }
    }

    Ok(installed)
}

/// Start downloading a catalog model. Returns the job id; progress is reported through
/// `download://progress` events (terminal states: installed / failed / cancelled).
#[command]
pub async fn download_model(state: State<'_, AppState>, app: AppHandle, model_id: String) -> Result<String> {
    let catalog = Catalog::load_embedded()?;
    let model = catalog
        .get_model(&model_id)
        .ok_or_else(|| AppError::NotFound(format!("Model not found: {}", model_id)))?;

    if model.is_installed(&state.paths) {
        return Err(AppError::Model("Model already installed".into()));
    }

    let file = model
        .files
        .first()
        .ok_or_else(|| AppError::Model("No download files specified".into()))?;

    let job_id = Uuid::new_v4().to_string();
    let cancel = state.downloads.register(&job_id, &model_id)?;

    let (tx, rx) = flume::unbounded();

    let job = DownloadJob {
        job_id: job_id.clone(),
        model_id: model_id.clone(),
        url: file.url.clone(),
        sha256: file.sha256.clone(),
        archive: file.archive.clone(),
        kind: model.kind.as_str().into(),
        paths: state.paths.clone(),
        tx,
        cancel,
    };

    let run_app = app.clone();
    let run_job_id = job_id.clone();
    tauri::async_runtime::spawn(async move {
        let _ = job.run().await;
        run_app.state::<AppState>().downloads.finish(&run_job_id);
    });

    // Forward progress to the frontend; ends when the job drops its sender.
    tauri::async_runtime::spawn(async move {
        while let Ok(progress) = rx.recv_async().await {
            let _ = app.emit(EVENT_DOWNLOAD_PROGRESS, &progress);
        }
    });

    Ok(job_id)
}

/// Cancel a download by job id (the model id is accepted too).
#[command]
pub async fn cancel_download(state: State<'_, AppState>, job_id: String) -> Result<()> {
    if state.downloads.cancel(&job_id) {
        Ok(())
    } else {
        Err(AppError::NotFound(format!("No active download: {}", job_id)))
    }
}

#[command]
pub async fn delete_model(state: State<'_, AppState>, model_id: String) -> Result<()> {
    if model_id.is_empty() || model_id.contains(['/', '\\']) || model_id.contains("..") {
        return Err(AppError::Validation("Invalid model id".into()));
    }
    state.downloads.cancel(&model_id);

    let (_, model_dir) = locate_model(&state.paths, &model_id)
        .ok_or_else(|| AppError::NotFound(format!("Model not found: {}", model_id)))?;

    // Release any loaded engine first: Windows cannot delete files that are still open.
    state.engines().evict(&model_id);

    if model_dir.exists() {
        std::fs::remove_dir_all(model_dir)?;
    }

    Ok(())
}

/// Import a model from a local file or folder (`kind` is "stt" or "tts").
/// STT: a whisper.cpp GGML `.bin` file (or a folder containing one).
/// TTS: a sherpa-onnx model folder (`*.onnx`, `tokens.txt`, `espeak-ng-data/`, optional `voices.bin`).
#[command]
pub async fn import_local_model(state: State<'_, AppState>, path: String, kind: String) -> Result<InstalledModel> {
    let kind = ModelKind::parse(&kind).ok_or_else(|| AppError::Validation("kind must be \"stt\" or \"tts\"".into()))?;
    let src_path = PathBuf::from(&path);
    if !src_path.exists() {
        return Err(AppError::NotFound("Source path does not exist".into()));
    }

    let model_id = if src_path.is_file() { src_path.file_stem() } else { src_path.file_name() }
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
        .ok_or_else(|| AppError::Validation("Invalid path".into()))?;

    if Catalog::load_embedded()?.get_model(&model_id).is_some() {
        return Err(AppError::Validation(format!("\"{}\" conflicts with a catalog model id", model_id)));
    }

    let dest_dir = state.paths.model_dir(kind.as_str(), &model_id);
    if dest_dir.exists() {
        return Err(AppError::Validation(format!("A model named \"{}\" is already installed", model_id)));
    }

    let manifest = {
        let dest_dir = dest_dir.clone();
        let model_id = model_id.clone();
        tauri::async_runtime::spawn_blocking(move || -> Result<ModelManifest> {
            let result = copy_model(&src_path, &dest_dir, &model_id);
            if result.is_err() {
                std::fs::remove_dir_all(&dest_dir).ok();
            }
            result
        })
        .await??
    };

    Ok(InstalledModel {
        id: model_id.clone(),
        kind,
        name: model_id,
        engine: default_engine(kind).into(),
        path: dest_dir.to_string_lossy().to_string(),
        manifest,
    })
}

fn copy_model(src: &Path, dest_dir: &Path, model_id: &str) -> Result<ModelManifest> {
    std::fs::create_dir_all(dest_dir)?;

    if src.is_file() {
        let name = src.file_name().ok_or_else(|| AppError::Validation("Invalid path".into()))?;
        std::fs::copy(src, dest_dir.join(name))?;
    } else {
        for entry in walkdir::WalkDir::new(src) {
            let entry = entry?;
            let rel = entry
                .path()
                .strip_prefix(src)
                .map_err(|e| AppError::Path(e.to_string()))?;
            let dest = dest_dir.join(rel);
            if entry.file_type().is_dir() {
                std::fs::create_dir_all(&dest)?;
            } else if entry.file_type().is_file() {
                std::fs::copy(entry.path(), &dest)?;
            }
        }
    }

    let (files, size_bytes) = list_dir_files(dest_dir)?;
    let manifest = ModelManifest {
        id: model_id.to_string(),
        version: "local".into(),
        sha256: String::new(),
        installed_at: Utc::now().timestamp_millis(),
        size_bytes,
        files,
    };
    std::fs::write(dest_dir.join("manifest.json"), serde_json::to_string_pretty(&manifest)?)?;
    Ok(manifest)
}
