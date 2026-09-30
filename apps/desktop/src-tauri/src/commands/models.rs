use std::path::{Path, PathBuf};
use chrono::Utc;
use tauri::{command, AppHandle, Emitter, Manager, State};
use uuid::Uuid;
use crate::catalog::{
    is_valid_model_id, validate_model_id, Catalog, CatalogModel, InstalledModel, ModelKind, ModelManifest,
    MAX_MODEL_ID_LEN,
};
use crate::commands::EVENT_DOWNLOAD_PROGRESS;
use crate::downloader::{ensure_free_space, list_dir_files, DownloadJob, DISK_MARGIN};
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
/// Ids that could not be a directory name of ours (traversal, drive prefixes...) find nothing.
pub(crate) fn locate_model(paths: &AppPaths, model_id: &str) -> Option<(ModelKind, PathBuf)> {
    if !is_valid_model_id(model_id) {
        return None;
    }
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
            if !dir.is_dir() || !is_valid_model_id(&id) || catalog.get_model(&id).is_some() {
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
    validate_model_id(&model_id)?;
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
        name: model.name.clone(),
        url: file.url.clone(),
        sha256: file.sha256.clone(),
        archive: file.archive.clone(),
        size_bytes: model.size_bytes,
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
    validate_model_id(&model_id)?;
    state.downloads.cancel(&model_id);

    let (_, model_dir) = locate_model(&state.paths, &model_id)
        .ok_or_else(|| AppError::NotFound(format!("Model not found: {}", model_id)))?;

    // Stop the engine first so it lets go of the model: Windows cannot delete open files.
    state.engine.shutdown();

    if model_dir.exists() {
        std::fs::remove_dir_all(model_dir)?;
    }

    Ok(())
}

/// Import a model from a local file or folder (`kind` is "stt" or "tts").
/// STT: a whisper.cpp GGML `.bin` file (or a folder containing one).
/// TTS: a sherpa-onnx model folder: Kokoro (`model.onnx`, `voices.bin`, `tokens.txt`,
/// `espeak-ng-data/`) or Piper/VITS (`<name>.onnx`, `tokens.txt`, `espeak-ng-data/`).
#[command]
pub async fn import_local_model(state: State<'_, AppState>, path: String, kind: String) -> Result<InstalledModel> {
    let kind = ModelKind::parse(&kind).ok_or_else(|| AppError::Validation("kind must be \"stt\" or \"tts\"".into()))?;
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || import_model(&paths, Path::new(&path), kind)).await?
}

/// Validate `src` as a model of `kind`, copy it into the models dir and write its manifest.
/// Blocking. On any failure nothing is left behind.
pub(crate) fn import_model(paths: &AppPaths, src: &Path, kind: ModelKind) -> Result<InstalledModel> {
    // Resolve the chosen path once; below it, links are never followed.
    let src = std::fs::canonicalize(src).map_err(|_| AppError::NotFound("Source path does not exist".into()))?;
    let is_file = std::fs::metadata(&src)?.is_file();

    let name = if is_file { src.file_stem() } else { src.file_name() }
        .and_then(|n| n.to_str())
        .ok_or_else(|| AppError::Validation("Invalid path".into()))?;
    let model_id = model_id_from_name(name)?;

    if Catalog::load_embedded()?.get_model(&model_id).is_some() {
        return Err(AppError::Validation(format!("\"{}\" conflicts with a catalog model id", model_id)));
    }
    if locate_model(paths, &model_id).is_some() {
        return Err(AppError::Validation(format!("A model named \"{}\" is already installed", model_id)));
    }

    validate_import_source(&src, kind)?;
    let size = source_size(&src)?;
    ensure_free_space(&paths.home, size.saturating_add(DISK_MARGIN), &model_id)?;

    let dest_dir = paths.model_dir(kind.as_str(), &model_id);
    if dest_dir.exists() {
        // No manifest (checked above): leftovers of an interrupted install.
        std::fs::remove_dir_all(&dest_dir)?;
    }

    // Copy into a scratch dir and move it into place at the end, so the models dir never
    // holds a half-copied model.
    let scratch = paths.cache_downloads.join(format!("{}.import", model_id));
    if scratch.exists() {
        std::fs::remove_dir_all(&scratch)?;
    }
    let result = copy_model(&src, &scratch, &model_id).and_then(|manifest| {
        if let Some(parent) = dest_dir.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(&scratch, &dest_dir)?;
        Ok(manifest)
    });
    if result.is_err() {
        std::fs::remove_dir_all(&scratch).ok();
    }
    let manifest = result?;

    Ok(InstalledModel {
        id: model_id.clone(),
        kind,
        name: model_id,
        engine: default_engine(kind).into(),
        path: dest_dir.to_string_lossy().to_string(),
        manifest,
    })
}

/// A valid model id from a file or folder name: characters outside `[A-Za-z0-9._-]` become
/// `-`, leading dots and dashes and trailing dashes are dropped, and it is cut to the max length.
pub(crate) fn model_id_from_name(name: &str) -> Result<String> {
    let mapped: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') { c } else { '-' })
        .collect();
    let trimmed = mapped.trim_start_matches(['.', '-']).trim_end_matches('-');
    let id: String = trimmed.chars().take(MAX_MODEL_ID_LEN).collect();
    if !is_valid_model_id(&id) || !id.bytes().any(|b| b.is_ascii_alphanumeric()) {
        return Err(AppError::Validation(format!(
            "Cannot derive a model name from \"{}\"; rename it using letters, digits, '.', '_' or '-'",
            name
        )));
    }
    Ok(id)
}

