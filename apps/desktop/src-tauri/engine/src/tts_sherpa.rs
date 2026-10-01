use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use sherpa_rs::tts::{CommonTtsConfig, KokoroTts, KokoroTtsConfig, TtsAudio, VitsTts, VitsTtsConfig};
use sherpa_rs::OnnxConfig;
use talkr_protocol::Voice;
use crate::{EngineError, JobControl, Result};

/// Synthesized mono audio.
pub struct AudioBuffer {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

/// Files espeak-ng cannot start without.
const ESPEAK_REQUIRED: &[&str] = &["phontab", "phondata", "phonindex"];

/// No real voice model is smaller than this.
const MIN_ONNX_BYTES: u64 = 1024;

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
    /// Inference threads it was loaded with; a new setting reloads the model.
    threads: usize,
    voices: Vec<Voice>,
}

/// A TTS model folder that has passed [`validate_model_dir`].
#[derive(Debug, Clone, PartialEq)]
pub enum ModelLayout {
    Kokoro {
        model: PathBuf,
        voices: PathBuf,
        tokens: PathBuf,
        data_dir: PathBuf,
        /// Speaker embeddings in `voices.bin`.
        num_speakers: usize,
    },
    Vits {
        model: PathBuf,
        tokens: PathBuf,
        data_dir: PathBuf,
    },
}

/// Check a TTS model folder before handing it to sherpa-onnx, which does not report a bad model
/// cleanly (sherpa-rs never checks the pointer it gets back, so a broken model crashes the
/// engine). Every failure is an [`EngineError::Invalid`] naming the file at fault.
///
/// Kokoro (a `voices.bin` is present): `model.onnx`, `voices.bin`, `tokens.txt`, `espeak-ng-data/`
/// (+ optional `lexicon*.txt`, `dict/`, `*.fst` for multi-lang).
/// Piper/VITS: `<name>.onnx`, `tokens.txt`, `espeak-ng-data/`.
pub fn validate_model_dir(dir: &Path) -> Result<ModelLayout> {
    if !dir.is_dir() {
        return Err(EngineError::Invalid(format!("The voice model folder does not exist: {}", dir.display())));
    }
    let voices = dir.join("voices.bin");
    if voices.exists() {
        let model = dir.join("model.onnx");
        check_onnx(&model, "Kokoro")?;
        let len = file_len(&voices, "Kokoro")?;
        if len < KOKORO_VOICE_BYTES || len % KOKORO_VOICE_BYTES != 0 {
            return Err(EngineError::Invalid(format!(
                "The Kokoro voices file is damaged or incomplete ({} bytes, expected a multiple of {}): {}",
                len,
                KOKORO_VOICE_BYTES,
                voices.display()
            )));
        }
        let tokens = check_tokens(dir, "Kokoro")?;
        let data_dir = check_espeak_data(dir, "Kokoro")?;
        let num_speakers = (len / KOKORO_VOICE_BYTES) as usize;
        Ok(ModelLayout::Kokoro { model, voices, tokens, data_dir, num_speakers })
    } else {
        let model = find_onnx_model(dir)?;
        check_onnx(&model, "Piper")?;
        let tokens = check_tokens(dir, "Piper")?;
        let data_dir = check_espeak_data(dir, "Piper")?;
        Ok(ModelLayout::Vits { model, tokens, data_dir })
    }
}

