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

use common::{model, model_dir, speak, synthesize, test_models, transcribe, Engine, PIPER_LESSAC, WHISPER_TINY_EN};
use std::path::Path;
use std::time::{Duration, Instant};
use talkr_protocol::{DeviceKind, Event, FailureKind, Op, Request};

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
