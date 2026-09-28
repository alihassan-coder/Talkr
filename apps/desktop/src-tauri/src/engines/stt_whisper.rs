use std::path::Path;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};
use crate::engines::{SttEngine, SttOptions, Transcript, TranscriptSegment, Result, AppError};

pub struct WhisperEngine {
    ctx: WhisperContext,
    model_id: String,
}

impl WhisperEngine {
    pub fn new(model_path: &Path, model_id: String) -> Result<Self> {
        let params = WhisperContextParameters::default();
        let ctx = WhisperContext::new_with_params(model_path.to_str().unwrap(), params)
            .map_err(|e| AppError::Engine(format!("Failed to load Whisper model: {}", e)))?;
        Ok(Self { ctx, model_id })
    }
}

impl SttEngine for WhisperEngine {
    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn transcribe(&self, samples: &[f32], opts: &SttOptions, on_progress: &dyn Fn(f32)) -> Result<Transcript> {
        let mut state = self.ctx.create_state().map_err(|e| AppError::Engine(e.to_string()))?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(opts.language.as_deref().unwrap_or("auto"));
        params.set_translate(opts.translate);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(true);

        params.set_progress_callback_safe(|progress| {
            on_progress(progress as f32 / 100.0);
        });

        state.full(params, samples).map_err(|e| AppError::Engine(e.to_string()))?;

        let num_segments = state.full_n_segments().map_err(|e| AppError::Engine(e.to_string()))?;

        let mut segments = Vec::new();
        let mut full_text = String::new();

        for i in 0..num_segments {
            let text = state.full_get_segment_text(i).map_err(|e| AppError::Engine(e.to_string()))?;
            let start_ts = state.full_get_segment_t0(i).map_err(|e| AppError::Engine(e.to_string()))?;
            let end_ts = state.full_get_segment_t1(i).map_err(|e| AppError::Engine(e.to_string()))?;

            segments.push(TranscriptSegment {
                start_ms: start_ts as i64 * 10,
                end_ms: end_ts as i64 * 10,
                text: text.clone(),
            });

            if !full_text.is_empty() {
                full_text.push(' ');
            }
            full_text.push_str(&text);
        }

        let language = state.full_lang_id().ok().and_then(|id| {
            whisper_rs::language_code_to_name(id).map(|s| s.to_string())
        });

        Ok(Transcript {
            text: full_text,
            segments,
            language,
        })
    }
}