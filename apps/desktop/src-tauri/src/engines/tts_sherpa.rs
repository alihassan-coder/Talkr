use std::path::{Path, PathBuf};
use std::sync::Mutex;
use sherpa_rs::tts::{CommonTtsConfig, KokoroTts, KokoroTtsConfig, TtsAudio, VitsTts, VitsTtsConfig};
use sherpa_rs::OnnxConfig;
use crate::engines::{AppError, AudioBuffer, JobControl, Result, TtsEngine, TtsOptions, Voice};

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

enum Backend {
    Kokoro(KokoroTts),
    Vits(VitsTts),
}

impl Backend {
    fn create(&mut self, text: &str, sid: i32, speed: f32) -> std::result::Result<TtsAudio, String> {
        // sherpa-rs returns `eyre::Result`; stringify so we don't depend on eyre directly.
        match self {
            Backend::Kokoro(t) => t.create(text, sid, speed).map_err(|e| e.to_string()),
            Backend::Vits(t) => t.create(text, sid, speed).map_err(|e| e.to_string()),
        }
    }
}

pub struct SherpaTtsEngine {
    tts: Mutex<Backend>,
    model_id: String,
    voices: Vec<Voice>,
}

impl SherpaTtsEngine {
    /// Load a sherpa-onnx TTS model directory.
    ///
    /// Kokoro layout: `model.onnx`, `voices.bin`, `tokens.txt`, `espeak-ng-data/`
    /// (+ optional `lexicon*.txt`, `dict/`, `*.fst` for multi-lang).
    /// Piper/VITS layout: `<name>.onnx`, `tokens.txt`, `espeak-ng-data/`.
    pub fn new(model_dir: &Path, model_id: String, num_threads: usize) -> Result<Self> {
        let model = find_onnx_model(model_dir)?;
        let tokens = require(model_dir.join("tokens.txt"))?;
        let data_dir = model_dir.join("espeak-ng-data");
        let data_dir = if data_dir.is_dir() { path_str(&data_dir)? } else { String::new() };
        let voices_bin = model_dir.join("voices.bin");

        let onnx_config = OnnxConfig {
            provider: "cpu".into(),
            debug: false,
            num_threads: num_threads.max(1) as i32,
        };

        let (backend, voices) = if voices_bin.is_file() {
            let lexicon = list_files(model_dir, |name| name.starts_with("lexicon") && name.ends_with(".txt"))?;
            let rule_fsts = list_files(model_dir, |name| name.ends_with(".fst"))?;
            let dict_dir = model_dir.join("dict");
            let config = KokoroTtsConfig {
                model: path_str(&model)?,
                voices: path_str(&voices_bin)?,
                tokens: path_str(&tokens)?,
                data_dir,
                dict_dir: if dict_dir.is_dir() { path_str(&dict_dir)? } else { String::new() },
                lexicon,
                length_scale: 1.0,
                onnx_config,
                common_config: CommonTtsConfig {
                    rule_fsts,
                    max_num_sentences: 1,
                    silence_scale: 0.2,
                    ..Default::default()
                },
                lang: String::new(),
            };
            let num_speakers = (std::fs::metadata(&voices_bin)?.len() / KOKORO_VOICE_BYTES) as usize;
            (Backend::Kokoro(KokoroTts::new(config)), kokoro_voices(num_speakers))
        } else {
            if data_dir.is_empty() {
                return Err(AppError::Model(format!(
                    "Missing espeak-ng-data directory in {}",
                    model_dir.display()
                )));
            }
            let config = VitsTtsConfig {
                model: path_str(&model)?,
                tokens: path_str(&tokens)?,
                data_dir,
                length_scale: 1.0,
                noise_scale: 0.667,
                noise_scale_w: 0.8,
                silence_scale: 0.2,
                onnx_config,
                tts_config: CommonTtsConfig {
                    max_num_sentences: 1,
                    silence_scale: 0.2,
                    ..Default::default()
                },
                ..Default::default()
            };
            let name = model
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| model_id.clone());
            let language = name.split('-').next().unwrap_or("").replace('_', "-");
            let voice = Voice {
                id: "0".into(),
                name,
                language,
                gender: None,
            };
            (Backend::Vits(VitsTts::new(config)), vec![voice])
        };

