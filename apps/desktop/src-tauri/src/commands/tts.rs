use chrono::Utc;
use talkr_protocol::{Event, ModelRef, Op, SynthesizeJob, Voice};
use tauri::{command, AppHandle, Manager, State};
use uuid::Uuid;
use crate::catalog::ModelKind;
use crate::commands::models::locate_model;
use crate::commands::{catch_panic, ensure_memory_for_model, finish_job, progress_emitter, to_stored_path, EVENT_TTS_DONE};
use crate::db::{insert_history, HistoryItem, HistoryKind};
use crate::engine_host::failure_to_error;
use crate::error::{AppError, Result};
use crate::AppState;

/// Resolve an installed text-to-speech model for the engine.
fn tts_model(state: &AppState, model_id: &str) -> Result<ModelRef> {
    let (kind, dir) = locate_model(&state.paths, model_id)
        .filter(|(_, dir)| dir.join("manifest.json").is_file())
        .ok_or_else(|| AppError::NotFound(format!("Model not installed: {}", model_id)))?;
    if kind != ModelKind::Tts {
        return Err(AppError::Validation(format!("{} is not a text-to-speech model", model_id)));
    }
    ensure_memory_for_model(&dir, model_id)?;
    // TTS models are small and fast on the CPU; sherpa-onnx's prebuilt libraries are CPU-only.
    Ok(ModelRef { model_id: model_id.to_string(), dir, threads: state.cpu_threads(), gpu: false })
}

#[command]
pub async fn list_voices(app: AppHandle, model_id: String) -> Result<Vec<Voice>> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let model = tts_model(&state, &model_id)?;
        let id = Uuid::new_v4().to_string();
        match state.engine.run(&id, &|_| Op::Voices(model.clone()), false, &|_| {})? {
            Event::Voices { voices, .. } => Ok(voices),
            Event::Failed { kind, error, .. } => Err(failure_to_error(kind, error)),
            other => Err(AppError::Engine(format!("Unexpected reply from the engine: {:?}", other))),
        }
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
    // sherpa-rs turns the text into a C string and panics on a NUL byte (text pasted from a PDF
    // can carry one).
    let text = text.replace('\0', "");
    if text.trim().is_empty() {
        return Err(AppError::Validation("Text is empty".into()));
    }
    let speed = speed.unwrap_or_else(|| state.settings().speech_rate);

    let job_id = Uuid::new_v4().to_string();
    state.jobs.register(&job_id);

    let job_app = app.clone();
    let job = job_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = job_app.state::<AppState>();
        let result = catch_panic(|| run_synthesis(&job_app, &state, &job, &text, &model_id, &voice_id, speed));
        finish_job(&job_app, &state, &job, EVENT_TTS_DONE, result);
    });

    Ok(job_id)
}

fn run_synthesis(
    app: &AppHandle,
    state: &AppState,
    job_id: &str,
    text: &str,
    model_id: &str,
    voice_id: &str,
    speed: f32,
) -> Result<HistoryItem> {
    let start = std::time::Instant::now();
    let progress = progress_emitter(app, job_id);
    progress(0.0);
    let model = tts_model(state, model_id)?;
    if state.jobs.is_cancelled(job_id) {
        return Err(AppError::Cancelled);
    }

    let paths = &state.paths;
    let audio_path = paths.audio_path("wav");
    let make_op = |_gpu: bool| {
        Op::Synthesize(SynthesizeJob {
            model: model.clone(),
            text: text.to_string(),
            voice_id: voice_id.to_string(),
            speed,
            out_path: audio_path.clone(),
        })
    };
    let (duration_ms, device) = match state.engine.run(job_id, &make_op, false, &progress)? {
        Event::Synthesized { duration_ms, device, .. } => (duration_ms, device),
        Event::Failed { kind, error, .. } => return Err(failure_to_error(kind, error)),
        other => return Err(AppError::Engine(format!("Unexpected reply from the engine: {:?}", other))),
    };
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
        device,
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
