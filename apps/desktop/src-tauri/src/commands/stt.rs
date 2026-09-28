use tauri::{command, State, AppHandle, Emitter};
use crate::lib::AppState;
use crate::error::{AppError, Result};
use crate::engines::{EngineRegistry, SttOptions, Transcript};
use crate::audio::{record, decode, resample, wav};
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

type RecorderState = Arc<Mutex<Option<crate::audio::record::AudioRecorder>>>;

fn get_recorder(state: &State<'_, AppState>) -> Arc<Mutex<crate::audio::record::AudioRecorder>> {
    let mut guard = state.try_get::<RecorderState>().expect("Recorder not initialized");
    let mut recorder_opt = guard.lock().unwrap();
    if recorder_opt.is_none() {
        *recorder_opt = Some(crate::audio::record::AudioRecorder::new().unwrap());
    }
    Arc::new(Mutex::new(recorder_opt.as_ref().unwrap().clone()))
}

#[command]
pub async fn start_recording(state: State<'_, AppState>, app: AppHandle) -> Result<()> {
    let recorder = get_recorder(&state);
    let mut recorder = recorder.lock().unwrap();
    recorder.start()?;

    let level_rx = recorder.get_level_receiver();
    let app_handle = app.clone();

    tokio::spawn(async move {
        while let Ok(level) = level_rx.recv_async().await {
            let _ = app_handle.emit("mic://level", level);
        }
    });

    Ok(())
}

#[command]
pub async fn stop_recording(state: State<'_, AppState>) -> Result<serde_json::Value> {
    let recorder = get_recorder(&state);
    let mut recorder = recorder.lock().unwrap();
    let samples = recorder.stop()?;

    let sample_rate = recorder.sample_rate();
    let duration_ms = (samples.len() as f32 / sample_rate as f32 * 1000.0) as i64;

    let paths = state.paths.clone();
    let audio_path = paths.audio_path("wav");
    std::fs::create_dir_all(audio_path.parent().unwrap())?;
    wav::write_wav(&audio_path, &samples, sample_rate)?;

    let rel_path = audio_path.strip_prefix(&paths.home).unwrap().to_string_lossy().to_string();

    Ok(serde_json::json!({
        "tempAudioPath": rel_path,
        "durationMs": duration_ms
    }))
}

#[command]
pub async fn get_input_level(state: State<'_, AppState>) -> Result<f32> {
    let recorder = get_recorder(&state);
    let recorder = recorder.lock().unwrap();
    let level_rx = recorder.get_level_receiver();
    Ok(level_rx.try_recv().unwrap_or(0.0))
}

#[command]
pub async fn transcribe_file(
    state: State<'_, AppState>,
    app: AppHandle,
    path: String,
    model_id: String,
    language: Option<String>,
) -> Result<String> {
    let job_id = Uuid::new_v4().to_string();

    let registry = get_engine_registry(&state);
    let mut registry = registry.lock().unwrap();

    if registry.get_stt().is_none() || registry.get_stt().as_ref().unwrap().model_id() != model_id {
        let catalog = crate::catalog::Catalog::load_embedded()?;
        let model = catalog.get_model(&model_id)
            .ok_or_else(|| AppError::NotFound(format!("Model not found: {}", model_id)))?;

        let model_dir = state.paths.model_dir("stt", &model_id);
        let model_file = model_dir.join(format!("ggml-{}.bin", model_id));
        let engine = crate::engines::stt_whisper::WhisperEngine::new(&model_file, model_id.clone())?;
        registry.set_stt(Arc::new(engine));
    }

    let stt = registry.get_stt().ok_or_else(|| AppError::Engine("STT engine not loaded".into()))?;
    let stt = stt.clone();
    drop(registry);

    let paths = state.paths.clone();
    let app_handle = app.clone();

    tokio::task::spawn_blocking(move || {
        let start = std::time::Instant::now();

        let full_path = paths.home.join(&path);
        let (samples, sample_rate) = decode::decode_audio_file(&full_path)?;
        let samples = resample::resample_to_16k_mono(&samples, sample_rate)?;

        let transcript = stt.transcribe(&samples, &SttOptions { language, translate: false }, &|p| {
            let _ = app_handle.emit("job://progress", serde_json::json!({ "jobId": job_id, "progress": p }));
        })?;

        let duration_ms = (samples.len() as f32 / 16000.0 * 1000.0) as i64;
        let title = transcript.text.chars().take(60).collect::<String>();

        let item = HistoryItem {
            id: job_id.clone(),
            kind: HistoryKind::Stt,
            created_at: Utc::now().timestamp_millis(),
            title,
            text: transcript.text.clone(),
            audio_path: Some(path.clone()),
            duration_ms: Some(duration_ms),
            model_id: model_id.clone(),
            voice_id: None,
            language: transcript.language,
            device: "cpu".into(),
            processing_ms: start.elapsed().as_millis() as i64,
            favorite: false,
            segments_json: Some(serde_json::to_string(&transcript.segments)?),
        };

        insert_history(&paths, &item)?;

        let _ = app_handle.emit("stt://done", serde_json::json!({ "historyItem": item }));

        Ok::<String, AppError>(job_id)
    });

    Ok(job_id)
}

#[command]
pub async fn cancel_job(state: State<'_, AppState>, job_id: String) -> Result<()> {
    let _ = state.try_get::<RecorderState>().map(|r| {
        let recorder = r.lock().unwrap();
    });
    Ok(())
}