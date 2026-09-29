use std::sync::Arc;
use chrono::Utc;
use tauri::{command, AppHandle, Manager, State};
use uuid::Uuid;
use crate::audio::wav;
use crate::catalog::ModelKind;
use crate::commands::models::locate_model;
use crate::commands::{finish_job, job_control, to_stored_path, EVENT_TTS_DONE};
use crate::db::{insert_history, HistoryItem, HistoryKind};
use crate::engines::tts_sherpa::SherpaTtsEngine;
use crate::engines::{JobControl, TtsEngine, TtsOptions, Voice};
use crate::error::{AppError, Result};
use crate::AppState;

/// Get the cached TTS engine for `model_id`, loading it if needed. Blocking.
pub(crate) fn load_tts(state: &AppState, model_id: &str) -> Result<Arc<dyn TtsEngine>> {
    let mut engines = state.engines();
    if let Some(engine) = engines.get_tts(model_id) {
        return Ok(engine);
    }
    let (kind, dir) = locate_model(&state.paths, model_id)
        .filter(|(_, dir)| dir.join("manifest.json").is_file())
        .ok_or_else(|| AppError::NotFound(format!("Model not installed: {}", model_id)))?;
    if kind != ModelKind::Tts {
        return Err(AppError::Validation(format!("{} is not a text-to-speech model", model_id)));
    }
    let engine: Arc<dyn TtsEngine> = Arc::new(SherpaTtsEngine::new(&dir, model_id.to_string(), state.cpu_threads())?);
    engines.set_tts(engine.clone());
    Ok(engine)
}

#[command]
pub async fn list_voices(app: AppHandle, model_id: String) -> Result<Vec<Voice>> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        Ok(load_tts(&state, &model_id)?.voices())
    })
    .await?
}

/// Start a synthesis job. Returns the job id; completion is reported through
/// `tts://done` (or `job://error`) and progress through `job://progress`.
#[command]
pub async fn synthesize(
    state: State<'_, AppState>,
    app: AppHandle,
    text: String,
    model_id: String,
    voice_id: String,
    speed: Option<f32>,
) -> Result<String> {
    if text.trim().is_empty() {
        return Err(AppError::Validation("Text is empty".into()));
    }
    let speed = speed.unwrap_or_else(|| state.settings().speech_rate);

    let job_id = Uuid::new_v4().to_string();
    let cancel = state.jobs.register(&job_id);
    let ctl = job_control(&app, &job_id, cancel);

    let job_app = app.clone();
    let job = job_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = job_app.state::<AppState>();
        let result = run_synthesis(&state, &ctl, &job, &text, &model_id, &voice_id, speed);
        finish_job(&job_app, &state, &job, EVENT_TTS_DONE, result);
    });

    Ok(job_id)
}

fn run_synthesis(
    state: &AppState,
    ctl: &JobControl,
    job_id: &str,
    text: &str,
    model_id: &str,
    voice_id: &str,
    speed: f32,
) -> Result<HistoryItem> {
    let start = std::time::Instant::now();
    ctl.progress(0.0);
    let tts = load_tts(state, model_id)?;
    if ctl.is_cancelled() {
        return Err(AppError::Cancelled);
    }

    let opts = TtsOptions {
        voice_id: voice_id.to_string(),
        speed,
    };
    let audio = tts.synthesize(text, &opts, ctl)?;
    if audio.samples.is_empty() || audio.sample_rate == 0 {
        return Err(AppError::Engine("Engine produced no audio".into()));
    }

    let paths = &state.paths;
    let audio_path = paths.audio_path("wav");
    if let Some(parent) = audio_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    wav::write_wav(&audio_path, &audio.samples, audio.sample_rate)?;

    let duration_ms = (audio.samples.len() as f64 / audio.sample_rate as f64 * 1000.0) as i64;
    let title = make_title(text);

    let item = HistoryItem {
        id: job_id.to_string(),
        kind: HistoryKind::Tts,
        created_at: Utc::now().timestamp_millis(),
        title,
        text: text.to_string(),
        audio_path: Some(to_stored_path(paths, &audio_path)),
        duration_ms: Some(duration_ms),
        model_id: model_id.to_string(),
        voice_id: Some(voice_id.to_string()),
        language: None,
        device: "cpu".into(),
        processing_ms: start.elapsed().as_millis() as i64,
        favorite: false,
        segments_json: None,
    };

    insert_history(paths, &item)?;
    Ok(item)
}

pub(crate) fn make_title(text: &str) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut title: String = line.chars().take(60).collect();
    if line.chars().count() > 60 {
        title.push('…');
    }
    if title.is_empty() {
        title = "Untitled".into();
    }
    title
}
