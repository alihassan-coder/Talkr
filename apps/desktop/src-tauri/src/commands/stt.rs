use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::TryLockError;
use std::time::Duration;
use chrono::Utc;
use serde::Serialize;
use talkr_protocol::{Decoding, Device, DeviceKind, Event, ModelRef, Op, TranscribeJob};
use tauri::{command, AppHandle, Emitter, Manager, State};
use uuid::Uuid;
use crate::catalog::ModelKind;
use crate::config::SttQuality;
use crate::commands::models::locate_model;
use crate::commands::tts::make_title;
use crate::commands::{
    catch_panic, ensure_memory_for_job, finish_job, is_owned_audio, progress_emitter, resolve_path, to_stored_path,
    MicErrorEvent, EVENT_MIC_ERROR, EVENT_MIC_LEVEL, EVENT_STT_DONE,
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
/// `audio_bytes` is the memory the decoded audio will take in the engine.
pub(crate) fn stt_model(state: &AppState, model_id: &str, gpu: bool, audio_bytes: u64) -> Result<ModelRef> {
    let (kind, dir) = locate_model(&state.paths, model_id)
        .filter(|(_, dir)| dir.join("manifest.json").is_file())
        .ok_or_else(|| AppError::NotFound(format!("Model not installed: {}", model_id)))?;
    if kind != ModelKind::Stt {
        return Err(AppError::Validation(format!("{} is not a speech-to-text model", model_id)));
    }
    check_cpu_support()?;
    // On a discrete GPU the weights live in video memory, and the engine checks that fits itself.
    // Integrated GPUs and Apple Silicon share system memory, so those are checked like the CPU.
    let unified_memory = cfg!(target_os = "macos");
    if !gpu || unified_memory || weights_in_system_memory(gpu, unified_memory, &state.engine.devices()) {
        ensure_memory_for_job(state, &dir, model_id, audio_bytes)?;
    }
    Ok(ModelRef { model_id: model_id.to_string(), dir, threads: state.cpu_threads(), gpu })
}

/// Whether a job's model weights will sit in system memory: on the CPU, on a GPU that shares system
/// memory, or when there is no discrete GPU to put them on (the engine prefers a discrete one).
fn weights_in_system_memory(gpu: bool, unified_memory: bool, devices: &[Device]) -> bool {
    !gpu || unified_memory || !devices.iter().any(|d| d.kind == DeviceKind::Gpu)
}

/// Rough memory the engine needs to hold `path` decoded: 16 kHz mono f32, 64 KB per second.
/// Exact for WAV (the header says how long it is); for compressed files the length is guessed
/// from the size at 64 kbps, typical of speech recordings. Reading a header is cheap; decoding is not.
fn decoded_audio_bytes(path: &Path) -> u64 {
    const DECODED_BYTES_PER_SECOND: u64 = 16_000 * 4;
    if let Ok(reader) = hound::WavReader::open(path) {
        let rate = reader.spec().sample_rate as u64;
        if let Some(bytes) = (reader.duration() as u64 * DECODED_BYTES_PER_SECOND).checked_div(rate) {
            return bytes;
        }
    }
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let seconds = size * 8 / 64_000;
    seconds * DECODED_BYTES_PER_SECOND
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

/// Start recording from the default microphone into a new WAV file under `~/.talkr/audio`.
/// Emits `mic://level` (RMS, 0..1) ~20x/s, and `mic://error` ({ message }) once if the microphone
/// fails or the file cannot be written; what was captured is kept until `stop_recording`.
#[command]
pub async fn start_recording(state: State<'_, AppState>, app: AppHandle) -> Result<()> {
    let audio_path = state.paths.audio_path("wav");
    if let Some(parent) = audio_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Opening the device can take seconds (or time out) with some drivers; keep it off the
    // async runtime.
    let recorder_app = app.clone();
    let signals = tauri::async_runtime::spawn_blocking(move || -> Result<_> {
        let state = recorder_app.state::<AppState>();
        let mut recorder = state.recorder();
        recorder.start(&audio_path)?;
        Ok(recorder.signals())
    })
    .await??;

    tauri::async_runtime::spawn(async move {
        let mut error_sent = false;
        while signals.recording.load(Ordering::Relaxed) {
            let _ = app.emit(EVENT_MIC_LEVEL, signals.level());
            if !error_sent {
                if let Some(message) = signals.error() {
                    error_sent = true;
                    let _ = app.emit(EVENT_MIC_ERROR, MicErrorEvent { message });
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let _ = app.emit(EVENT_MIC_LEVEL, 0.0f32);
    });

    Ok(())
}

/// Stop recording and finalize its WAV file (16 kHz mono) under `~/.talkr/audio`.
#[command]
pub async fn stop_recording(state: State<'_, AppState>, app: AppHandle) -> Result<RecordingResult> {
    let recorded = tauri::async_runtime::spawn_blocking(move || -> Result<_> {
        let state = app.state::<AppState>();
        // Only asking to stop needs the lock; waiting for the device does not, so nothing else
        // queues behind a slow driver.
        let pending = state.recorder().request_stop()?;
        pending.wait()
    })
    .await??;
    if recorded.limit_reached {
        log::warn!("recording hit the length limit; keeping what was captured");
    }
    if let Some(error) = &recorded.error {
        log::warn!("recording ended with an error ({}); keeping what was captured", error);
    }

    if recorded.samples == 0 {
        let _ = std::fs::remove_file(&recorded.path);
        return Err(AppError::Audio(recorded.error.unwrap_or_else(|| "No audio was captured".into())));
    }

    Ok(RecordingResult {
        temp_audio_path: to_stored_path(&state.paths, &recorded.path),
        duration_ms: recorded.duration_ms(),
    })
}

/// Current microphone RMS level (0 when not recording). Never waits for the recorder: while it is
/// busy starting or stopping there is no level to show anyway.
#[command]
pub async fn get_input_level(state: State<'_, AppState>) -> Result<f32> {
    Ok(match state.recorder.try_lock() {
        Ok(recorder) => recorder.level(),
        Err(TryLockError::Poisoned(e)) => e.into_inner().level(),
        Err(TryLockError::WouldBlock) => 0.0,
    })
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

    let (language, quality, save_recordings) = {
        let settings = state.settings();
        (language.unwrap_or_else(|| settings.stt_language.clone()), settings.stt_quality, settings.save_recordings)
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
            quality,
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
    quality: SttQuality,
    save_recordings: bool,
}

/// How whisper should search: beam search for `Accurate`, and under `Auto` for imported files;
/// greedy for `Fast`, and under `Auto` for in-app recordings, where the user is waiting.
fn decoding_for(quality: SttQuality, own_recording: bool) -> Decoding {
    match quality {
        SttQuality::Accurate => Decoding::Beam,
        SttQuality::Fast => Decoding::Greedy,
        SttQuality::Auto if own_recording => Decoding::Greedy,
        SttQuality::Auto => Decoding::Beam,
    }
}

fn run_transcription(app: &AppHandle, state: &AppState, job_id: &str, request: &SttRequest) -> Result<HistoryItem> {
    let start = std::time::Instant::now();
    let progress = progress_emitter(app, job_id);
    progress(0.0);

    let gpu = state.engine.should_use_gpu(state.gpu_policy());
    let model = stt_model(state, request.model_id, gpu, decoded_audio_bytes(request.path))?;
    if state.jobs.is_cancelled(job_id) {
        return Err(AppError::Cancelled);
    }
    let paths = &state.paths;
    let full_path = request.path;
    let is_own_recording = is_owned_audio(paths, full_path);
    let decoding = decoding_for(request.quality, is_own_recording);
    let make_op = |gpu: bool| {
        Op::Transcribe(TranscribeJob {
            model: ModelRef { gpu, ..model.clone() },
            audio_path: request.path.to_path_buf(),
            language: request.language.clone(),
            translate: request.translate,
            decoding,
        })
    };
    let (transcript, duration_ms, device) = match state.engine.run(job_id, &make_op, gpu, &progress)? {
        Event::Transcribed { transcript, audio_ms, device, .. } => (transcript, audio_ms, device),
        Event::Failed { kind, error, .. } => return Err(failure_to_error(kind, error)),
        other => return Err(AppError::Engine(format!("Unexpected reply from the engine: {:?}", other))),
    };

    // Recordings made in-app are discarded after transcription when the user opted out of keeping
    // them, but only once the transcript is safely in history (below).
    let discard_recording = is_own_recording && !request.save_recordings;
    let audio_path = (!discard_recording).then(|| to_stored_path(paths, full_path));

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

    // If saving fails the recording stays on disk, so the user can transcribe it again.
    insert_history(paths, &item)?;
    if discard_recording {
        if let Err(e) = std::fs::remove_file(full_path) {
            log::warn!("Failed to delete recording {}: {}", full_path.display(), e);
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn device(kind: DeviceKind) -> Device {
        Device { kind, name: "x".into(), description: String::new(), memory_free: 0, memory_total: 0 }
    }

    #[test]
    fn only_a_discrete_gpu_moves_weights_out_of_system_memory() {
        let discrete = [device(DeviceKind::Cpu), device(DeviceKind::Gpu)];
        let integrated = [device(DeviceKind::Cpu), device(DeviceKind::Igpu)];
        assert!(weights_in_system_memory(false, false, &discrete));
        assert!(!weights_in_system_memory(true, false, &discrete));
        assert!(weights_in_system_memory(true, false, &integrated));
        assert!(weights_in_system_memory(true, false, &[]));
        // Apple Silicon: the GPU shares system memory.
        assert!(weights_in_system_memory(true, true, &discrete));
    }

    #[test]
    fn auto_quality_is_accurate_for_files_and_fast_for_recordings() {
        assert_eq!(decoding_for(SttQuality::Auto, false), Decoding::Beam);
        assert_eq!(decoding_for(SttQuality::Auto, true), Decoding::Greedy);
        for own in [false, true] {
            assert_eq!(decoding_for(SttQuality::Accurate, own), Decoding::Beam);
            assert_eq!(decoding_for(SttQuality::Fast, own), Decoding::Greedy);
        }
    }

    #[test]
    fn decoded_audio_size_is_estimated_cheaply() {
        let dir = tempfile::tempdir().unwrap();
        // Two seconds of 48 kHz stereo WAV decodes to two seconds of 16 kHz mono f32.
        let wav = dir.path().join("a.wav");
        let spec = hound::WavSpec { channels: 2, sample_rate: 48_000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(&wav, spec).unwrap();
        for _ in 0..(2 * 48_000 * 2) {
            w.write_sample(0i16).unwrap();
        }
        w.finalize().unwrap();
        assert_eq!(decoded_audio_bytes(&wav), 2 * 64_000);

        // A compressed file is guessed from its size: 80 KB at 64 kbps is 10 seconds.
        let mp3 = dir.path().join("a.mp3");
        std::fs::write(&mp3, vec![0u8; 80_000]).unwrap();
        assert_eq!(decoded_audio_bytes(&mp3), 10 * 64_000);
        assert_eq!(decoded_audio_bytes(&dir.path().join("missing.mp3")), 0);
    }
}