impl SherpaTtsEngine {
    /// Load a sherpa-onnx TTS model directory (see [`validate_model_dir`] for the layouts).
    pub fn new(model_dir: &Path, model_id: String, num_threads: usize) -> Result<Self> {
        let layout = validate_model_dir(model_dir)?;
        let onnx_config = OnnxConfig {
            provider: "cpu".into(),
            debug: false,
            num_threads: num_threads.max(1) as i32,
        };

        let (backend, voices) = match layout {
            ModelLayout::Kokoro { model, voices, tokens, data_dir, num_speakers } => {
                let lexicon = list_files(model_dir, |name| name.starts_with("lexicon") && name.ends_with(".txt"))?;
                let rule_fsts = list_files(model_dir, |name| name.ends_with(".fst"))?;
                let dict_dir = model_dir.join("dict");
                let config = KokoroTtsConfig {
                    model: path_str(&model)?,
                    voices: path_str(&voices)?,
                    tokens: path_str(&tokens)?,
                    data_dir: path_str(&data_dir)?,
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
                (Backend::Kokoro(KokoroTts::new(config)), kokoro_voices(num_speakers))
            }
            ModelLayout::Vits { model, tokens, data_dir } => {
                let config = VitsTtsConfig {
                    model: path_str(&model)?,
                    tokens: path_str(&tokens)?,
                    data_dir: path_str(&data_dir)?,
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
                (Backend::Vits(VitsTts::new(config)), vec![piper_voice(&model, &model_id)])
            }
        };

        Ok(Self {
            tts: Mutex::new(backend),
            model_id,
            threads: num_threads,
            voices,
        })
    }
}

impl SherpaTtsEngine {
    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    pub fn threads(&self) -> usize {
        self.threads
    }

    pub fn voices(&self) -> Vec<Voice> {
        self.voices.clone()
    }

    /// Speak `text` sentence by sentence. Progress is reported before each sentence, and a
    /// cancel takes effect between sentences.
    pub fn synthesize(&self, text: &str, voice_id: &str, speed: f32, ctl: &JobControl) -> Result<AudioBuffer> {
        // sherpa-rs turns the text into a C string and panics on a NUL byte.
        let text = text.replace('\0', "");
        let chunks = split_sentences(&text);
        if chunks.is_empty() {
            return Err(EngineError::Invalid("Text is empty".into()));
        }

        let sid = speaker_id(&self.voices, voice_id);
        let speed = if speed.is_finite() && speed > 0.0 { speed.clamp(0.25, 4.0) } else { 1.0 };
        let total = chunks.len() as f32;

        let mut all_samples = Vec::new();
        let mut sample_rate = 0u32;
        let mut tts = self.tts.lock().map_err(|_| EngineError::Engine("TTS engine poisoned".into()))?;

        for (i, chunk) in chunks.iter().enumerate() {
            ctl.check_cancelled()?;
            ctl.progress(i as f32 / total);

            let audio = tts.create(chunk, sid, speed).map_err(EngineError::Engine)?;
            if audio.samples.is_empty() {
                continue;
            }
            if sample_rate != 0 && audio.sample_rate != sample_rate {
                return Err(EngineError::Engine(format!(
                    "The voice changed sample rate mid-text ({} Hz, then {} Hz)",
                    sample_rate, audio.sample_rate
                )));
            }
            sample_rate = audio.sample_rate;
            all_samples.extend_from_slice(&audio.samples);
        }

        ctl.check_cancelled()?;
        ctl.progress(1.0);

        Ok(AudioBuffer {
            samples: all_samples,
            sample_rate,
        })
    }
}

/// The sherpa speaker id for `voice_id`: its index among `voices`, or a numeric id as given
/// (sherpa-onnx falls back to speaker 0 for one out of range), else 0.
fn speaker_id(voices: &[Voice], voice_id: &str) -> i32 {
    if let Some(idx) = voices.iter().position(|v| v.id == voice_id) {
        return idx as i32;
    }
    voice_id.trim().parse::<i32>().ok().filter(|&i| i >= 0).unwrap_or(0)
}

/// The one voice of a Piper model, named after its file: `en_US-lessac-medium.onnx` is
/// "en_US-lessac-medium" in "en-US".
fn piper_voice(model: &Path, model_id: &str) -> Voice {
    let name = model
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| model_id.to_string());
    let language = name.split('-').next().unwrap_or("").replace('_', "-");
    Voice {
        id: "0".into(),
        name,
        language,
        gender: None,
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

/// Longest chunk handed to the model at once. A run-on "sentence" is split at a comma or space
/// so one call never has to hold minutes of audio.
const MAX_CHUNK_CHARS: usize = 400;

/// Split text into sentence-sized chunks, keeping terminal punctuation for natural prosody.
///
/// `.`, `!` and `?` end a sentence only before whitespace or the end of the text (after any
/// closing quotes or brackets), so "3.14", "e.g.," and "..." stay whole; CJK full stops and line
/// breaks always end one. Chunks without a letter or digit are dropped.
fn split_sentences(text: &str) -> Vec<String> {
    const CLOSERS: &[char] = &['"', '\'', '\u{201D}', '\u{2019}', ')', ']', '\u{00BB}'];
    let chars: Vec<char> = text.chars().collect();
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        current.push(c);
        i += 1;
        let ends = match c {
            '\n' | '\u{3002}' | '\u{FF01}' | '\u{FF1F}' => true,
            '.' | '!' | '?' => {
                while i < chars.len() && CLOSERS.contains(&chars[i]) {
                    current.push(chars[i]);
                    i += 1;
                }
                i == chars.len() || chars[i].is_whitespace()
            }
            _ => false,
        };
        if ends {
            push_chunk(&mut chunks, &current);
            current.clear();
        }
    }
    push_chunk(&mut chunks, &current);
    chunks
}

/// Add `text` as one chunk, or as several if it is longer than [`MAX_CHUNK_CHARS`].
fn push_chunk(chunks: &mut Vec<String>, text: &str) {
    let mut rest = text.trim();
    while rest.chars().count() > MAX_CHUNK_CHARS {
        // Byte offset of the character just past the limit.
        let limit = rest.char_indices().nth(MAX_CHUNK_CHARS).map_or(rest.len(), |(b, _)| b);
        let head = &rest[..limit];
        let cut = head
            .rfind([',', ';', ':'])
            .map(|b| b + 1)
            .or_else(|| head.rfind(char::is_whitespace))
            .filter(|&b| b > 0)
            .unwrap_or(limit);
        let (piece, tail) = rest.split_at(cut);
        push_one(chunks, piece);
        rest = tail.trim_start();
    }
    push_one(chunks, rest);
}

fn push_one(chunks: &mut Vec<String>, piece: &str) {
    let piece = piece.trim();
    if piece.chars().any(|c| c.is_alphanumeric()) {
        chunks.push(piece.to_string());
    }
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
        .ok_or_else(|| EngineError::Invalid(format!("No .onnx model found in {}", dir.display())))
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

fn file_len(path: &Path, kind: &str) -> Result<u64> {
    match std::fs::metadata(path) {
        Ok(m) if m.is_file() => Ok(m.len()),
        _ => Err(EngineError::Invalid(format!("The {} voice model is missing {}", kind, path.display()))),
    }
}

/// An ONNX file is a protobuf `ModelProto`, which every exporter starts with its `ir_version`
/// field (tag byte 0x08). Catches an empty, truncated or mislabelled download.
fn check_onnx(path: &Path, kind: &str) -> Result<()> {
    let len = file_len(path, kind)?;
    let mut first = [0u8; 1];
    let looks_right = len >= MIN_ONNX_BYTES
        && std::fs::File::open(path).and_then(|mut f| f.read_exact(&mut first)).is_ok()
        && first[0] == 0x08;
    if !looks_right {
        return Err(EngineError::Invalid(format!(
            "The {} voice model file is damaged or not an ONNX model: {}",
            kind,
            path.display()
        )));
    }
    Ok(())
}

/// `tokens.txt` holds one `<token> <id>` pair per line (the token may itself be a space).
fn check_tokens(dir: &Path, kind: &str) -> Result<PathBuf> {
    let path = dir.join("tokens.txt");
    file_len(&path, kind)?;
    let bad = |why: &str| EngineError::Invalid(format!("The {} voice model's tokens.txt is {}: {}", kind, why, path.display()));
    let text = String::from_utf8(std::fs::read(&path)?).map_err(|_| bad("not UTF-8 text"))?;
    let mut count = 0;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let id = line.split_whitespace().last().unwrap_or("");
        if id.parse::<u32>().is_err() {
            return Err(bad("damaged"));
        }
        count += 1;
    }
    if count == 0 {
        return Err(bad("empty"));
    }
    Ok(path)
}

/// espeak-ng turns text into phonemes for both model kinds, and aborts the process if its data
/// is missing.
fn check_espeak_data(dir: &Path, kind: &str) -> Result<PathBuf> {
    let data = dir.join("espeak-ng-data");
    if !data.is_dir() {
        return Err(EngineError::Invalid(format!(
            "The {} voice model is missing its espeak-ng-data folder: {}",
            kind,
            data.display()
        )));
    }
    for name in ESPEAK_REQUIRED {
        if !data.join(name).is_file() {
            return Err(EngineError::Invalid(format!(
                "The {} voice model's espeak-ng-data folder is incomplete (no {}): {}",
                kind,
                name,
                data.display()
            )));
        }
    }
    Ok(data)
}

fn path_str(path: &Path) -> Result<String> {
    path.to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| EngineError::Invalid(format!("Non UTF-8 path: {}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // ---- sentence splitting ----

    #[test]
    fn splits_on_sentence_ends_and_keeps_the_punctuation() {
        assert_eq!(
            split_sentences("Hello world. How are you?  Fine!\nNew line"),
            ["Hello world.", "How are you?", "Fine!", "New line"]
        );
        assert_eq!(split_sentences("你好。世界！好吗？"), ["你好。", "世界！", "好吗？"]);
    }

    #[test]
    fn does_not_split_inside_numbers_abbreviations_or_ellipses() {
        assert_eq!(split_sentences("Pi is 3.14 roughly. Next."), ["Pi is 3.14 roughly.", "Next."]);
        assert_eq!(split_sentences("Use e.g. this one."), ["Use e.g.", "this one."]);
        assert_eq!(split_sentences("Wait... what?! Yes."), ["Wait...", "what?!", "Yes."]);
        assert_eq!(split_sentences("See www.example.com now"), ["See www.example.com now"]);
    }

    #[test]
    fn closing_quotes_stay_with_their_sentence() {
        assert_eq!(
            split_sentences("He said \"stop.\" Then left. (Really.) Done"),
            ["He said \"stop.\"", "Then left.", "(Really.)", "Done"]
        );
        assert_eq!(split_sentences("\u{201C}Hi!\u{201D} she said."), ["\u{201C}Hi!\u{201D}", "she said."]);
    }

    #[test]
    fn drops_chunks_without_words() {
        assert!(split_sentences("").is_empty());
        assert!(split_sentences("   \n\n ... !!! ??").is_empty());
        assert_eq!(split_sentences("... Hello ..."), ["Hello ..."]);
        assert_eq!(split_sentences("a"), ["a"]);
    }

    #[test]
    fn run_on_text_is_cut_at_commas_or_spaces() {
        let clause = "and then the quick brown fox jumped over the lazy dog, ";
        let text = clause.repeat(30); // ~1650 chars, no sentence end
        let chunks = split_sentences(&text);
        assert!(chunks.len() >= 4, "{} chunks", chunks.len());
        for c in &chunks {
            assert!(c.chars().count() <= MAX_CHUNK_CHARS, "{} chars", c.chars().count());
            assert!(c.ends_with(',') || c.ends_with("dog"), "cut mid-clause: {c:?}");
        }
        // Nothing is lost: the words come back in order.
        let rejoined: Vec<&str> = chunks.iter().flat_map(|c| c.split_whitespace()).collect();
        let original: Vec<&str> = text.split_whitespace().collect();
        assert_eq!(rejoined, original);

        // No spaces at all (and multi-byte characters): a hard cut on a character boundary.
        let solid = "é".repeat(MAX_CHUNK_CHARS * 2 + 10);
        let chunks = split_sentences(&solid);
        assert_eq!(chunks.iter().map(|c| c.chars().count()).collect::<Vec<_>>(), [400, 400, 10]);
    }

    // ---- voices ----

    #[test]
    fn kokoro_voice_names_map_to_languages_and_genders() {
        let v = kokoro_voice("af_bella");
        assert_eq!((v.id.as_str(), v.name.as_str(), v.language.as_str()), ("af_bella", "Bella", "en-US"));
        assert_eq!(v.gender.as_deref(), Some("female"));
        let v = kokoro_voice("bm_george");
        assert_eq!((v.name.as_str(), v.language.as_str(), v.gender.as_deref()), ("George", "en-GB", Some("male")));
        let v = kokoro_voice("zm_yunyang");
        assert_eq!((v.language.as_str(), v.gender.as_deref()), ("zh", Some("male")));
        let v = kokoro_voice("af");
        assert_eq!((v.name.as_str(), v.language.as_str()), ("Af", "en-US"));
        let v = kokoro_voice("xx_odd");
        assert_eq!((v.name.as_str(), v.language.as_str(), v.gender), ("Odd", "", None));
    }

    #[test]
    fn kokoro_voice_lists_follow_the_speaker_count() {
        let en = kokoro_voices(KOKORO_EN_V0_19.len());
        assert_eq!(en.len(), 11);
        assert_eq!(en[0].id, "af");
        assert_eq!(en[10].id, "bm_lewis");

        let multi = kokoro_voices(KOKORO_MULTI_V1_0.len());
        assert_eq!(multi.len(), 53);
        assert_eq!(multi[3].id, "af_heart");
        assert_eq!(multi[52].id, "zm_yunyang");
        let ids: std::collections::HashSet<_> = multi.iter().map(|v| &v.id).collect();
        assert_eq!(ids.len(), 53, "voice ids are unique");

        let unknown = kokoro_voices(5);
        assert_eq!(unknown.iter().map(|v| v.id.as_str()).collect::<Vec<_>>(), ["0", "1", "2", "3", "4"]);
        assert_eq!(unknown[2].name, "Speaker 2");
        assert_eq!(kokoro_voices(0).len(), 1, "at least one voice");
    }

    #[test]
    fn speaker_ids_resolve_by_voice_id_then_number() {
        let voices = kokoro_voices(KOKORO_MULTI_V1_0.len());
        assert_eq!(speaker_id(&voices, "af_alloy"), 0);
        assert_eq!(speaker_id(&voices, "af_heart"), 3);
        assert_eq!(speaker_id(&voices, "zm_yunyang"), 52);
        assert_eq!(speaker_id(&voices, "7"), 7);
        assert_eq!(speaker_id(&voices, " 12 "), 12);
        assert_eq!(speaker_id(&voices, "-3"), 0);
        assert_eq!(speaker_id(&voices, "nobody"), 0);
        assert_eq!(speaker_id(&voices, ""), 0);
        let unknown = kokoro_voices(4);
        assert_eq!(speaker_id(&unknown, "2"), 2);
    }

    #[test]
    fn piper_voices_are_named_after_the_model_file() {
        let v = piper_voice(Path::new("/m/en_US-lessac-medium.onnx"), "piper-x");
        assert_eq!((v.id.as_str(), v.name.as_str(), v.language.as_str()), ("0", "en_US-lessac-medium", "en-US"));
        let v = piper_voice(Path::new("/m/de_DE-thorsten-high.onnx"), "piper-x");
        assert_eq!(v.language, "de-DE");
    }

    // ---- model folder validation ----

    fn fake_onnx(path: &Path) {
        let mut bytes = vec![0x08, 0x07];
        bytes.resize(MIN_ONNX_BYTES as usize + 10, 0);
        fs::write(path, bytes).unwrap();
    }

    fn fake_espeak(dir: &Path) {
        let data = dir.join("espeak-ng-data");
        fs::create_dir_all(&data).unwrap();
        for name in ESPEAK_REQUIRED {
            fs::write(data.join(name), b"x").unwrap();
        }
    }

    /// A folder that passes validation as a Piper model.
    fn piper_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fake_onnx(&dir.path().join("en_US-test-low.onnx"));
        fs::write(dir.path().join("en_US-test-low.onnx.json"), b"{}").unwrap();
        fs::write(dir.path().join("tokens.txt"), "_ 0\n^ 1\n  3\n! 4\r\n\n").unwrap();
        fake_espeak(dir.path());
        dir
    }

    /// A folder that passes validation as a Kokoro model with `speakers` voices.
    fn kokoro_dir(speakers: u64) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fake_onnx(&dir.path().join("model.onnx"));
        fs::File::create(dir.path().join("voices.bin")).unwrap().set_len(KOKORO_VOICE_BYTES * speakers).unwrap();
        fs::write(dir.path().join("tokens.txt"), "$ 0\n; 1\n").unwrap();
        fake_espeak(dir.path());
        dir
    }

    fn invalid(dir: &Path) -> String {
        match validate_model_dir(dir) {
            Err(EngineError::Invalid(m)) => m,
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn a_complete_piper_folder_validates() {
        let dir = piper_dir();
        let layout = validate_model_dir(dir.path()).unwrap();
        assert_eq!(
            layout,
            ModelLayout::Vits {
                model: dir.path().join("en_US-test-low.onnx"),
                tokens: dir.path().join("tokens.txt"),
                data_dir: dir.path().join("espeak-ng-data"),
            }
        );
    }

    #[test]
    fn a_complete_kokoro_folder_validates_and_counts_speakers() {
        let dir = kokoro_dir(11);
        match validate_model_dir(dir.path()).unwrap() {
            ModelLayout::Kokoro { model, num_speakers, .. } => {
                assert_eq!(model, dir.path().join("model.onnx"));
                assert_eq!(num_speakers, 11);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_missing_folder_is_invalid() {
        let dir = tempfile::tempdir().unwrap();
        assert!(invalid(&dir.path().join("gone")).contains("does not exist"));
    }

    #[test]
    fn piper_folder_problems_are_named() {
        let dir = piper_dir();
        fs::remove_file(dir.path().join("en_US-test-low.onnx")).unwrap();
        assert!(invalid(dir.path()).contains("No .onnx model"));

        let dir = piper_dir();
        fs::write(dir.path().join("en_US-test-low.onnx"), b"").unwrap();
        assert!(invalid(dir.path()).contains("not an ONNX model"));

        let dir = piper_dir();
        fs::write(dir.path().join("en_US-test-low.onnx"), vec![b'<'; 4096]).unwrap();
        assert!(invalid(dir.path()).contains("not an ONNX model"), "an HTML error page saved as .onnx");

        let dir = piper_dir();
        fs::remove_file(dir.path().join("tokens.txt")).unwrap();
        assert!(invalid(dir.path()).contains("tokens.txt"));

        let dir = piper_dir();
        fs::write(dir.path().join("tokens.txt"), "\n \n").unwrap();
        assert!(invalid(dir.path()).contains("tokens.txt is empty"));

        let dir = piper_dir();
        fs::write(dir.path().join("tokens.txt"), "_ 0\nbroken line\n").unwrap();
        assert!(invalid(dir.path()).contains("tokens.txt is damaged"));

        let dir = piper_dir();
        fs::write(dir.path().join("tokens.txt"), [0xff, 0xfe, b' ', b'1']).unwrap();
        assert!(invalid(dir.path()).contains("not UTF-8"));

        let dir = piper_dir();
        fs::remove_dir_all(dir.path().join("espeak-ng-data")).unwrap();
        assert!(invalid(dir.path()).contains("espeak-ng-data folder"));

        let dir = piper_dir();
        fs::remove_file(dir.path().join("espeak-ng-data").join("phontab")).unwrap();
        assert!(invalid(dir.path()).contains("incomplete (no phontab)"));
    }

    #[test]
    fn kokoro_folder_problems_are_named() {
        let dir = kokoro_dir(2);
        fs::rename(dir.path().join("model.onnx"), dir.path().join("kokoro.onnx")).unwrap();
        let m = invalid(dir.path());
        assert!(m.contains("Kokoro") && m.contains("model.onnx"), "{m}");

        let dir = kokoro_dir(2);
        fs::File::options().write(true).open(dir.path().join("voices.bin")).unwrap().set_len(KOKORO_VOICE_BYTES + 7).unwrap();
        assert!(invalid(dir.path()).contains("voices file is damaged"));

        let dir = kokoro_dir(2);
        fs::write(dir.path().join("voices.bin"), b"").unwrap();
        assert!(invalid(dir.path()).contains("voices file is damaged"));

        let dir = kokoro_dir(2);
        fs::remove_file(dir.path().join("tokens.txt")).unwrap();
        assert!(invalid(dir.path()).contains("tokens.txt"));

        let dir = kokoro_dir(2);
        fs::remove_dir_all(dir.path().join("espeak-ng-data")).unwrap();
        assert!(invalid(dir.path()).contains("espeak-ng-data"));
    }

    #[test]
    fn loading_a_bad_folder_fails_before_sherpa_sees_it() {
        let dir = piper_dir();
        fs::write(dir.path().join("en_US-test-low.onnx"), b"not a model").unwrap();
        let err = SherpaTtsEngine::new(dir.path(), "piper-test".into(), 1).err().expect("must fail");
        assert!(matches!(err, EngineError::Invalid(_)), "{err:?}");
    }
}
