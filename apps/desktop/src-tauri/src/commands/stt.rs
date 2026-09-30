use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::Duration;
use chrono::Utc;
use serde::Serialize;
use talkr_protocol::{Event, ModelRef, Op, TranscribeJob};
use tauri::{command, AppHandle, Emitter, Manager, State};
use uuid::Uuid;
use crate::audio::wav;
use crate::catalog::ModelKind;
use crate::commands::models::locate_model;
use crate::commands::tts::make_title;
use crate::commands::{
    catch_panic, ensure_memory_for_model, finish_job, is_owned_audio, progress_emitter, resolve_path, to_stored_path,
    EVENT_MIC_LEVEL, EVENT_STT_DONE,
};
use crate::db::{insert_history, HistoryItem, HistoryKind};
use crate::engine_host::failure_to_error;
use crate::error::{AppError, Result};
use crate::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingResult {
    /// Path of the saved WAV, relative to the Talkr home (pass it to `transcribe_file`).
    pub temp_audio_path: String,
    pub duration_ms: i64,
}

/// Resolve an installed speech-to-text model for the engine, checking up front what can be
/// checked (kind, CPU support, free memory) so the common failures get a clear message.
pub(crate) fn stt_model(state: &AppState, model_id: &str, gpu: bool) -> Result<ModelRef> {
    let (kind, dir) = locate_model(&state.paths, model_id)
        .filter(|(_, dir)| dir.join("manifest.json").is_file())
        .ok_or_else(|| AppError::NotFound(format!("Model not installed: {}", model_id)))?;
    if kind != ModelKind::Stt {
        return Err(AppError::Validation(format!("{} is not a speech-to-text model", model_id)));
    }
    check_cpu_support()?;
    // On the GPU the weights live in video memory; the engine checks that fits itself.
    if !gpu {
        ensure_memory_for_model(&dir, model_id)?;
    }
    Ok(ModelRef { model_id: model_id.to_string(), dir, threads: state.cpu_threads(), gpu })
}

/// whisper.cpp is compiled for x86-64 CPUs with AVX2, FMA, F16C and BMI2 (roughly 2013 onwards; the
/// release workflow sets GGML_NATIVE=OFF so it is not tuned to the build machine). On an older CPU
/// the first instruction it hits would kill the app, so say so instead.
fn check_cpu_support() -> Result<()> {
    #[cfg(target_arch = "x86_64")]
    {
        let missing: Vec<&str> = [
            ("AVX2", std::arch::is_x86_feature_detected!("avx2")),
            ("FMA", std::arch::is_x86_feature_detected!("fma")),
            ("F16C", std::arch::is_x86_feature_detected!("f16c")),
            ("BMI2", std::arch::is_x86_feature_detected!("bmi2")),
        ]
        .into_iter()
        .filter(|(_, ok)| !ok)
        .map(|(name, _)| name)
        .collect();
        if !missing.is_empty() {
            return Err(AppError::Validation(format!(
                "This processor lacks {}, which speech to text needs. Text to speech still works.",
                missing.join(", ")
            )));
        }
    }
    Ok(())
}

/// Start recording from the default microphone. Emits `mic://level` (RMS, 0..1) ~20x/s.
#[command]
pub async fn start_recording(app: AppHandle) -> Result<()> {
    // Opening the device can take seconds (or time out) with some drivers; keep it off the
    // async runtime.
    let recorder_app = app.clone();
    let (recording, level) = tauri::async_runtime::spawn_blocking(move || -> Result<_> {
        let state = recorder_app.state::<AppState>();
        let mut recorder = state.recorder();
        recorder.start()?;
        Ok((recorder.recording_flag(), recorder.level_handle()))
    })
    .await??;

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
pub async fn stop_recording(state: State<'_, AppState>, app: AppHandle) -> Result<RecordingResult> {
    let (samples, sample_rate, limit_reached) = tauri::async_runtime::spawn_blocking(move || -> Result<_> {
        let state = app.state::<AppState>();
        let mut recorder = state.recorder();
        let limit_reached = recorder.limit_reached();
        let samples = recorder.stop()?;
        Ok((samples, recorder.sample_rate(), limit_reached))
    })
    .await??;
    if limit_reached {
        log::warn!("recording hit the length limit; keeping what was captured");
    }

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
    state.jobs.register(&job_id);

    let job_app = app.clone();
    let job = job_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = job_app.state::<AppState>();
        let request = SttRequest {
            path: &full_path,
            model_id: &model_id,
            language: Some(language).filter(|l| !l.is_empty()),
            translate: translate.unwrap_or(false),
            save_recordings,
        };
        let result = catch_panic(|| run_transcription(&job_app, &state, &job, &request));
        finish_job(&job_app, &state, &job, EVENT_STT_DONE, result);
    });

    Ok(job_id)
}

struct SttRequest<'a> {
    path: &'a Path,
    model_id: &'a str,
    language: Option<String>,
    translate: bool,
    save_recordings: bool,
}

fn run_transcription(app: &AppHandle, state: &AppState, job_id: &str, request: &SttRequest) -> Result<HistoryItem> {
    let start = std::time::Instant::now();
    let progress = progress_emitter(app, job_id);
    progress(0.0);

    let gpu = state.engine.should_use_gpu(state.gpu_policy());
    let model = stt_model(state, request.model_id, gpu)?;
    if state.jobs.is_cancelled(job_id) {
        return Err(AppError::Cancelled);
    }
    let make_op = |gpu: bool| {
        Op::Transcribe(TranscribeJob {
            model: ModelRef { gpu, ..model.clone() },
            audio_path: request.path.to_path_buf(),
            language: request.language.clone(),
            translate: request.translate,
        })
    };
    let (transcript, duration_ms, device) = match state.engine.run(job_id, &make_op, gpu, &progress)? {
        Event::Transcribed { transcript, audio_ms, device, .. } => (transcript, audio_ms, device),
        Event::Failed { kind, error, .. } => return Err(failure_to_error(kind, error)),
        other => return Err(AppError::Engine(format!("Unexpected reply from the engine: {:?}", other))),
    };

    let paths = &state.paths;
    let full_path = request.path;
    let is_own_recording = is_owned_audio(paths, full_path);

    // Recordings made in-app are discarded after transcription when the user opted out of keeping them.
    let audio_path = if is_own_recording && !request.save_recordings {
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
        model_id: request.model_id.to_string(),
        voice_id: None,
        language: transcript.language,
        device,
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
        state.engine.cancel(&job_id);
        Ok(())
    } else {
        Err(AppError::NotFound(format!("No running job: {}", job_id)))
    }
}
