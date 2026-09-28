use tauri::{command, State, AppHandle, Emitter};
use crate::lib::AppState;
use crate::error::{AppError, Result};
use crate::engines::{EngineRegistry, TtsOptions, AudioBuffer};
use crate::audio::wav;
use crate::db::{insert_history, HistoryItem, HistoryKind};
use crate::paths::AppPaths;
use std::sync::{Arc, Mutex};
use uuid::Uuid;
use chrono::Utc;

type EngineRegistryState = Arc<Mutex<Option<EngineRegistry>>>;

fn get_engine_registry(state: &State<'_, AppState>) -> Arc<Mutex<EngineRegistry>> {
    let mut guard = state.try_get::<EngineRegistryState>().expect("EngineRegistry not initialized");
    let mut registry_opt = guard.lock().unwrap();
    if registry_opt.is_none() {
        *registry_opt = Some(EngineRegistry::new());
    }
    Arc::new(Mutex::new(registry_opt.as_ref().unwrap().clone()))
}

#[command]
pub async fn list_voices(state: State<'_, AppState>, model_id: String) -> Result<Vec<crate::engines::Voice>> {
    let registry = get_engine_registry(&state);
    let mut registry = registry.lock().unwrap();

    if registry.get_tts().is_none() || registry.get_tts().as_ref().unwrap().model_id() != model_id {
        let catalog = crate::catalog::Catalog::load_embedded()?;
        let model = catalog.get_model(&model_id)
            .ok_or_else(|| AppError::NotFound(format!("Model not found: {}", model_id)))?;

        let model_dir = state.paths.model_dir("tts", &model_id);
        let engine = crate::engines::tts_sherpa::SherpaTtsEngine::new(&model_dir, model_id.clone())?;
        registry.set_tts(Arc::new(engine));
    }

    let tts = registry.get_tts().ok_or_else(|| AppError::Engine("TTS engine not loaded".into()))?;
    Ok(tts.voices())
}

#[command]
pub async fn synthesize(
    state: State<'_, AppState>,
    app: AppHandle,
    text: String,
    model_id: String,
    voice_id: String,
    speed: f32,
) -> Result<String> {
    let job_id = Uuid::new_v4().to_string();

    let registry = get_engine_registry(&state);
    let mut registry = registry.lock().unwrap();

    if registry.get_tts().is_none() || registry.get_tts().as_ref().unwrap().model_id() != model_id {
        let catalog = crate::catalog::Catalog::load_embedded()?;
        let model = catalog.get_model(&model_id)
            .ok_or_else(|| AppError::NotFound(format!("Model not found: {}", model_id)))?;

        let model_dir = state.paths.model_dir("tts", &model_id);
        let engine = crate::engines::tts_sherpa::SherpaTtsEngine::new(&model_dir, model_id.clone())?;
        registry.set_tts(Arc::new(engine));
    }

    let tts = registry.get_tts().ok_or_else(|| AppError::Engine("TTS engine not loaded".into()))?;
    let tts = tts.clone();
    drop(registry);

    let paths = state.paths.clone();
    let app_handle = app.clone();

    tokio::task::spawn_blocking(move || {
        let start = std::time::Instant::now();
        let audio = tts.synthesize(&text, &TtsOptions { voice_id: voice_id.clone(), speed }, &|p| {
            let _ = app_handle.emit("job://progress", serde_json::json!({ "jobId": job_id, "progress": p }));
        })?;

        let audio_path = paths.audio_path("wav");
        std::fs::create_dir_all(audio_path.parent().unwrap())?;
        wav::write_wav(&audio_path, &audio.samples, audio.sample_rate)?;

        let duration_ms = (audio.samples.len() as f32 / audio.sample_rate as f32 * 1000.0) as i64;
        let title = text.chars().take(60).collect::<String>();

        let item = HistoryItem {
            id: job_id.clone(),
            kind: HistoryKind::Tts,
            created_at: Utc::now().timestamp_millis(),
            title,
            text: text.clone(),
            audio_path: Some(audio_path.strip_prefix(&paths.home).unwrap().to_string_lossy().to_string()),
            duration_ms: Some(duration_ms),
            model_id: model_id.clone(),
            voice_id: Some(voice_id),
            language: None,
            device: "cpu".into(),
            processing_ms: start.elapsed().as_millis() as i64,
            favorite: false,
            segments_json: None,
        };

        insert_history(&paths, &item)?;

        let _ = app_handle.emit("tts://done", serde_json::json!({ "historyItem": item }));

        Ok::<String, AppError>(job_id)
    });

    Ok(job_id)
}