/// whisper.cpp's `GGML_FILE_MAGIC` (0x67676d6c, "ggml"), as stored: a little-endian u32.
const GGML_MAGIC: [u8; 4] = 0x6767_6d6c_u32.to_le_bytes();
/// GGUF files start with these bytes. whisper.cpp does not load them.
const GGUF_MAGIC: [u8; 4] = *b"GGUF";

/// Metadata without following links: a link is neither a file nor a dir here.
fn is_real_file(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.is_file())
}

fn is_real_dir(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.is_dir())
}

fn has_extension(path: &Path, ext: &str) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

/// Top-level regular files in `dir` with extension `ext`, sorted (the engine picks the first).
fn files_with_extension(dir: &Path, ext: &str) -> Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| has_extension(p, ext) && is_real_file(p))
        .collect();
    files.sort();
    Ok(files)
}

/// Check that `src` looks like a model the engine can load, before copying anything.
fn validate_import_source(src: &Path, kind: ModelKind) -> Result<()> {
    match kind {
        ModelKind::Stt => {
            let bin = if is_real_file(src) {
                if !has_extension(src, "bin") {
                    return Err(AppError::Validation(
                        "A speech-to-text model must be a whisper.cpp .bin file".into(),
                    ));
                }
                src.to_path_buf()
            } else {
                files_with_extension(src, "bin")?.into_iter().next().ok_or_else(|| {
                    AppError::Validation("The folder has no whisper.cpp .bin model file".into())
                })?
            };
            check_ggml_magic(&bin)
        }
        ModelKind::Tts => {
            if !is_real_dir(src) {
                return Err(AppError::Validation(
                    "A text-to-speech model is imported as a folder (with tokens.txt and espeak-ng-data)".into(),
                ));
            }
            let mut missing = Vec::new();
            if !is_real_file(&src.join("tokens.txt")) {
                missing.push("tokens.txt");
            }
            if !is_real_dir(&src.join("espeak-ng-data")) {
                missing.push("espeak-ng-data/");
            }
            // Same rule as the engine: voices.bin means Kokoro, which loads model.onnx.
            let kokoro = is_real_file(&src.join("voices.bin"));
            if kokoro {
                if !is_real_file(&src.join("model.onnx")) {
                    missing.push("model.onnx");
                }
            } else if files_with_extension(src, "onnx")?.is_empty() {
                missing.push("a .onnx model");
            }
            if !missing.is_empty() {
                return Err(AppError::Validation(format!(
                    "Not a {} model folder, missing: {}",
                    if kokoro { "Kokoro" } else { "Piper" },
                    missing.join(", ")
                )));
            }
            Ok(())
        }
    }
}

fn check_ggml_magic(path: &Path) -> Result<()> {
    use std::io::Read;
    let mut magic = [0u8; 4];
    let read = std::fs::File::open(path).and_then(|mut f| f.read_exact(&mut magic));
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    match read {
        Ok(()) if magic == GGML_MAGIC => Ok(()),
        Ok(()) if magic == GGUF_MAGIC => Err(AppError::Validation(format!(
            "{} is a GGUF file; Whisper needs a whisper.cpp GGML model (ggml-*.bin)",
            name
        ))),
        Ok(()) => Err(AppError::Validation(format!("{} is not a whisper.cpp GGML model", name))),
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Err(AppError::Validation(format!(
            "{} is too small to be a whisper.cpp GGML model",
            name
        ))),
        Err(e) => Err(e.into()),
    }
}

/// Total bytes to copy. Refuses symbolic links and special files anywhere in the tree.
fn source_size(src: &Path) -> Result<u64> {
    let mut total = 0u64;
    for entry in walkdir::WalkDir::new(src).follow_links(false) {
        let entry = entry?;
        let file_type = entry.file_type();
        if file_type.is_file() {
            total = total.saturating_add(entry.metadata()?.len());
        } else if !file_type.is_dir() {
            return Err(unsupported_entry(entry.path()));
        }
    }
    Ok(total)
}

fn unsupported_entry(path: &Path) -> AppError {
    AppError::Validation(format!(
        "{} is a link or special file; copy the real files into the model folder and import again",
        path.display()
    ))
}

/// Copy `src` into `dest_dir` (which must not exist yet) without following links, then
/// write the manifest.
fn copy_model(src: &Path, dest_dir: &Path, model_id: &str) -> Result<ModelManifest> {
    std::fs::create_dir_all(dest_dir)?;

    if is_real_file(src) {
        let name = src.file_name().ok_or_else(|| AppError::Validation("Invalid path".into()))?;
        std::fs::copy(src, dest_dir.join(name))?;
    } else {
        for entry in walkdir::WalkDir::new(src).follow_links(false) {
            let entry = entry?;
            let rel = entry
                .path()
                .strip_prefix(src)
                .map_err(|e| AppError::Path(e.to_string()))?;
            let dest = dest_dir.join(rel);
            let file_type = entry.file_type();
            if file_type.is_dir() {
                std::fs::create_dir_all(&dest)?;
            } else if file_type.is_file() {
                std::fs::copy(entry.path(), &dest)?;
            } else {
                // Checked up front too; this closes the window where a file became a link.
                return Err(unsupported_entry(entry.path()));
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

#[cfg(test)]
#[path = "models_tests.rs"]
mod tests;
