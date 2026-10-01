use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use chrono::Utc;
use talkr_protocol::{Event, ModelRef, Op, SynthesizeJob, Voice};
use tauri::{command, AppHandle, Manager, State};
use uuid::Uuid;
use crate::catalog::{ModelKind, ModelManifest};
use crate::commands::models::locate_model;
use crate::commands::{catch_panic, ensure_memory_for_job, finish_job, progress_emitter, to_stored_path, EVENT_TTS_DONE};
use crate::db::{insert_history, HistoryItem, HistoryKind};
use crate::engine_host::failure_to_error;
use crate::error::{AppError, Result};
use crate::AppState;

/// Most characters one synthesis request may carry. Far more than anyone pastes on purpose
/// (about two hours of speech), but it keeps a stray multi-megabyte paste from tying up the
/// engine for hours and bloating the history database.
const MAX_SYNTH_CHARS: usize = 100_000;
/// Longest model or voice id accepted from the frontend, in bytes.
const MAX_ID_BYTES: usize = 128;

/// Resolve an installed text-to-speech model: its folder and manifest. Only a manifest that
/// parses counts, so a model torn by an interrupted install reads as "not installed".
fn installed_tts_model(state: &AppState, model_id: &str) -> Result<(PathBuf, ModelManifest)> {
    let (kind, dir, manifest) = locate_model(&state.paths, model_id)
        .and_then(|(kind, dir)| ModelManifest::read(&dir).map(|m| (kind, dir, m)))
        .ok_or_else(|| AppError::NotFound(format!("Model not installed: {}", model_id)))?;
    if kind != ModelKind::Tts {
        return Err(AppError::Validation(format!("{} is not a text-to-speech model", model_id)));
    }
    Ok((dir, manifest))
}

/// Resolve an installed text-to-speech model for the engine.
fn tts_model(state: &AppState, model_id: &str) -> Result<ModelRef> {
    let (dir, _) = installed_tts_model(state, model_id)?;
    ensure_memory_for_job(state, &dir, model_id, 0)?;
    // TTS models are small and fast on the CPU; sherpa-onnx's prebuilt libraries are CPU-only.
    Ok(ModelRef { model_id: model_id.to_string(), dir, threads: state.cpu_threads(), gpu: false })
}

/// Reject an id longer than any real one before it reaches the file system or the engine.
fn check_id_len(what: &str, id: &str) -> Result<()> {
    if id.len() > MAX_ID_BYTES {
        return Err(AppError::Validation(format!("{} is too long (at most {} bytes)", what, MAX_ID_BYTES)));
    }
    Ok(())
}

/// Clean and check the inputs of a synthesis request. Returns the text to speak.
fn validate_synthesis(text: &str, model_id: &str, voice_id: &str) -> Result<String> {
    check_id_len("The model id", model_id)?;
    check_id_len("The voice id", voice_id)?;
    // sherpa-rs turns the text into a C string and panics on a NUL byte (text pasted from a PDF
    // can carry one).
    let text = text.replace('\0', "");
    if text.trim().is_empty() {
        return Err(AppError::Validation("Text is empty".into()));
    }
    let chars = text.chars().count();
    if chars > MAX_SYNTH_CHARS {
        return Err(AppError::Validation(format!(
            "Text is too long: {} characters. Talkr can speak up to 100,000 at a time.",
            format_count(chars)
        )));
    }
    Ok(text)
}

