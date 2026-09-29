use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use chrono::Utc;
use serde::Serialize;
use tauri::{command, AppHandle, Emitter, Manager, State};
use uuid::Uuid;
use crate::audio::{decode, resample, wav};
use crate::catalog::ModelKind;
use crate::commands::models::locate_model;
use crate::commands::tts::make_title;
use crate::commands::{finish_job, job_control, resolve_path, to_stored_path, EVENT_MIC_LEVEL, EVENT_STT_DONE};
use crate::db::{insert_history, HistoryItem, HistoryKind};
use crate::engines::stt_whisper::WhisperEngine;
use crate::engines::{JobControl, SttEngine, SttOptions};
use crate::error::{AppError, Result};
use crate::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingResult {
    /// Path of the saved WAV, relative to the Talkr home (pass it to `transcribe_file`).
    pub temp_audio_path: String,
    pub duration_ms: i64,
}

/// Get the cached STT engine for `model_id`, loading it if needed. Blocking.
pub(crate) fn load_stt(state: &AppState, model_id: &str) -> Result<Arc<dyn SttEngine>> {
    let mut engines = state.engines();
    if let Some(engine) = engines.get_stt(model_id) {
        return Ok(engine);
    }
    let (kind, dir) = locate_model(&state.paths, model_id)
        .filter(|(_, dir)| dir.join("manifest.json").is_file())
        .ok_or_else(|| AppError::NotFound(format!("Model not installed: {}", model_id)))?;
    if kind != ModelKind::Stt {
        return Err(AppError::Validation(format!("{} is not a speech-to-text model", model_id)));
    }
    let engine: Arc<dyn SttEngine> = Arc::new(WhisperEngine::new(&dir, model_id.to_string())?);
    engines.set_stt(engine.clone());
    Ok(engine)
}

/// Start recording from the default microphone. Emits `mic://level` (RMS, 0..1) ~20x/s.
#[command]
pub async fn start_recording(state: State<'_, AppState>, app: AppHandle) -> Result<()> {
    let (recording, level) = {
        let mut recorder = state.recorder();
        recorder.start()?;
        (recorder.recording_flag(), recorder.level_handle())
    };

    tauri::async_runtime::spawn(async move {
        while recording.load(Ordering::Relaxed) {
            let _ = app.emit(EVENT_MIC_LEVEL, f32::from_bits(level.load(Ordering::Relaxed)));
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let _ = app.emit(EVENT_MIC_LEVEL, 0.0f32);
    });

    Ok(())
}

/// Stop recording and save the audio as a WAV file under `~/.talkr/audio`.
#[command]
pub async fn stop_recording(state: State<'_, AppState>) -> Result<RecordingResult> {
    let (samples, sample_rate) = {
        let mut recorder = state.recorder();
        let samples = recorder.stop()?;
        (samples, recorder.sample_rate())
    };

    if samples.is_empty() {
        return Err(AppError::Audio("No audio was captured".into()));
    }

    let duration_ms = (samples.len() as f64 / sample_rate as f64 * 1000.0) as i64;

    let paths = state.paths.clone();
    let audio_path = paths.audio_path("wav");
    if let Some(parent) = audio_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let write_path = audio_path.clone();
    tauri::async_runtime::spawn_blocking(move || wav::write_wav(&write_path, &samples, sample_rate)).await??;

    Ok(RecordingResult {
        temp_audio_path: to_stored_path(&paths, &audio_path),
        duration_ms,
    })
}

/// Current microphone RMS level (0 when not recording).
#[command]
pub async fn get_input_level(state: State<'_, AppState>) -> Result<f32> {
    Ok(state.recorder().level())
}

/// Start a transcription job for an audio file (absolute path, or a path relative to the Talkr
/// home such as the one returned by `stop_recording`). `language` defaults to the settings value;
/// `translate` = translate to English (multilingual models only).
/// Completion is reported through `stt://done` (or `job://error`), progress through `job://progress`.
#[command]
pub async fn transcribe_file(
    state: State<'_, AppState>,
    app: AppHandle,
    path: String,
    model_id: String,
    language: Option<String>,
    translate: Option<bool>,
) -> Result<String> {
    let full_path = resolve_path(&state.paths, &path);
    if !full_path.is_file() {
        return Err(AppError::NotFound(format!("Audio file not found: {}", path)));
    }

    let (language, save_recordings) = {
        let settings = state.settings();
        (language.unwrap_or_else(|| settings.stt_language.clone()), settings.save_recordings)
    };

    let job_id = Uuid::new_v4().to_string();
    let cancel = state.jobs.register(&job_id);
    let ctl = job_control(&app, &job_id, cancel);

    let job_app = app.clone();
    let job = job_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = job_app.state::<AppState>();
        let opts = SttOptions {
            language: Some(language).filter(|l| !l.is_empty()),
            translate: translate.unwrap_or(false),
            threads: state.cpu_threads(),
        };
        let result = run_transcription(&state, &ctl, &job, &full_path, &model_id, &opts, save_recordings);
        finish_job(&job_app, &state, &job, EVENT_STT_DONE, result);
    });

    Ok(job_id)
}

fn run_transcription(
    state: &AppState,
    ctl: &JobControl,
    job_id: &str,
    full_path: &std::path::Path,
    model_id: &str,
    opts: &SttOptions,
    save_recordings: bool,
) -> Result<HistoryItem> {
    let start = std::time::Instant::now();
    ctl.progress(0.0);

    let stt = load_stt(state, model_id)?;
    if ctl.is_cancelled() {
        return Err(AppError::Cancelled);
    }

    let (samples, sample_rate) = decode::decode_audio_file(full_path)?;
    let samples = resample::resample_to_16k_mono(&samples, sample_rate)?;
    if ctl.is_cancelled() {
        return Err(AppError::Cancelled);
    }

    let transcript = stt.transcribe(&samples, opts, ctl)?;

    let paths = &state.paths;
    let duration_ms = (samples.len() as f64 / 16000.0 * 1000.0) as i64;
    let is_own_recording = full_path.starts_with(&paths.audio);

    // Recordings made in-app are discarded after transcription when the user opted out of keeping them.
    let audio_path = if is_own_recording && !save_recordings {
        std::fs::remove_file(full_path).ok();
        None
    } else {
        Some(to_stored_path(paths, full_path))
    };

    let title = if transcript.text.trim().is_empty() {
        full_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".into())
    } else {
        make_title(&transcript.text)
    };

    let item = HistoryItem {
        id: job_id.to_string(),
        kind: HistoryKind::Stt,
        created_at: Utc::now().timestamp_millis(),
        title,
        text: transcript.text.clone(),
        audio_path,
        duration_ms: Some(duration_ms),
        model_id: model_id.to_string(),
        voice_id: None,
        language: transcript.language,
        device: "cpu".into(),
        processing_ms: start.elapsed().as_millis() as i64,
        favorite: false,
        segments_json: Some(serde_json::to_string(&transcript.segments)?),
    };

    insert_history(paths, &item)?;
    Ok(item)
}

/// Request cancellation of a running synthesis/transcription job.
#[command]
pub async fn cancel_job(state: State<'_, AppState>, job_id: String) -> Result<()> {
    if state.jobs.cancel(&job_id) {
        Ok(())
    } else {
        Err(AppError::NotFound(format!("No running job: {}", job_id)))
    }
}