        Ok(Self {
            tts: Mutex::new(backend),
            model_id,
            voices,
        })
    }

    fn speaker_id(&self, voice_id: &str) -> i32 {
        if let Some(idx) = self.voices.iter().position(|v| v.id == voice_id) {
            return idx as i32;
        }
        voice_id.parse::<i32>().ok().filter(|&i| i >= 0).unwrap_or(0)
    }
}

impl TtsEngine for SherpaTtsEngine {
    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn voices(&self) -> Vec<Voice> {
        self.voices.clone()
    }

    fn synthesize(&self, text: &str, opts: &TtsOptions, ctl: &JobControl) -> Result<AudioBuffer> {
        let chunks = split_sentences(text);
        if chunks.is_empty() {
            return Err(AppError::Validation("Text is empty".into()));
        }

        let sid = self.speaker_id(&opts.voice_id);
        let speed = if opts.speed.is_finite() && opts.speed > 0.0 { opts.speed } else { 1.0 };
        let total = chunks.len() as f32;

        let mut all_samples = Vec::new();
        let mut sample_rate = 0u32;
        let mut tts = self.tts.lock().map_err(|_| AppError::Engine("TTS engine poisoned".into()))?;

        for (i, chunk) in chunks.iter().enumerate() {
            if ctl.is_cancelled() {
                return Err(AppError::Cancelled);
            }
            ctl.progress(i as f32 / total);

            let audio = tts
                .create(chunk, sid, speed)
                .map_err(AppError::Engine)?;
            sample_rate = audio.sample_rate;
            all_samples.extend_from_slice(&audio.samples);
        }

        ctl.progress(1.0);

        Ok(AudioBuffer {
            samples: all_samples,
            sample_rate,
        })
    }
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
            None => Voice {
                id: i.to_string(),
                name: format!("Speaker {}", i),
                language: String::new(),
                gender: None,
            },
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
    Voice {
        id: id.to_string(),
        name,
        language: lang.to_string(),
        gender,
    }
}

/// Split text into sentence-sized chunks, keeping terminal punctuation for natural prosody.
fn split_sentences(text: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for c in text.chars() {
        current.push(c);
        if matches!(c, '.' | '!' | '?' | '\n' | '。' | '！' | '？') {
            let trimmed = current.trim();
            if trimmed.chars().any(|c| c.is_alphanumeric()) {
                chunks.push(trimmed.to_string());
            }
            current.clear();
        }
    }
    let trimmed = current.trim();
    if trimmed.chars().any(|c| c.is_alphanumeric()) {
        chunks.push(trimmed.to_string());
    }
    chunks
}

fn find_onnx_model(dir: &Path) -> Result<PathBuf> {
    let preferred = dir.join("model.onnx");
    if preferred.is_file() {
        return Ok(preferred);
    }
    let mut candidates: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("onnx")))
        .collect();
    candidates.sort();
    candidates
        .into_iter()
        .next()
        .ok_or_else(|| AppError::Model(format!("No .onnx model found in {}", dir.display())))
}

fn list_files(dir: &Path, pred: impl Fn(&str) -> bool) -> Result<String> {
    let mut files: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        let matches = path.is_file() && path.file_name().and_then(|n| n.to_str()).is_some_and(&pred);
        if matches {
            files.push(path_str(&path)?);
        }
    }
    files.sort();
    Ok(files.join(","))
}

fn require(path: PathBuf) -> Result<PathBuf> {
    if path.exists() {
        Ok(path)
    } else {
        Err(AppError::Model(format!("Missing model file: {}", path.display())))
    }
}

fn path_str(path: &Path) -> Result<String> {
    path.to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| AppError::Path(format!("Non UTF-8 path: {}", path.display())))
}