/// `1234567` as "1,234,567".
fn format_count(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

type VoiceCache = Mutex<HashMap<String, (i64, Vec<Voice>)>>;

/// Voices already listed, per model id. Keyed on the manifest's install time too, so a model
/// that is deleted and installed again (possibly a different build) is read afresh.
fn voice_cache() -> &'static VoiceCache {
    static CACHE: OnceLock<VoiceCache> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// List the voices of an installed TTS model.
///
/// Asking the engine loads the whole model, which evicts a loaded Whisper model and waits
/// behind any running transcription. The voice list depends only on the model's files, so it
/// is read from them directly (the same way the engine derives it), and the engine is asked
/// only when the folder is not a layout we recognise. Results are cached per model.
#[command]
pub async fn list_voices(app: AppHandle, model_id: String) -> Result<Vec<Voice>> {
    check_id_len("The model id", &model_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let (dir, manifest) = installed_tts_model(&state, &model_id)?;

        let cached = voice_cache()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&model_id)
            .filter(|(installed_at, _)| *installed_at == manifest.installed_at)
            .map(|(_, voices)| voices.clone());
        if let Some(voices) = cached {
            return Ok(voices);
        }

        let voices = match voices_from_files(&dir, &model_id) {
            Some(voices) => voices,
            None => {
                let model = tts_model(&state, &model_id)?;
                let id = Uuid::new_v4().to_string();
                match state.engine.run(&id, &|_| Op::Voices(model.clone()), false, &|_| {})? {
                    Event::Voices { voices, .. } => voices,
                    Event::Failed { kind, error, .. } => return Err(failure_to_error(kind, error)),
                    other => return Err(AppError::Engine(format!("Unexpected reply from the engine: {:?}", other))),
                }
            }
        };
        voice_cache()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(model_id, (manifest.installed_at, voices.clone()));
        Ok(voices)
    })
    .await?
}

// Voice metadata from the model folder. This mirrors `engine/src/tts_sherpa.rs`: the engine
// picks the speaker by its index in the same list, so the ids must match exactly. The
// `voice_tables_match_the_engine` test keeps the two in step.

/// Size in bytes of one Kokoro speaker embedding in `voices.bin` (510 x 256 f32).
const KOKORO_VOICE_BYTES: u64 = 510 * 256 * 4;

/// Speaker order of `kokoro-en-v0_19` (11 speakers).
const KOKORO_EN_V0_19: &[&str] = &[
    "af", "af_bella", "af_nicole", "af_sarah", "af_sky", "am_adam", "am_michael", "bf_emma",
    "bf_isabella", "bm_george", "bm_lewis",
];

/// Speaker order of `kokoro-multi-lang-v1_0` (53 speakers).
const KOKORO_MULTI_V1_0: &[&str] = &[
    "af_alloy", "af_aoede", "af_bella", "af_heart", "af_jessica", "af_kore", "af_nicole",
    "af_nova", "af_river", "af_sarah", "af_sky", "am_adam", "am_echo", "am_eric", "am_fenrir",
    "am_liam", "am_michael", "am_onyx", "am_puck", "am_santa", "bf_alice", "bf_emma",
    "bf_isabella", "bf_lily", "bm_daniel", "bm_fable", "bm_george", "bm_lewis", "ef_dora",
    "em_alex", "ff_siwis", "hf_alpha", "hf_beta", "hm_omega", "hm_psi", "if_sara", "im_nicola",
    "jf_alpha", "jf_gongitsune", "jf_nezumi", "jf_tebukuro", "jm_kumo", "pf_dora", "pm_alex",
    "pm_santa", "zf_xiaobei", "zf_xiaoni", "zf_xiaoxiao", "zf_xiaoyi", "zm_yunjian", "zm_yunxi",
    "zm_yunxia", "zm_yunyang",
];

/// The voices of the model in `dir`, from its files alone: Kokoro's speaker count comes from
/// the size of `voices.bin`, and a Piper model has one voice named after its `.onnx` file.
/// `None` when the folder is not a layout we recognise (or looks damaged), so the engine can
/// decide and report a proper error.
fn voices_from_files(dir: &Path, model_id: &str) -> Option<Vec<Voice>> {
    let voices_bin = dir.join("voices.bin");
    if voices_bin.exists() {
        let len = std::fs::metadata(&voices_bin).ok()?.len();
        if len < KOKORO_VOICE_BYTES || !len.is_multiple_of(KOKORO_VOICE_BYTES) || !dir.join("model.onnx").is_file() {
            return None;
        }
        return Some(kokoro_voices((len / KOKORO_VOICE_BYTES) as usize));
    }
    let model = find_onnx_model(dir)?;
    Some(vec![piper_voice(&model, model_id)])
}

/// `model.onnx` if present, else the first `.onnx` file by name (as the engine chooses).
fn find_onnx_model(dir: &Path) -> Option<PathBuf> {
    let preferred = dir.join("model.onnx");
    if preferred.is_file() {
        return Some(preferred);
    }
    let mut candidates: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("onnx")))
        .collect();
    candidates.sort();
    candidates.into_iter().next()
}

