//! The engine's request loop: requests arrive on stdin, events leave on stdout (see
//! `talkr_protocol`). Jobs run one at a time on a job thread, so memory use stays at one model
//! plus one job, while the reader thread stays free to take cancellations.

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use talkr_protocol::{
    from_line, to_line, Device, Event, FailureKind, ModelRef, Op, Request, SynthesizeJob, TranscribeJob, Transcript,
    Voice,
};
use crate::stt_whisper::{TranscribeOptions, WhisperEngine};
use crate::tts_sherpa::SherpaTtsEngine;
use crate::{audio, devices, EngineError, JobControl, Result};

/// What a finished transcription reports besides the text.
pub struct Transcribed {
    pub transcript: Transcript,
    pub audio_ms: i64,
    pub device: String,
}

pub struct Synthesized {
    pub sample_rate: u32,
    pub duration_ms: i64,
    pub device: String,
}

/// The engines the loop drives. `NativeEngines` is the real one; tests substitute their own.
pub trait Engines {
    fn probe(&mut self) -> Vec<Device>;
    fn transcribe(&mut self, job: &TranscribeJob, ctl: &JobControl) -> Result<Transcribed>;
    fn synthesize(&mut self, job: &SynthesizeJob, ctl: &JobControl) -> Result<Synthesized>;
    fn voices(&mut self, model: &ModelRef) -> Result<Vec<Voice>>;
    fn unload(&mut self);
}

type Writer = Arc<Mutex<Box<dyn Write + Send>>>;

/// How often the heartbeat speaks for a job that is working but has no new progress to report.
/// whisper.cpp reports every 5 % of the audio, which on a slow CPU and a long file can be many
/// minutes apart; the app restarts an engine it has not heard from in 10 minutes.
pub const HEARTBEAT: Duration = Duration::from_secs(20);

/// The job on the job thread, as the heartbeat sees it.
struct RunningJob {
    id: String,
    /// Grows while the engine calls back (see `JobControl::alive`).
    alive: Arc<AtomicU64>,
    /// The last progress reported, as `f32` bits.
    progress: Arc<AtomicU32>,
}

/// Jobs accepted and not answered yet. Lock order: this table, then the output writer.
#[derive(Default)]
struct Jobs {
    /// Cancel flags of queued and running jobs.
    flags: HashMap<String, Arc<AtomicBool>>,
    running: Option<RunningJob>,
}

type JobTable = Arc<Mutex<Jobs>>;

fn send(out: &Writer, event: &Event) {
    let mut out = out.lock().unwrap_or_else(|e| e.into_inner());
    // If the app has gone away there is nobody to tell; the loop ends when stdin closes.
    let _ = out.write_all(to_line(event).as_bytes());
    let _ = out.flush();
}

/// Run the loop until `input` closes. Returns once the job in progress, if any, has stopped.
pub fn serve<R, W, E>(input: R, output: W, engines: E)
where
    R: BufRead,
    W: Write + Send + 'static,
    E: Engines + Send + 'static,
{
    serve_with(input, output, engines, HEARTBEAT)
}

