use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use talkr_protocol::{Decoding, Transcript, TranscriptSegment};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};
use crate::{devices, logger, EngineError, JobControl, Result};

/// How to run one transcription.
#[derive(Debug, Clone, Copy)]
pub struct TranscribeOptions<'a> {
    pub language: Option<&'a str>,
    pub translate: bool,
    pub threads: usize,
    pub decoding: Decoding,
}

/// Beam search uses whisper.cpp's CLI default of 5 beams.
fn sampling(decoding: Decoding) -> SamplingStrategy {
    match decoding {
        Decoding::Greedy => SamplingStrategy::Greedy { best_of: 1 },
        Decoding::Beam => SamplingStrategy::BeamSearch { beam_size: 5, patience: -1.0 },
    }
}

pub struct WhisperEngine {
    ctx: WhisperContext,
    model_id: String,
    gpu: bool,
    /// What the model runs on, for history ("cpu", or the GPU's name).
    device: String,
}

impl WhisperEngine {
    /// Load a whisper.cpp GGML model. `model_path` may be the `.bin` file itself or the model
    /// directory, in which case the first `*.bin` file inside it is used. With `gpu`, the model
    /// goes to the best GPU that can hold it, if there is one.
    pub fn new(model_path: &Path, model_id: String, gpu: bool) -> Result<Self> {
        let file = if model_path.is_dir() {
            find_ggml_file(model_path)?
        } else {
            model_path.to_path_buf()
        };
        let model_bytes = std::fs::metadata(&file)
            .map_err(|e| EngineError::Invalid(format!("Cannot read the Whisper model {}: {}", file.display(), e)))?
            .len();

        let mut params = WhisperContextParameters::default();
        // Flash attention cuts the compute buffers substantially on CPU and GPU (whisper.cpp's
        // own default; whisper-rs turns it off).
        params.flash_attn(true);
        let mut device = "cpu".to_string();
        match gpu.then(devices::list).as_deref().and_then(|d| devices::choose_gpu(d, model_bytes)) {
            Some((index, dev)) => {
                params.use_gpu(true).gpu_device(index);
                device = dev.description.clone();
            }
            None => {
                params.use_gpu(false);
            }
        }

        logger::take_last_error();
        let ctx = WhisperContext::new_with_params(&file, params).map_err(|e| {
            let detail = logger::take_last_error().unwrap_or_else(|| e.to_string());
            if logger::is_allocation_failure(&detail) {
                EngineError::OutOfMemory(format!("Not enough memory to load {}: {}", model_id, detail))
            } else {
                EngineError::Invalid(format!("Could not load the Whisper model {}: {}", model_id, detail))
            }
        })?;
        log::info!("loaded {} on {}", model_id, device);
        Ok(Self { ctx, model_id, gpu, device })
    }

    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    /// Whether this engine was loaded for a GPU request (the loaded model may still be on the CPU
    /// when no GPU could take it).
    pub fn gpu_requested(&self) -> bool {
        self.gpu
    }

    pub fn device(&self) -> &str {
        &self.device
    }