/// The one voice of a Piper model, named after its file: `en_US-lessac-medium.onnx` is
/// "en_US-lessac-medium" in "en-US".
fn piper_voice(model: &Path, model_id: &str) -> Voice {
    let name = model
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| model_id.to_string());
    let language = name.split('-').next().unwrap_or("").replace('_', "-");
    Voice { id: "0".into(), name, language, gender: None }
}

fn kokoro_voices(num_speakers: usize) -> Vec<Voice> {
    let names: Option<&[&str]> = match num_speakers {
        n if n == KOKORO_EN_V0_19.len() => Some(KOKORO_EN_V0_19),
        n if n == KOKORO_MULTI_V1_0.len() => Some(KOKORO_MULTI_V1_0),
        _ => None,
    };
    (0..num_speakers.max(1))
        .map(|i| match names.and_then(|n| n.get(i)) {
            Some(name) => kokoro_voice(name),
            None => Voice { id: i.to_string(), name: format!("Speaker {}", i), language: String::new(), gender: None },
        })
        .collect()
}

fn kokoro_voice(id: &str) -> Voice {
    let mut chars = id.chars();
    let lang = match chars.next() {
        Some('a') => "en-US",
        Some('b') => "en-GB",
        Some('e') => "es",
        Some('f') => "fr",
        Some('h') => "hi",
        Some('i') => "it",
        Some('j') => "ja",
        Some('p') => "pt-BR",
        Some('z') => "zh",
        _ => "",
    };
    let gender = match chars.next() {
        Some('f') => Some("female".to_string()),
        Some('m') => Some("male".to_string()),
        _ => None,
    };
    let display = id.split_once('_').map(|(_, n)| n).unwrap_or(id);
    let mut name: String = display.to_string();
    if let Some(first) = name.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    Voice { id: id.to_string(), name, language: lang.to_string(), gender }
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
    let text = validate_synthesis(&text, &model_id, &voice_id)?;
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
    let synthesize = || -> Result<HistoryItem> {
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

        let item = HistoryItem {
            id: job_id.to_string(),
            kind: HistoryKind::Tts,
            created_at: Utc::now().timestamp_millis(),
            title: make_title(text),
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
    };
    let result = synthesize();
    if result.is_err() {
        // Cancelled, failed, or not saved to history: nothing will ever point at this file.
        let _ = std::fs::remove_file(&audio_path);
    }
    result
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthesis_text_is_cleaned_and_bounded() {
        assert_eq!(validate_synthesis("hi\0 there", "kokoro", "af").unwrap(), "hi there");
        assert!(matches!(validate_synthesis(" \0\n", "kokoro", "af"), Err(AppError::Validation(_))));

        // The limit counts characters, not bytes: 100 000 multi-byte characters are fine.
        let at_limit = "é".repeat(MAX_SYNTH_CHARS);
        assert!(validate_synthesis(&at_limit, "kokoro", "af").is_ok());
        let over = "a".repeat(MAX_SYNTH_CHARS + 1);
        let Err(AppError::Validation(msg)) = validate_synthesis(&over, "kokoro", "af") else {
            panic!("expected a validation error")
        };
        assert_eq!(msg, "Text is too long: 100,001 characters. Talkr can speak up to 100,000 at a time.");
    }

    #[test]
    fn synthesis_ids_are_bounded() {
        let long = "x".repeat(MAX_ID_BYTES + 1);
        assert!(validate_synthesis("hi", &"x".repeat(MAX_ID_BYTES), "af").is_ok());
        assert!(matches!(validate_synthesis("hi", &long, "af"), Err(AppError::Validation(_))));
        assert!(matches!(validate_synthesis("hi", "kokoro", &long), Err(AppError::Validation(_))));
        // Bytes, not characters: 43 three-byte characters are 129 bytes.
        assert!(matches!(validate_synthesis("hi", "kokoro", &"€".repeat(43)), Err(AppError::Validation(_))));
    }

    #[test]
    fn counts_are_grouped() {
        assert_eq!(format_count(0), "0");
        assert_eq!(format_count(999), "999");
        assert_eq!(format_count(1000), "1,000");
        assert_eq!(format_count(100_001), "100,001");
        assert_eq!(format_count(1_234_567), "1,234,567");
    }

    fn kokoro_dir(speakers: u64) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("model.onnx"), b"onnx").unwrap();
        // Sized without writing the bytes: only the length matters.
        let f = std::fs::File::create(dir.path().join("voices.bin")).unwrap();
        f.set_len(speakers * KOKORO_VOICE_BYTES).unwrap();
        dir
    }

    #[test]
    fn kokoro_voices_come_from_the_voices_file_size() {
        let dir = kokoro_dir(11);
        let voices = voices_from_files(dir.path(), "kokoro-en-v0_19").unwrap();
        assert_eq!(voices.len(), 11);
        assert_eq!(voices[1].id, "af_bella");
        assert_eq!(voices[1].name, "Bella");
        assert_eq!(voices[1].language, "en-US");
        assert_eq!(voices[1].gender.as_deref(), Some("female"));

        let dir = kokoro_dir(53);
        let voices = voices_from_files(dir.path(), "kokoro-multi-lang-v1_0").unwrap();
        assert_eq!(voices.len(), 53);
        assert_eq!(voices[3].id, "af_heart");
        assert_eq!(voices[52].id, "zm_yunyang");

        // An unknown speaker count still lists one voice per speaker, by number.
        let dir = kokoro_dir(2);
        let ids: Vec<String> = voices_from_files(dir.path(), "k").unwrap().into_iter().map(|v| v.id).collect();
        assert_eq!(ids, ["0", "1"]);
    }

    #[test]
    fn damaged_kokoro_folder_is_left_to_the_engine() {
        let dir = kokoro_dir(11);
        let f = std::fs::OpenOptions::new().write(true).open(dir.path().join("voices.bin")).unwrap();
        f.set_len(11 * KOKORO_VOICE_BYTES - 1).unwrap();
        assert!(voices_from_files(dir.path(), "k").is_none());

        let dir = kokoro_dir(11);
        std::fs::remove_file(dir.path().join("model.onnx")).unwrap();
        assert!(voices_from_files(dir.path(), "k").is_none());
    }

    #[test]
    fn piper_voice_is_named_after_its_model_file() {
        let dir = tempfile::tempdir().unwrap();
        assert!(voices_from_files(dir.path(), "p").is_none(), "no model at all");
        std::fs::write(dir.path().join("tokens.txt"), b"a 1").unwrap();
        std::fs::write(dir.path().join("en_US-lessac-medium.onnx"), b"onnx").unwrap();
        std::fs::write(dir.path().join("zz-later.onnx"), b"onnx").unwrap();
        let voices = voices_from_files(dir.path(), "p").unwrap();
        assert_eq!(voices.len(), 1);
        assert_eq!(voices[0].id, "0");
        assert_eq!(voices[0].name, "en_US-lessac-medium");
        assert_eq!(voices[0].language, "en-US");
    }

    /// The quoted names of `const <name>: &[&str] = &[ ... ];` in Rust source.
    fn const_list(source: &str, name: &str) -> Vec<String> {
        let start = source.find(&format!("const {}: &[&str] = &[", name)).unwrap_or_else(|| panic!("{name} not found"));
        let body = &source[start..];
        let body = &body[body.find("&[").unwrap() + 2..body.find("];").unwrap()];
        body.split('"').skip(1).step_by(2).map(str::to_string).collect()
    }

    #[test]
    fn voice_tables_match_the_engine() {
        // The engine chooses the speaker by its position in these lists, so a voice id listed
        // here but ordered differently there would speak with the wrong voice.
        let engine = include_str!("../../engine/src/tts_sherpa.rs");
        assert_eq!(const_list(engine, "KOKORO_EN_V0_19"), KOKORO_EN_V0_19);
        assert_eq!(const_list(engine, "KOKORO_MULTI_V1_0"), KOKORO_MULTI_V1_0);
        assert!(engine.contains("const KOKORO_VOICE_BYTES: u64 = 510 * 256 * 4;"));
    }
}
