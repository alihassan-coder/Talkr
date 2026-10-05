//! End-to-end tests with real models, run through the `talkr-engine` process.
//!
//! They run only when `TALKR_TEST_MODELS` names a folder of installed models, laid out as the app
//! installs them, one folder per model id:
//!
//! ```text
//! $TALKR_TEST_MODELS/
//!   whisper-tiny-en/ggml-tiny.en.bin
//!   piper-en_US-lessac-medium/en_US-lessac-medium.onnx, tokens.txt, espeak-ng-data/, ...
//! ```
//!
//! CI fills it with `node apps/desktop/scripts/fetch-test-models.mjs <dir>`. Without the variable
//! each test prints a note and passes.
//!
//! The GPU test also needs `TALKR_TEST_GPU=1` and an engine built with `--features vulkan` (CI runs
//! it on Linux with Mesa's lavapipe software Vulkan driver).

mod common;

use common::{
    model, model_dir, speak, synthesize, test_models, transcribe, transcribe_with, Engine, PIPER_LESSAC, WHISPER_TINY_EN,
};
use std::path::Path;
use std::time::{Duration, Instant};
use talkr_protocol::{Decoding, DeviceKind, Event, FailureKind, Op, Request};

const SENTENCE: &str = "Hello world, this is a test.";
const LONG: Duration = Duration::from_secs(300);

fn transcript(event: Event, engine: &Engine) -> (String, i64, String) {
    match event {
        Event::Transcribed { transcript, audio_ms, device, .. } => (transcript.text, audio_ms, device),
        other => panic!("transcription failed: {other:?}\nlog:\n{}", engine.log()),
    }
}

fn assert_heard_the_sentence(text: &str) {
    // The tiny model sometimes hears "Hello" as "A low"; the rest of the sentence is reliable.
    let lower = text.to_lowercase();
    assert!(lower.contains("world") && lower.contains("test"), "whisper heard {text:?}");
}

fn assert_progress_is_sane(progress: &[f32]) {
    assert!(!progress.is_empty(), "no progress reported");
    assert!(progress.iter().all(|p| (0.0..=1.0).contains(p)), "{progress:?}");
    assert!(progress.windows(2).all(|w| w[0] <= w[1]), "progress went backwards: {progress:?}");
}

#[test]
fn piper_speech_round_trips_through_whisper() {
    let Some(models) = test_models("piper_speech_round_trips_through_whisper") else { return };
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("nested").join("hello.wav");
    let mut engine = Engine::start();

    // Speak.
    let piper = model(model_dir(&models, PIPER_LESSAC), PIPER_LESSAC, false);
    let (progress, done) = engine.call(synthesize("say", piper, SENTENCE, &wav), LONG);
    assert_progress_is_sane(&progress);
    let (rate, duration_ms) = match done {
        Event::Synthesized { sample_rate, duration_ms, device, .. } => {
            assert_eq!(device, "cpu");
            (sample_rate, duration_ms)
        }
        other => panic!("synthesis failed: {other:?}\nlog:\n{}", engine.log()),
    };
    assert_eq!(rate, 22_050, "lessac-medium speaks at 22.05 kHz");
    assert!((800..8_000).contains(&duration_ms), "{duration_ms} ms for one sentence");
    let reader = hound::WavReader::open(&wav).unwrap();
    assert_eq!(reader.spec().sample_rate, rate);
    assert_eq!(reader.spec().channels, 1);
    assert_eq!(reader.duration() as i64 * 1000 / rate as i64, duration_ms);

    // Listen.
    let whisper = model(model_dir(&models, WHISPER_TINY_EN), WHISPER_TINY_EN, false);
    let (progress, done) = engine.call(transcribe("hear", whisper, &wav), LONG);
    assert_progress_is_sane(&progress);
    let (text, audio_ms, device) = transcript(done, &engine);
    assert_heard_the_sentence(&text);
    assert!((audio_ms - duration_ms).abs() <= 1, "{audio_ms} ms heard vs {duration_ms} ms spoken");
    assert_eq!(device, "cpu");
}

#[test]
fn piper_lists_its_voice_and_ignores_nul_bytes() {
    let Some(models) = test_models("piper_lists_its_voice_and_ignores_nul_bytes") else { return };
    let dir = tempfile::tempdir().unwrap();
    let mut engine = Engine::start();
    let piper = model(model_dir(&models, PIPER_LESSAC), PIPER_LESSAC, false);

    match engine.call(Request { id: "v".into(), op: Op::Voices(piper.clone()) }, LONG).1 {
        Event::Voices { voices, .. } => {
            assert_eq!(voices.len(), 1, "{voices:?}");
            assert_eq!(voices[0].id, "0");
            assert_eq!(voices[0].language, "en-US");
        }
        other => panic!("{other:?}"),
    }

    let out = dir.path().join("stripped.wav");
    let done = engine.call(synthesize("nul", piper, "Hel\0lo there.\0", &out), LONG).1;
    assert!(matches!(done, Event::Synthesized { .. }), "{done:?}");
    assert!(out.is_file());
}

