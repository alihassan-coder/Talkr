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

        // SAFETY: whisper-rs requires a 'static callback, but `params` (which owns the
        // closure) is consumed and dropped inside `state.full` before this function returns.
        let on_progress: &'static dyn Fn(f32) = unsafe { std::mem::transmute(on_progress) };
        params.set_progress_callback_safe(move |progress: i32| {
            on_progress(progress as f32 / 100.0);
        });

        state.full(params, samples).map_err(|e| AppError::Engine(e.to_string()))?;

        let num_segments = state.full_n_segments();

        let mut segments = Vec::new();
        let mut full_text = String::new();

        for i in 0..num_segments {
            let segment = state.get_segment(i)
                .ok_or_else(|| AppError::Engine(format!("Missing segment {}", i)))?;
            let text = segment.to_str_lossy().map_err(|e| AppError::Engine(e.to_string()))?.into_owned();
            let start_ts = segment.start_timestamp();
            let end_ts = segment.end_timestamp();

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

        let language = whisper_rs::get_lang_str_full(state.full_lang_id_from_state())
            .map(|s| s.to_string());

        Ok(Transcript {
            text: full_text,
            segments,
            language,
        })
    }
}