    /// Transcribe 16 kHz mono samples.
    pub fn transcribe(&self, samples: &[f32], options: &TranscribeOptions, ctl: &JobControl) -> Result<Transcript> {
        let TranscribeOptions { language, translate, threads, decoding } = *options;
        logger::take_last_error();
        let mut state = self.ctx.create_state().map_err(|e| self.native_error("prepare", e))?;

        let mut params = FullParams::new(sampling(decoding));
        params.set_language(Some(sanitize_language(language)));
        params.set_translate(translate);
        params.set_n_threads(threads.max(1) as i32);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        let progress_ctl = ctl.clone();
        params.set_progress_callback_safe(move |progress: i32| {
            progress_ctl.progress(progress as f32 / 100.0);
        });
        // Not `set_abort_callback_safe`: in whisper-rs 0.16 it stores a `Box<Box<dyn FnMut>>` but
        // its trampoline reads the pointer as the closure itself, so every poll read garbage.
        // That showed up as a bogus "failed to encode" (-6) or an access violation that closed
        // the app mid-transcription. Point the C callback straight at the job's cancel flag.
        // ggml calls it on every graph computation, so it also tells the worker's heartbeat that
        // a long transcription is still working between whisper's 5 % progress reports.
        let abort = AbortData { cancel: ctl.cancel_flag(), alive: ctl.alive() };
        // SAFETY: `abort` outlives `state.full` below, and the callback only touches its atomics.
        unsafe {
            params.set_abort_callback(Some(abort_if_cancelled));
            params.set_abort_callback_user_data(&abort as *const AbortData as *mut c_void);
        }

        let result = state.full(params, samples);
        ctl.check_cancelled()?;
        result.map_err(|e| self.native_error("transcribe", e))?;

        let mut segments = Vec::new();
        let mut text = String::new();
        for i in 0..state.full_n_segments() {
            let segment = state
                .get_segment(i)
                .ok_or_else(|| EngineError::Engine(format!("Missing segment {}", i)))?;
            let piece = segment.to_str_lossy().map_err(|e| EngineError::Engine(e.to_string()))?;
            let piece = piece.trim();
            if piece.is_empty() {
                continue;
            }
            // whisper.cpp timestamps are in centiseconds.
            segments.push(TranscriptSegment {
                start_ms: segment.start_timestamp() * 10,
                end_ms: segment.end_timestamp() * 10,
                text: piece.to_string(),
            });
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(piece);
        }

        let language = whisper_rs::get_lang_str(state.full_lang_id_from_state()).map(|s| s.to_string());
        ctl.progress(1.0);
        Ok(Transcript { text, segments, language })
    }

    fn native_error(&self, stage: &str, e: whisper_rs::WhisperError) -> EngineError {
        let detail = logger::take_last_error().unwrap_or_else(|| e.to_string());
        if logger::is_allocation_failure(&detail) {
            EngineError::OutOfMemory(format!("Not enough memory to run {}: {}", self.model_id, detail))
        } else {
            EngineError::Engine(format!("Whisper could not {}: {}", stage, detail))
        }
    }
}

/// What whisper.cpp's abort callback reads: the job's cancel flag and its liveness counter.
struct AbortData {
    cancel: Arc<AtomicBool>,
    alive: Arc<AtomicU64>,
}

unsafe extern "C" fn abort_if_cancelled(user_data: *mut c_void) -> bool {
    let data = &*(user_data as *const AbortData);
    data.alive.fetch_add(1, Ordering::Relaxed);
    data.cancel.load(Ordering::Relaxed)
}

/// whisper-rs panics on a NUL byte and whisper.cpp rejects unknown codes, so anything that is not
/// a plain language code means auto-detect.
pub fn sanitize_language(language: Option<&str>) -> &str {
    match language {
        Some(l) if (2..=3).contains(&l.len()) && l.bytes().all(|b| b.is_ascii_lowercase()) => l,
        _ => "auto",
    }
}

fn find_ggml_file(dir: &Path) -> Result<PathBuf> {
    let mut candidates: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("bin")))
        .collect();
    candidates.sort();
    candidates
        .into_iter()
        .next()
        .ok_or_else(|| EngineError::Invalid(format!("No Whisper model (.bin) found in {}", dir.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_codes_are_sanitized() {
        assert_eq!(sanitize_language(Some("en")), "en");
        assert_eq!(sanitize_language(Some("haw")), "haw");
        assert_eq!(sanitize_language(None), "auto");
        assert_eq!(sanitize_language(Some("")), "auto");
        assert_eq!(sanitize_language(Some("auto")), "auto");
        assert_eq!(sanitize_language(Some("EN")), "auto");
        assert_eq!(sanitize_language(Some("e\0")), "auto");
        assert_eq!(sanitize_language(Some("english")), "auto");
    }

    #[test]
    fn a_folder_without_a_model_is_invalid_input() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("notes.txt"), "x").unwrap();
        let err = WhisperEngine::new(dir.path(), "x".into(), false).err().unwrap();
        assert!(matches!(err, EngineError::Invalid(_)), "{err:?}");
    }

    #[test]
    fn a_corrupt_model_file_is_an_error_not_a_crash() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("ggml-broken.bin"), b"definitely not a ggml model").unwrap();
        let err = WhisperEngine::new(dir.path(), "broken".into(), false).err().unwrap();
        assert!(matches!(err, EngineError::Invalid(_) | EngineError::OutOfMemory(_)), "{err:?}");
    }
}