#[test]
fn cancelling_synthesis_stops_between_sentences() {
    let Some(models) = test_models("cancelling_synthesis_stops_between_sentences") else { return };
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("long.wav");
    let mut engine = Engine::start();
    let piper = model(model_dir(&models, PIPER_LESSAC), PIPER_LESSAC, false);
    let text = "This is one of many sentences that make a long read. ".repeat(40);

    engine.send(&synthesize("long", piper, &text, &out));
    // Wait until a sentence or two is done, then cancel.
    loop {
        match engine.next(LONG) {
            Event::Progress { progress, .. } if progress > 0.0 => break,
            Event::Progress { .. } => {}
            other => panic!("{other:?}"),
        }
    }
    engine.send(&Request { id: "long".into(), op: Op::Cancel });
    let cancelled_at = Instant::now();
    let (_, done) = engine.finish("long", LONG);
    assert!(matches!(done, Event::Failed { kind: FailureKind::Cancelled, .. }), "{done:?}");
    assert!(cancelled_at.elapsed() < Duration::from_secs(10), "took {:?}", cancelled_at.elapsed());
    assert!(!out.exists(), "a cancelled job writes no file");

    // The engine carries on.
    let short = dir.path().join("short.wav");
    speak(&mut engine, &models, SENTENCE, &short);
    assert!(short.is_file());
}

#[test]
fn cancelling_mid_transcription_reports_cancelled() {
    let Some(models) = test_models("cancelling_mid_transcription_reports_cancelled") else { return };
    let dir = tempfile::tempdir().unwrap();
    let mut engine = Engine::start();

    // About three minutes of speech: the sentence, repeated.
    let one = dir.path().join("one.wav");
    speak(&mut engine, &models, SENTENCE, &one);
    let long = dir.path().join("long.wav");
    repeat_wav(&one, &long, 180.0);

    let whisper = model(model_dir(&models, WHISPER_TINY_EN), WHISPER_TINY_EN, false);
    engine.send(&transcribe("long", whisper.clone(), &long));
    // The first progress event arrives as the job starts; give whisper a moment to get going.
    match engine.next(LONG) {
        Event::Progress { .. } => {}
        other => panic!("{other:?}"),
    }
    std::thread::sleep(Duration::from_millis(1500));
    engine.send(&Request { id: "long".into(), op: Op::Cancel });
    let cancelled_at = Instant::now();
    let (_, done) = engine.finish("long", LONG);
    assert!(matches!(done, Event::Failed { kind: FailureKind::Cancelled, .. }), "{done:?}\nlog:\n{}", engine.log());
    assert!(cancelled_at.elapsed() < Duration::from_secs(15), "took {:?}", cancelled_at.elapsed());

    // The engine and the loaded model still work.
    let (_, done) = engine.call(transcribe("again", whisper, &one), LONG);
    assert_heard_the_sentence(&transcript(done, &engine).0);
}

#[test]
fn beam_search_transcribes_too() {
    let Some(models) = test_models("beam_search_transcribes_too") else { return };
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("hello.wav");
    let mut engine = Engine::start();
    speak(&mut engine, &models, SENTENCE, &wav);

    let whisper = model(model_dir(&models, WHISPER_TINY_EN), WHISPER_TINY_EN, false);
    let (progress, done) = engine.call(transcribe_with("beam", whisper, &wav, Decoding::Beam), LONG);
    assert_progress_is_sane(&progress);
    assert_heard_the_sentence(&transcript(done, &engine).0);
}