fn serve_with<R, W, E>(input: R, output: W, engines: E, heartbeat: Duration)
where
    R: BufRead,
    W: Write + Send + 'static,
    E: Engines + Send + 'static,
{
    let out: Writer = Arc::new(Mutex::new(Box::new(output)));
    let table: JobTable = Arc::default();
    send(&out, &Event::Ready { version: env!("CARGO_PKG_VERSION").into() });

    let (jobs_tx, jobs_rx) = mpsc::channel::<Request>();
    let job_thread = {
        let out = out.clone();
        let table = table.clone();
        std::thread::Builder::new()
            .name("talkr-engine-job".into())
            // whisper.cpp and onnxruntime use deep native stacks; don't rely on the platform default.
            .stack_size(8 << 20)
            .spawn(move || run_jobs(jobs_rx, engines, &out, &table))
            .expect("spawn the engine job thread")
    };
    let (stop_heartbeat, heartbeat_stopped) = mpsc::channel::<()>();
    let heartbeat_thread = {
        let out = out.clone();
        let table = table.clone();
        std::thread::Builder::new()
            .name("talkr-engine-heartbeat".into())
            .spawn(move || beat(&table, &out, &heartbeat_stopped, heartbeat))
            .expect("spawn the engine heartbeat thread")
    };

    for line in input.lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let request: Request = match from_line(&line) {
            Ok(r) => r,
            Err(e) => {
                log::error!("ignoring malformed request: {}", e);
                continue;
            }
        };
        match request.op {
            Op::Cancel => cancel(&table, &out, request.id),
            _ => {
                lock(&table).flags.insert(request.id.clone(), Arc::new(AtomicBool::new(false)));
                if jobs_tx.send(request).is_err() {
                    break;
                }
            }
        }
    }

    // The app closed our stdin: stop whatever is running and leave.
    for flag in lock(&table).flags.values() {
        flag.store(true, Ordering::Relaxed);
    }
    drop(jobs_tx);
    let _ = job_thread.join();
    drop(stop_heartbeat);
    let _ = heartbeat_thread.join();
}

/// Cancel job `id`. A running job is flagged and answers once it notices. A queued job is
/// answered right here and dropped from the table, so the app is not kept waiting behind the
/// job ahead of it, and never mistakes that job for one that ignores its cancellation.
fn cancel(table: &JobTable, out: &Writer, id: String) {
    let mut jobs = lock(table);
    let running = jobs.running.as_ref().is_some_and(|r| r.id == id);
    if running {
        if let Some(flag) = jobs.flags.get(&id) {
            flag.store(true, Ordering::Relaxed);
        }
    } else if jobs.flags.remove(&id).is_some() {
        send(out, &Event::Failed { id, kind: FailureKind::Cancelled, error: EngineError::Cancelled.to_string() });
    }
}

