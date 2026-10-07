//! Drives the real `talkr-engine` binary over its stdin/stdout protocol.

#![allow(dead_code)] // each test file uses a different subset

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use talkr_protocol::{from_line, to_line, Decoding, Event, ModelRef, Op, Request, SynthesizeJob, TranscribeJob};

/// What the engine printed on stdout: an event, or a line that is not one (a protocol bug).
#[derive(Debug)]
pub enum Line {
    Event(Event),
    Garbage(String),
}

pub struct Engine {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<Line>,
    log: Arc<Mutex<String>>,
}

impl Engine {
    /// Start the engine and wait for its `Ready`.
    pub fn start() -> Self {
        let mut engine = Self::spawn();
        match engine.next(Duration::from_secs(30)) {
            Event::Ready { .. } => engine,
            other => panic!("expected Ready first, got {other:?}"),
        }
    }

    /// Start the engine without waiting for anything.
    pub fn spawn() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_talkr-engine"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start talkr-engine (on Windows, the sherpa-onnx DLLs must be on PATH)");
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                let parsed = match from_line::<Event>(&line) {
                    Ok(event) => Line::Event(event),
                    Err(_) => Line::Garbage(line),
                };
                if tx.send(parsed).is_err() {
                    break;
                }
            }
        });
        let log = Arc::new(Mutex::new(String::new()));
        let sink = log.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                let Ok(line) = line else { break };
                let mut log = sink.lock().unwrap();
                log.push_str(&line);
                log.push('\n');
            }
        });
        Self { child, stdin, lines, log }
    }

    /// The engine's stderr so far, for failure messages.
    pub fn log(&self) -> String {
        self.log.lock().unwrap().clone()
    }

    pub fn send(&mut self, request: &Request) {
        self.send_raw(&to_line(request));
    }

    pub fn send_raw(&mut self, text: &str) {
        let stdin = self.stdin.as_mut().expect("stdin is open");
        stdin.write_all(text.as_bytes()).unwrap();
        stdin.flush().unwrap();
    }

    /// The next event; fails the test on a timeout, a non-event line or the engine exiting.
    pub fn next(&mut self, timeout: Duration) -> Event {
        match self.lines.recv_timeout(timeout) {
            Ok(Line::Event(event)) => event,
            Ok(Line::Garbage(line)) => panic!("engine wrote a non-protocol line: {line:?}\nlog:\n{}", self.log()),
            Err(RecvTimeoutError::Timeout) => panic!("no event within {timeout:?}\nlog:\n{}", self.log()),
            Err(RecvTimeoutError::Disconnected) => {
                panic!("the engine exited ({:?})\nlog:\n{}", self.child.try_wait(), self.log())
            }
        }
    }

    /// Whether another line arrives within `wait`.
    pub fn is_quiet_for(&mut self, wait: Duration) -> bool {
        match self.lines.recv_timeout(wait) {
            Ok(line) => panic!("unexpected output: {line:?}"),
            Err(_) => true,
        }
    }

    /// Collect `id`'s progress values until its final event.
    pub fn finish(&mut self, id: &str, timeout: Duration) -> (Vec<f32>, Event) {
        let deadline = Instant::now() + timeout;
        let mut progress = Vec::new();
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.next(left.max(Duration::from_millis(1))) {
                Event::Progress { id: pid, progress: p } if pid == id => progress.push(p),
                event if event.id() == Some(id) && event.is_final() => return (progress, event),
                other => panic!("unexpected event while waiting for {id}: {other:?}"),
            }
        }
    }

    /// Send a request and wait for its final event.
    pub fn call(&mut self, request: Request, timeout: Duration) -> (Vec<f32>, Event) {
        let id = request.id.clone();
        self.send(&request);
        self.finish(&id, timeout)
    }

    pub fn close_stdin(&mut self) {
        self.stdin = None;
    }

    /// Wait for the process to exit, killing it after `timeout`.
    pub fn wait_exit(&mut self, timeout: Duration) -> Option<ExitStatus> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Some(status) = self.child.try_wait().unwrap() {
                return Some(status);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        None
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stdin = None;
        if self.wait_exit(Duration::from_secs(5)).is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

pub fn model(dir: impl Into<PathBuf>, id: &str, gpu: bool) -> ModelRef {
    ModelRef { model_id: id.into(), dir: dir.into(), threads: 4, gpu }
}

pub fn transcribe(id: &str, model: ModelRef, audio: &Path) -> Request {
    transcribe_with(id, model, audio, Decoding::Greedy)
}

pub fn transcribe_with(id: &str, model: ModelRef, audio: &Path, decoding: Decoding) -> Request {
    Request {
        id: id.into(),
        op: Op::Transcribe(TranscribeJob {
            model,
            audio_path: audio.into(),
            language: Some("en".into()),
            translate: false,
            decoding,
            prompt: None,
        }),
    }
}

pub fn synthesize(id: &str, model: ModelRef, text: &str, out: &Path) -> Request {
    Request {
        id: id.into(),
        op: Op::Synthesize(SynthesizeJob { model, text: text.into(), voice_id: "0".into(), speed: 1.0, out_path: out.into() }),
    }
}

/// The folder of installed test models named by `TALKR_TEST_MODELS`, or `None` (with a note
/// on stderr) when it is unset. Layout: `<dir>/<model id>/<model files>`.
pub fn test_models(test: &str) -> Option<PathBuf> {
    match std::env::var_os("TALKR_TEST_MODELS") {
        Some(dir) if !dir.is_empty() => {
            let dir = PathBuf::from(dir);
            assert!(dir.is_dir(), "TALKR_TEST_MODELS={} is not a folder", dir.display());
            Some(dir)
        }
        _ => {
            eprintln!("skipping {test}: set TALKR_TEST_MODELS to a folder of test models to run it");
            None
        }
    }
}

pub const WHISPER_TINY_EN: &str = "whisper-tiny-en";
pub const PIPER_LESSAC: &str = "piper-en_US-lessac-medium";

/// `<models>/<id>`, which must exist.
pub fn model_dir(models: &Path, id: &str) -> PathBuf {
    let dir = models.join(id);
    assert!(dir.is_dir(), "missing test model {} (expected {})", id, dir.display());
    dir
}

/// Speak `text` with the Piper test voice into `out`; returns the reported duration.
pub fn speak(engine: &mut Engine, models: &Path, text: &str, out: &Path) -> i64 {
    let piper = model(model_dir(models, PIPER_LESSAC), PIPER_LESSAC, false);
    match engine.call(synthesize("speak", piper, text, out), Duration::from_secs(120)) {
        (_, Event::Synthesized { duration_ms, .. }) => duration_ms,
        (_, other) => panic!("synthesis failed: {other:?}\nlog:\n{}", engine.log()),
    }
}