/// Speed and word error rate of each decoding mode, on speech of known text. Run it with
/// `TALKR_BENCH=1` (and `TALKR_TEST_MODELS`; `TALKR_BENCH_MODEL` picks a Whisper model other than
/// the tiny one) and `--nocapture` to see the table.
#[test]
fn bench_decoding_modes() {
    if std::env::var("TALKR_BENCH").as_deref() != Ok("1") {
        eprintln!("skipping bench_decoding_modes: set TALKR_BENCH=1");
        return;
    }
    let Some(models) = test_models("bench_decoding_modes") else { return };
    let whisper_id = std::env::var("TALKR_BENCH_MODEL").unwrap_or_else(|_| WHISPER_TINY_EN.into());
    let dir = tempfile::tempdir().unwrap();
    let mut engine = Engine::start();

    let wav = dir.path().join("passage.wav");
    speak(&mut engine, &models, PASSAGE, &wav);
    let whisper = model(model_dir(&models, &whisper_id), &whisper_id, false);
    // Load the model first, so neither mode pays for it.
    engine.call(transcribe("warm", whisper.clone(), &wav), LONG);

    eprintln!("\n{whisper_id}, {} threads", whisper.threads);
    eprintln!("{:<8} {:>10} {:>12} {:>8}", "mode", "seconds", "x realtime", "WER");
    for (name, decoding) in [("greedy", Decoding::Greedy), ("beam", Decoding::Beam)] {
        let started = Instant::now();
        let (text, audio_ms, _) = transcript(engine.call(transcribe_with(name, whisper.clone(), &wav, decoding), LONG).1, &engine);
        let seconds = started.elapsed().as_secs_f64();
        let wer = word_error_rate(PASSAGE, &text);
        eprintln!("{name:<8} {seconds:>10.2} {:>12.1} {:>7.1}%", audio_ms as f64 / 1000.0 / seconds, wer * 100.0);
        eprintln!("         heard: {text}");
    }
}

const PASSAGE: &str = "The quick brown fox jumps over the lazy dog. \
    Please call Stella and ask her to bring these things with her from the store. \
    Six spoons of fresh snow peas, five thick slabs of blue cheese, and maybe a snack for her brother Bob. \
    We also need a small plastic snake and a big toy frog for the kids.";

/// Word-level edit distance over the number of reference words, ignoring case and punctuation.
fn word_error_rate(reference: &str, heard: &str) -> f64 {
    let words = |s: &str| -> Vec<String> {
        s.split_whitespace()
            .map(|w| w.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase())
            .filter(|w| !w.is_empty())
            .collect()
    };
    let (want, got) = (words(reference), words(heard));
    let mut row: Vec<usize> = (0..=got.len()).collect();
    for (i, w) in want.iter().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, g) in got.iter().enumerate() {
            let substitution = diagonal + usize::from(w != g);
            diagonal = row[j + 1];
            row[j + 1] = substitution.min(row[j] + 1).min(row[j + 1] + 1);
        }
    }
    row[got.len()] as f64 / want.len().max(1) as f64
}

#[test]
fn word_error_rate_counts_edits_per_reference_word() {
    assert_eq!(word_error_rate("Hello world, this is a test.", "hello world this is a test"), 0.0);
    assert_eq!(word_error_rate("one two three four", "one too three"), 0.5);
    assert_eq!(word_error_rate("one two", "one two three four"), 1.0);
}

#[test]
fn gpu_probe_and_transcription() {
    if std::env::var("TALKR_TEST_GPU").as_deref() != Ok("1") {
        eprintln!("skipping gpu_probe_and_transcription: set TALKR_TEST_GPU=1 (needs the vulkan build and a driver)");
        return;
    }
    let mut engine = Engine::start();
    let devices = match engine.call(Request { id: "p".into(), op: Op::Probe }, LONG).1 {
        Event::Devices { devices, .. } => devices,
        other => panic!("{other:?}"),
    };
    for d in &devices {
        eprintln!("device: {:?} {} ({}), {} of {} bytes free", d.kind, d.name, d.description, d.memory_free, d.memory_total);
    }
    assert!(
        devices.iter().any(|d| matches!(d.kind, DeviceKind::Gpu | DeviceKind::Igpu | DeviceKind::Accelerator)),
        "no GPU device found: {devices:?}"
    );

    let Some(models) = test_models("gpu_probe_and_transcription (transcription part)") else { return };
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("hello.wav");
    speak(&mut engine, &models, SENTENCE, &wav);
    let whisper = model(model_dir(&models, WHISPER_TINY_EN), WHISPER_TINY_EN, true);
    let (text, _, device) = transcript(engine.call(transcribe("gpu", whisper, &wav), LONG).1, &engine);
    eprintln!("GPU transcription on {device:?}: {text:?}");
    assert_heard_the_sentence(&text);
    assert_ne!(device, "cpu", "asked for the GPU but ran on the CPU\nlog:\n{}", engine.log());
}

/// Write `src` over and over into `dst` until it lasts `seconds`.
fn repeat_wav(src: &Path, dst: &Path, seconds: f32) {
    let mut reader = hound::WavReader::open(src).unwrap();
    let spec = reader.spec();
    let samples: Vec<i16> = reader.samples::<i16>().map(|s| s.unwrap()).collect();
    let mut writer = hound::WavWriter::create(dst, spec).unwrap();
    let total = (seconds * spec.sample_rate as f32) as usize * spec.channels as usize;
    for s in samples.iter().cycle().take(total) {
        writer.write_sample(*s).unwrap();
    }
    writer.finalize().unwrap();
}