/// Speak for the running job every `every` while its engine keeps calling back, by repeating its
/// last progress. Silence from an engine that stopped calling back is left for the app's
/// watchdog to notice.
fn beat(table: &JobTable, out: &Writer, stop: &mpsc::Receiver<()>, every: Duration) {
    let mut seen: Option<(String, u64)> = None;
    while let Err(mpsc::RecvTimeoutError::Timeout) = stop.recv_timeout(every) {
        let jobs = lock(table);
        let Some(job) = &jobs.running else {
            seen = None;
            continue;
        };
        let count = job.alive.load(Ordering::Relaxed);
        let advanced = seen.as_ref().is_some_and(|(id, last)| *id == job.id && *last != count);
        if advanced {
            let progress = f32::from_bits(job.progress.load(Ordering::Relaxed));
            send(out, &Event::Progress { id: job.id.clone(), progress });
        }
        seen = Some((job.id.clone(), count));
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn run_jobs<E: Engines>(jobs: mpsc::Receiver<Request>, mut engines: E, out: &Writer, table: &JobTable) {
    for request in jobs {
        let id = request.id.clone();
        let progress = Arc::new(AtomicU32::new(0f32.to_bits()));
        let ctl = {
            let mut jobs = lock(table);
            // No flag: it was cancelled while queued, and `cancel` has answered it already.
            let Some(cancel) = jobs.flags.get(&id).cloned() else { continue };
            let progress_out = out.clone();
            let progress_id = id.clone();
            let last_progress = progress.clone();
            let ctl = JobControl::new(
                Arc::new(move |p: f32| {
                    last_progress.store(p.to_bits(), Ordering::Relaxed);
                    send(&progress_out, &Event::Progress { id: progress_id.clone(), progress: p })
                }),
                cancel,
            );
            jobs.running = Some(RunningJob { id: id.clone(), alive: ctl.alive(), progress });
            ctl
        };

        let event = match catch_unwind(AssertUnwindSafe(|| handle(&mut engines, &request, &ctl))) {
            Ok(Ok(event)) => event,
            Ok(Err(e)) => Event::Failed { id: id.clone(), kind: e.kind(), error: e.to_string() },
            Err(panic) => {
                // A panicking engine may have left its model half-updated; start clean next time.
                engines.unload();
                Event::Failed { id: id.clone(), kind: FailureKind::Engine, error: panic_message(panic) }
            }
        };
        {
            // Off the table before the answer goes out, so no heartbeat follows it.
            let mut jobs = lock(table);
            jobs.flags.remove(&id);
            jobs.running = None;
        }
        send(out, &event);
    }
}

fn handle<E: Engines>(engines: &mut E, request: &Request, ctl: &JobControl) -> Result<Event> {
    let id = request.id.clone();
    ctl.check_cancelled()?;
    Ok(match &request.op {
        Op::Probe => Event::Devices { id, devices: engines.probe() },
        Op::Transcribe(job) => {
            let done = engines.transcribe(job, ctl)?;
            Event::Transcribed { id, transcript: done.transcript, audio_ms: done.audio_ms, device: done.device }
        }
        Op::Synthesize(job) => {
            let done = engines.synthesize(job, ctl)?;
            Event::Synthesized { id, sample_rate: done.sample_rate, duration_ms: done.duration_ms, device: done.device }
        }
        Op::Voices(model) => Event::Voices { id, voices: engines.voices(model)? },
        Op::Unload => {
            engines.unload();
            Event::Unloaded { id }
        }
        Op::Cancel => unreachable!("cancel requests are handled by the reader"),
    })
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    let message = payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown error".into());
    format!("The engine stopped unexpectedly: {}", message)
}

/// whisper.cpp and sherpa-onnx, keeping the last-used model of each kind loaded. Loading one kind
/// frees the other, so a small machine only ever holds one model.
#[derive(Default)]
pub struct NativeEngines {
    stt: Option<WhisperEngine>,
    tts: Option<SherpaTtsEngine>,
}

impl NativeEngines {
    fn stt(&mut self, model: &ModelRef) -> Result<&WhisperEngine> {
        let cached = self
            .stt
            .as_ref()
            .is_some_and(|e| e.model_id() == model.model_id && e.gpu_requested() == model.gpu);
        if !cached {
            // Free the old model before loading the new one, not after.
            self.stt = None;
            self.tts = None;
            self.stt = Some(WhisperEngine::new(&model.dir, model.model_id.clone(), model.gpu)?);
        }
        Ok(self.stt.as_ref().expect("just loaded"))
    }

    fn tts(&mut self, model: &ModelRef) -> Result<&SherpaTtsEngine> {
        // Thread count is fixed when onnxruntime loads the model, so a changed setting reloads it.
        if !self.tts.as_ref().is_some_and(|e| e.model_id() == model.model_id && e.threads() == model.threads) {
            self.tts = None;
            self.stt = None;
            self.tts = Some(SherpaTtsEngine::new(&model.dir, model.model_id.clone(), model.threads)?);
        }
        Ok(self.tts.as_ref().expect("just loaded"))
    }
}

impl Engines for NativeEngines {
    fn probe(&mut self) -> Vec<Device> {
        devices::list()
    }

    fn transcribe(&mut self, job: &TranscribeJob, ctl: &JobControl) -> Result<Transcribed> {
        ctl.progress(0.0);
        // Decode first: a bad file should fail fast, before a model load.
        let samples = audio::load_16k_mono_with(&job.audio_path, audio::LoadOptions { ctl: Some(ctl), ..Default::default() })?;
        ctl.check_cancelled()?;
        let engine = self.stt(&job.model)?;
        ctl.check_cancelled()?;
        let options = TranscribeOptions {
            language: job.language.as_deref(),
            translate: job.translate,
            threads: job.model.threads,
            decoding: job.decoding,
            prompt: job.prompt.as_deref(),
        };
        let transcript = engine.transcribe(&samples, &options, ctl)?;
        Ok(Transcribed {
            transcript,
            audio_ms: (samples.len() as i64) * 1000 / 16_000,
            device: engine.device().to_string(),
        })
    }

    fn synthesize(&mut self, job: &SynthesizeJob, ctl: &JobControl) -> Result<Synthesized> {
        ctl.progress(0.0);
        // sherpa-rs turns the text into a C string and panics on a NUL byte.
        let text = job.text.replace('\0', "");
        if text.trim().is_empty() {
            return Err(EngineError::Invalid("Text is empty".into()));
        }
        let engine = self.tts(&job.model)?;
        ctl.check_cancelled()?;
        let audio = engine.synthesize(&text, &job.voice_id, job.speed, ctl)?;
        if audio.samples.is_empty() || audio.sample_rate == 0 {
            return Err(EngineError::Engine("The voice produced no audio".into()));
        }
        if let Some(parent) = job.out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        audio::wav::write_wav(&job.out_path, &audio.samples, audio.sample_rate)?;
        Ok(Synthesized {
            sample_rate: audio.sample_rate,
            duration_ms: audio.samples.len() as i64 * 1000 / audio.sample_rate as i64,
            device: "cpu".into(),
        })
    }

    fn voices(&mut self, model: &ModelRef) -> Result<Vec<Voice>> {
        Ok(self.tts(model)?.voices())
    }

    fn unload(&mut self) {
        self.stt = None;
        self.tts = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufReader, Read};
    use std::time::Duration;

    /// Scriptable engines: `transcribe` waits until cancelled when the audio path is "wait",
    /// panics when it is "panic", and fails with out-of-memory when it is "oom".
    #[derive(Default)]
    struct FakeEngines {
        unloads: Arc<Mutex<u32>>,
    }

    impl Engines for FakeEngines {
        fn probe(&mut self) -> Vec<Device> {
            vec![]
        }

        fn transcribe(&mut self, job: &TranscribeJob, ctl: &JobControl) -> Result<Transcribed> {
            match job.audio_path.to_str().unwrap() {
                "wait" => loop {
                    ctl.check_cancelled()?;
                    std::thread::sleep(Duration::from_millis(5));
                },
                "panic" => panic!("boom"),
                // Working for a while: one progress report, then only cancellation checks.
                "busy" => {
                    ctl.progress(0.25);
                    for _ in 0..60 {
                        ctl.check_cancelled()?;
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Ok(Transcribed {
                        transcript: Transcript { text: "busy".into(), segments: vec![], language: None },
                        audio_ms: 10,
                        device: "cpu".into(),
                    })
                }
                // Stuck in native code: no callbacks at all.
                "hung" => {
                    std::thread::sleep(Duration::from_millis(300));
                    Err(EngineError::Engine("gave up".into()))
                }
                "oom" => Err(EngineError::OutOfMemory("no memory".into())),
                _ => {
                    ctl.progress(0.5);
                    Ok(Transcribed {
                        transcript: Transcript { text: "hi".into(), segments: vec![], language: Some("en".into()) },
                        audio_ms: 10,
                        device: "cpu".into(),
                    })
                }
            }
        }

        fn synthesize(&mut self, _: &SynthesizeJob, _: &JobControl) -> Result<Synthesized> {
            Err(EngineError::Invalid("Text is empty".into()))
        }

        fn voices(&mut self, _: &ModelRef) -> Result<Vec<Voice>> {
            Ok(vec![])
        }

        fn unload(&mut self) {
            *self.unloads.lock().unwrap() += 1;
        }
    }

    fn model() -> ModelRef {
        ModelRef { model_id: "m".into(), dir: "d".into(), threads: 1, gpu: false }
    }

    fn transcribe(id: &str, path: &str) -> String {
        to_line(&Request {
            id: id.into(),
            op: Op::Transcribe(TranscribeJob { model: model(), audio_path: path.into(), language: None, translate: false, decoding: Default::default(), prompt: None }),
        })
    }

    /// A pipe: the loop reads from the returned reader; tests write requests into the sender.
    struct Pipe(mpsc::Receiver<Vec<u8>>, Vec<u8>);

    impl Read for Pipe {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.1.is_empty() {
                match self.0.recv() {
                    Ok(bytes) => self.1 = bytes,
                    Err(_) => return Ok(0),
                }
            }
            let n = buf.len().min(self.1.len());
            buf[..n].copy_from_slice(&self.1[..n]);
            self.1.drain(..n);
            Ok(n)
        }
    }

    /// Captures everything the loop writes.
    #[derive(Clone, Default)]
    struct Sink(Arc<Mutex<Vec<u8>>>);

    impl Write for Sink {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Sink {
        fn events(&self) -> Vec<Event> {
            let text = String::from_utf8(self.0.lock().unwrap().clone()).unwrap();
            text.lines().map(|l| from_line(l).unwrap()).collect()
        }

        fn wait_for(&self, pred: impl Fn(&Event) -> bool) -> Event {
            for _ in 0..1000 {
                if let Some(e) = self.events().into_iter().find(&pred) {
                    return e;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            panic!("event never arrived; got {:?}", self.events());
        }
    }

    /// Feed `input`, keep stdin open until `finals` jobs have answered (closing it cancels
    /// whatever is still running), then close it.
    fn run(input: &str, engines: FakeEngines, finals: usize) -> Vec<Event> {
        run_beating(input, engines, finals, HEARTBEAT)
    }

    fn run_beating(input: &str, engines: FakeEngines, finals: usize, heartbeat: Duration) -> Vec<Event> {
        let (tx, rx) = mpsc::channel();
        let sink = Sink::default();
        let out = sink.clone();
        let server =
            std::thread::spawn(move || serve_with(BufReader::new(Pipe(rx, vec![])), out, engines, heartbeat));
        tx.send(input.as_bytes().to_vec()).unwrap();
        for _ in 0..1000 {
            if sink.events().iter().filter(|e| e.is_final()).count() >= finals {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        drop(tx);
        server.join().unwrap();
        sink.events()
    }

    #[test]
    fn announces_itself_then_answers_in_order() {
        let input = transcribe("a", "ok") + &to_line(&Request { id: "b".into(), op: Op::Probe });
        let events = run(&input, FakeEngines::default(), 2);
        assert!(matches!(events[0], Event::Ready { .. }));
        let finals: Vec<_> = events.iter().filter(|e| e.is_final()).collect();
        assert!(matches!(finals[0], Event::Transcribed { id, .. } if id == "a"));
        assert!(matches!(finals[1], Event::Devices { id, .. } if id == "b"));
        assert!(events.iter().any(|e| matches!(e, Event::Progress { id, progress } if id == "a" && *progress == 0.5)));
    }

    #[test]
    fn engine_errors_keep_their_kind() {
        let events = run(&transcribe("a", "oom"), FakeEngines::default(), 1);
        assert!(matches!(
            events.last().unwrap(),
            Event::Failed { id, kind: FailureKind::OutOfMemory, .. } if id == "a"
        ));
    }

    #[test]
    fn a_panic_becomes_an_error_and_the_loop_keeps_going() {
        let unloads = Arc::new(Mutex::new(0));
        let input = transcribe("a", "panic") + &transcribe("b", "ok");
        let events = run(&input, FakeEngines { unloads: unloads.clone() }, 2);
        let finals: Vec<_> = events.iter().filter(|e| e.is_final()).collect();
        assert!(matches!(finals[0], Event::Failed { kind: FailureKind::Engine, error, .. } if error.contains("boom")));
        assert!(matches!(finals[1], Event::Transcribed { .. }));
        assert_eq!(*unloads.lock().unwrap(), 1, "a panic drops the loaded models");
    }

    #[test]
    fn malformed_lines_are_skipped() {
        let input = "garbage\n\n".to_string() + &transcribe("a", "ok");
        let events = run(&input, FakeEngines::default(), 1);
        assert!(matches!(events.last().unwrap(), Event::Transcribed { .. }));
    }

    #[test]
    fn cancel_stops_a_running_job() {
        let (tx, rx) = mpsc::channel();
        let sink = Sink::default();
        let out = sink.clone();
        let server = std::thread::spawn(move || serve(BufReader::new(Pipe(rx, vec![])), out, FakeEngines::default()));

        tx.send(transcribe("slow", "wait").into_bytes()).unwrap();
        std::thread::sleep(Duration::from_millis(50));
        tx.send(to_line(&Request { id: "slow".into(), op: Op::Cancel }).into_bytes()).unwrap();
        let done = sink.wait_for(Event::is_final);
        assert!(matches!(done, Event::Failed { kind: FailureKind::Cancelled, .. }), "{done:?}");

        // Still serving after the cancel.
        tx.send(transcribe("next", "ok").into_bytes()).unwrap();
        sink.wait_for(|e| matches!(e, Event::Transcribed { id, .. } if id == "next"));
        drop(tx);
        server.join().unwrap();
    }

    #[test]
    fn closing_stdin_cancels_the_running_job_and_exits() {
        let (tx, rx) = mpsc::channel();
        let sink = Sink::default();
        let out = sink.clone();
        let server = std::thread::spawn(move || serve(BufReader::new(Pipe(rx, vec![])), out, FakeEngines::default()));
        tx.send(transcribe("slow", "wait").into_bytes()).unwrap();
        std::thread::sleep(Duration::from_millis(50));
        drop(tx);
        server.join().unwrap();
        assert!(matches!(sink.events().last().unwrap(), Event::Failed { kind: FailureKind::Cancelled, .. }));
    }

    #[test]
    fn cancelling_a_queued_job_answers_at_once_and_leaves_the_running_one_alone() {
        let (tx, rx) = mpsc::channel();
        let sink = Sink::default();
        let out = sink.clone();
        let server = std::thread::spawn(move || serve(BufReader::new(Pipe(rx, vec![])), out, FakeEngines::default()));

        tx.send((transcribe("running", "wait") + &transcribe("queued", "ok")).into_bytes()).unwrap();
        std::thread::sleep(Duration::from_millis(50));
        tx.send(to_line(&Request { id: "queued".into(), op: Op::Cancel }).into_bytes()).unwrap();
        let answer = sink.wait_for(|e| e.is_final());
        assert!(
            matches!(&answer, Event::Failed { id, kind: FailureKind::Cancelled, .. } if id == "queued"),
            "{answer:?}"
        );
        // The running job was not touched: it is still going.
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(sink.events().iter().filter(|e| e.is_final()).count(), 1);

        tx.send(to_line(&Request { id: "running".into(), op: Op::Cancel }).into_bytes()).unwrap();
        sink.wait_for(|e| matches!(e, Event::Failed { id, .. } if id == "running"));
        tx.send(transcribe("next", "ok").into_bytes()).unwrap();
        sink.wait_for(|e| matches!(e, Event::Transcribed { id, .. } if id == "next"));
        drop(tx);
        server.join().unwrap();

        // The cancelled job never ran, and nothing answered it twice.
        let events = sink.events();
        assert!(!events.iter().any(|e| matches!(e, Event::Transcribed { id, .. } if id == "queued")));
        assert_eq!(events.iter().filter(|e| e.is_final() && e.id() == Some("queued")).count(), 1);
    }

    #[test]
    fn the_heartbeat_speaks_for_a_working_job() {
        let events = run_beating(&transcribe("long", "busy"), FakeEngines::default(), 1, Duration::from_millis(20));
        let beats = events
            .iter()
            .filter(|e| matches!(e, Event::Progress { id, progress } if id == "long" && *progress == 0.25))
            .count();
        // One real report, then heartbeats repeating it.
        assert!(beats >= 3, "{events:?}");
        assert!(matches!(events.last().unwrap(), Event::Transcribed { id, .. } if id == "long"));
    }

    #[test]
    fn the_heartbeat_stays_quiet_for_a_hung_job() {
        let events = run_beating(&transcribe("stuck", "hung"), FakeEngines::default(), 1, Duration::from_millis(20));
        assert!(!events.iter().any(|e| matches!(e, Event::Progress { .. })), "{events:?}");
        assert!(matches!(events.last().unwrap(), Event::Failed { id, .. } if id == "stuck"));
    }

    #[test]
    fn cancelling_an_unknown_job_is_harmless() {
        let input = to_line(&Request { id: "nope".into(), op: Op::Cancel }) + &transcribe("a", "ok");
        let events = run(&input, FakeEngines::default(), 1);
        assert!(matches!(events.last().unwrap(), Event::Transcribed { .. }));
    }
}
