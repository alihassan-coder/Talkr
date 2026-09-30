//! The real `talkr-engine` process, spoken to over stdin/stdout as the app does. These need no
//! models: every request here fails (or answers) before a model is loaded.
//!
//! On Windows the sherpa-onnx DLLs must be on PATH (the build copies them next to the binaries in
//! the target's `debug` folder).

mod common;

use common::{model, synthesize, transcribe, Engine};
use std::time::{Duration, Instant};
use talkr_protocol::{DeviceKind, Event, FailureKind, Op, Request};

const T: Duration = Duration::from_secs(30);

fn request(id: &str, op: Op) -> Request {
    Request { id: id.into(), op }
}

fn failure(event: Event) -> (FailureKind, String) {
    match event {
        Event::Failed { kind, error, .. } => (kind, error),
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[test]
fn announces_itself_with_its_version() {
    let mut engine = Engine::spawn();
    match engine.next(T) {
        Event::Ready { version } => assert_eq!(version, env!("CARGO_PKG_VERSION")),
        other => panic!("expected Ready, got {other:?}"),
    }
}

#[test]
fn probe_lists_at_least_the_cpu() {
    let mut engine = Engine::start();
    match engine.call(request("p", Op::Probe), T).1 {
        Event::Devices { devices, .. } => {
            assert!(devices.iter().any(|d| d.kind == DeviceKind::Cpu), "{devices:?}");
        }
        other => panic!("expected Devices, got {other:?}"),
    }
}

#[test]
fn transcribing_a_missing_file_is_invalid() {
    let mut engine = Engine::start();
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nope.wav");
    let job = transcribe("t", model(dir.path(), "whisper-none", false), &missing);
    let (kind, error) = failure(engine.call(job, T).1);
    assert_eq!(kind, FailureKind::Invalid);
    assert!(error.contains("not found"), "{error}");
}

#[test]
fn transcribing_a_file_that_is_not_audio_is_invalid() {
    let mut engine = Engine::start();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("notes.mp3");
    std::fs::write(&path, "these are some notes, not audio").unwrap();
    let (kind, _) = failure(engine.call(transcribe("t", model(dir.path(), "whisper-none", false), &path), T).1);
    assert_eq!(kind, FailureKind::Invalid);
}

#[test]
fn synthesizing_empty_text_is_invalid() {
    let mut engine = Engine::start();
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out.wav");
    for (i, text) in ["", "   \n\t", "\0\0\0"].into_iter().enumerate() {
        let id = format!("s{i}");
        let (kind, error) = failure(engine.call(synthesize(&id, model(dir.path(), "piper-none", false), text, &out), T).1);
        assert_eq!(kind, FailureKind::Invalid, "{text:?}");
        assert!(error.contains("empty"), "{error}");
    }
    assert!(!out.exists());
}

#[test]
fn a_folder_that_is_not_a_voice_model_is_invalid_and_the_engine_survives() {
    let mut engine = Engine::start();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("voice.onnx"), b"<html>404</html>").unwrap();
    let bad = model(dir.path(), "piper-broken", false);

    let (kind, error) = failure(engine.call(synthesize("s", bad.clone(), "Hello.", &dir.path().join("o.wav")), T).1);
    assert_eq!(kind, FailureKind::Invalid);
    assert!(error.contains("ONNX"), "{error}");
    let (kind, _) = failure(engine.call(request("v", Op::Voices(bad)), T).1);
    assert_eq!(kind, FailureKind::Invalid);

    assert!(matches!(engine.call(request("p", Op::Probe), T).1, Event::Devices { .. }));
}

#[test]
fn malformed_lines_are_ignored() {
    let mut engine = Engine::start();
    engine.send_raw("this is not json\n");
    engine.send_raw("{\"id\":\"x\",\"op\":\"explode\"}\n");
    engine.send_raw("{}\n\n   \n");
    assert!(engine.is_quiet_for(Duration::from_millis(300)), "no reply to garbage");
    // Still serving, and the next reply is for the next real request.
    assert!(matches!(engine.call(request("p", Op::Probe), T).1, Event::Devices { .. }));
}

#[test]
fn unload_and_unknown_cancels_are_harmless() {
    let mut engine = Engine::start();
    engine.send(&request("ghost", Op::Cancel));
    assert!(matches!(engine.call(request("u", Op::Unload), T).1, Event::Unloaded { .. }));
    assert!(matches!(engine.call(request("p", Op::Probe), T).1, Event::Devices { .. }));
}

#[test]
fn closing_stdin_exits_cleanly_and_promptly() {
    let mut engine = Engine::start();
    let started = Instant::now();
    engine.close_stdin();
    let status = engine.wait_exit(Duration::from_secs(10)).expect("the engine exits when stdin closes");
    assert!(status.success(), "{status:?}\nlog:\n{}", engine.log());
    assert!(started.elapsed() < Duration::from_secs(5), "took {:?}", started.elapsed());
}
