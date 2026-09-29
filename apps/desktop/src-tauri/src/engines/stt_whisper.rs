use std::path::{Path, PathBuf};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};
use crate::engines::{AppError, JobControl, Result, SttEngine, SttOptions, Transcript, TranscriptSegment};

pub struct WhisperEngine {
    ctx: WhisperContext,
    model_id: String,
}

impl WhisperEngine {
    /// Load a whisper.cpp GGML model. `model_path` may be the `.bin` file itself or the
    /// model directory, in which case the first `*.bin` file inside it is used.
    pub fn new(model_path: &Path, model_id: String) -> Result<Self> {
        let file = if model_path.is_dir() {
            find_ggml_file(model_path)?
        } else {
            model_path.to_path_buf()
        };
        let path_str = file
            .to_str()
            .ok_or_else(|| AppError::Path(format!("Non UTF-8 model path: {}", file.display())))?;
        let params = WhisperContextParameters::default();
        let ctx = WhisperContext::new_with_params(path_str, params)
            .map_err(|e| AppError::Engine(format!("Failed to load Whisper model: {}", e)))?;
        Ok(Self { ctx, model_id })
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
        .ok_or_else(|| AppError::Model(format!("No GGML .bin model found in {}", dir.display())))
}

impl SttEngine for WhisperEngine {
    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn transcribe(&self, samples: &[f32], opts: &SttOptions, ctl: &JobControl) -> Result<Transcript> {
        let mut state = self.ctx.create_state().map_err(|e| AppError::Engine(e.to_string()))?;

        let language = match opts.language.as_deref() {
            None | Some("") => "auto",
            Some(l) => l,
        };

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some(language));
        params.set_translate(opts.translate);
        params.set_n_threads(opts.threads.max(1) as i32);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        let progress_ctl = ctl.clone();
        params.set_progress_callback_safe(move |progress: i32| {
            progress_ctl.progress(progress as f32 / 100.0);
        });
        let abort_ctl = ctl.clone();
        params.set_abort_callback_safe(move || abort_ctl.is_cancelled());

        let result = state.full(params, samples);
        if ctl.is_cancelled() {
            return Err(AppError::Cancelled);
        }
        result.map_err(|e| AppError::Engine(e.to_string()))?;

        let num_segments = state.full_n_segments();
        let mut segments = Vec::with_capacity(num_segments.max(0) as usize);
        let mut full_text = String::new();

        for i in 0..num_segments {
            let segment = state
                .get_segment(i)
                .ok_or_else(|| AppError::Engine(format!("Missing segment {}", i)))?;
            let text = segment
                .to_str_lossy()
                .map_err(|e| AppError::Engine(e.to_string()))?
                .trim()
                .to_string();
            if text.is_empty() {
                continue;
            }

            // whisper.cpp timestamps are in centiseconds.
            segments.push(TranscriptSegment {
                start_ms: segment.start_timestamp() * 10,
                end_ms: segment.end_timestamp() * 10,
                text: text.clone(),
            });

            if !full_text.is_empty() {
                full_text.push(' ');
            }
            full_text.push_str(&text);
        }

        let language = whisper_rs::get_lang_str(state.full_lang_id_from_state()).map(|s| s.to_string());

        ctl.progress(1.0);

        Ok(Transcript {
            text: full_text,
            segments,
            language,
        })
    }
}
