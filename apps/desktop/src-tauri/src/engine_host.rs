//! Runs the speech engines in a separate process (`talkr-engine`, see `talkr_protocol`).
//!
//! whisper.cpp and sherpa-onnx are native code. When one of them crashes, runs out of memory
//! (Linux's OOM killer) or hits a broken GPU driver, only the engine process dies; the app says
//! what happened and starts a fresh engine for the next job. A job that was on the GPU when the
//! engine died is retried once on the CPU, and the GPU is not used again until the next update
//! or until the user changes the compute setting.
//!
//! Binaries, installed next to the app's executable:
//! - `talkr-engine`: CPU on Windows/Linux; on macOS it also drives Metal.
//! - `talkr-engine-gpu` (Windows/Linux, optional): the Vulkan build. It needs the Vulkan
//!   loader to even start, which is why the CPU build exists alongside it.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use talkr_protocol::{from_line, to_line, Device, DeviceKind, Event, FailureKind, Op, Request};
use crate::error::{AppError, Result};

/// Which engine build to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Cpu,
    Gpu,
}

/// A started engine: its pipes and a handle on the process.
pub struct Launched {
    pub stdin: Box<dyn Write + Send>,
    pub stdout: Box<dyn Read + Send>,
    pub stderr: Option<Box<dyn Read + Send>>,
    pub process: Box<dyn EngineProcess>,
}

pub trait EngineProcess: Send {
    fn kill(&mut self);
    /// How the process ended, or `None` while it is still running.
    fn try_wait(&mut self) -> Option<Exit>;
}

/// How an engine process ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    Code(i64),
    /// Killed by a Unix signal.
    Signal(i32),
    Unknown,
}

/// Starts engine processes. `NativeLauncher` runs the real binaries; tests use fakes.
pub trait Launcher: Send + Sync {
    fn launch(&self, variant: Variant) -> std::io::Result<Launched>;
    /// Whether a separate GPU build is installed.
    fn has_gpu_build(&self) -> bool;
    /// Whether the CPU build can use a GPU itself (macOS: Metal).
    fn cpu_build_has_gpu(&self) -> bool;
}

pub struct NativeLauncher {
    cpu: PathBuf,
    gpu: Option<PathBuf>,
}

impl NativeLauncher {
    /// Find the engines next to the running executable (where the installers and `tauri dev` put
    /// sidecars). `TALKR_ENGINE` / `TALKR_ENGINE_GPU` override the paths.
    pub fn locate() -> Self {
        let dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .unwrap_or_default();
        let exe = |name: &str| dir.join(format!("{}{}", name, std::env::consts::EXE_SUFFIX));
        let cpu = std::env::var_os("TALKR_ENGINE").map(PathBuf::from).unwrap_or_else(|| exe("talkr-engine"));
        let gpu = std::env::var_os("TALKR_ENGINE_GPU")
            .map(PathBuf::from)
            .or_else(|| Some(exe("talkr-engine-gpu")))
            .filter(|p| p.is_file());
        Self { cpu, gpu }
    }
}

impl Launcher for NativeLauncher {
    fn launch(&self, variant: Variant) -> std::io::Result<Launched> {
        let path = match (variant, &self.gpu) {
            (Variant::Gpu, Some(gpu)) => gpu,
            _ => &self.cpu,
        };
        let mut cmd = Command::new(path);
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = cmd.spawn().map_err(|e| {
            std::io::Error::new(e.kind(), format!("could not start {}: {}", path.display(), e))
        })?;
        Ok(Launched {
            stdin: Box::new(child.stdin.take().expect("piped")),
            stdout: Box::new(child.stdout.take().expect("piped")),
            stderr: child.stderr.take().map(|s| Box::new(s) as Box<dyn Read + Send>),
            process: Box::new(child),
        })
    }

    fn has_gpu_build(&self) -> bool {
        self.gpu.is_some()
    }

    fn cpu_build_has_gpu(&self) -> bool {
        cfg!(target_os = "macos")
    }
}

impl EngineProcess for Child {
    fn kill(&mut self) {
        let _ = Child::kill(self);
    }

    fn try_wait(&mut self) -> Option<Exit> {
        match Child::try_wait(self) {
            Ok(Some(status)) => {
                #[cfg(unix)]
                {
                    use std::os::unix::process::ExitStatusExt;
                    if let Some(signal) = status.signal() {
                        return Some(Exit::Signal(signal));
                    }
                }
                // Windows NTSTATUS crash codes arrive as negative i32s; widen them unsigned.
                Some(status.code().map(|c| Exit::Code(c as u32 as i64)).unwrap_or(Exit::Unknown))
            }
            Ok(None) => None,
            Err(_) => Some(Exit::Unknown),
        }
    }
}

/// What the reader thread forwards to a waiting job.
enum Msg {
    Event(Event),
    /// The engine process ended while the job was waiting.
    Died(Exit),
}

/// Jobs waiting for the engine's answer, by request id.
type Pending = Mutex<HashMap<String, flume::Sender<Msg>>>;

struct Worker {
    variant: Variant,
    stdin: Box<dyn Write + Send>,
    process: Arc<Mutex<Box<dyn EngineProcess>>>,
    pending: Arc<Pending>,
    alive: Arc<AtomicBool>,
}

impl Drop for Worker {
    fn drop(&mut self) {
        // Closing stdin asks the engine to stop; the kill covers one that is stuck in native code.
        lock(&self.process).kill();
    }
}

/// Ask for a job to run on the GPU. `Auto` uses a discrete GPU (or Apple Silicon's) when one is
/// present; integrated GPUs are often slower than the CPU for Whisper, so they are opt-in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuPolicy {
    Never,
    Auto,
    Prefer,
}

pub struct EngineHost {
    launcher: Box<dyn Launcher>,
    worker: Mutex<Option<Worker>>,
    gpu_failed: AtomicBool,
    /// Written when the GPU engine dies, so the next launch doesn't try it again.
    gpu_marker: Option<PathBuf>,
    /// GPUs the engine reported, probed once (on the GPU build if there is one).
    devices: Mutex<Option<Vec<Device>>>,
    last_used: Mutex<Instant>,
    busy: AtomicU64,
}

/// A job's final answer: the engine's own event, or a description of how it failed.
pub type Answer = std::result::Result<Event, AppError>;

impl EngineHost {
    pub fn new(launcher: Box<dyn Launcher>, gpu_marker: Option<PathBuf>) -> Self {
        let gpu_failed = gpu_marker
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .is_some_and(|version| version.trim() == env!("CARGO_PKG_VERSION"));
        Self {
            launcher,
            worker: Mutex::new(None),
            gpu_failed: AtomicBool::new(gpu_failed),
            gpu_marker,
            devices: Mutex::new(None),
            last_used: Mutex::new(Instant::now()),
            busy: AtomicU64::new(0),
        }
    }

    /// Whether GPU jobs are possible at all in this install.
    pub fn gpu_available(&self) -> bool {
        (self.launcher.has_gpu_build() || self.launcher.cpu_build_has_gpu()) && !self.gpu_failed.load(Ordering::Relaxed)
    }

    pub fn gpu_failed(&self) -> bool {
        self.gpu_failed.load(Ordering::Relaxed)
    }

    /// Forget an earlier GPU failure (the user changed the compute setting).
    pub fn reset_gpu(&self) {
        self.gpu_failed.store(false, Ordering::Relaxed);
        *lock(&self.devices) = None;
        if let Some(marker) = &self.gpu_marker {
            let _ = std::fs::remove_file(marker);
        }
    }

    /// Decide whether a speech-to-text job should use the GPU.
    pub fn should_use_gpu(&self, policy: GpuPolicy) -> bool {
        if policy == GpuPolicy::Never || !self.gpu_available() {
            return false;
        }
        if policy == GpuPolicy::Prefer {
            return true;
        }
        if self.launcher.cpu_build_has_gpu() {
            // macOS: Metal is worth it on Apple Silicon; Intel Macs' GPUs rarely beat the CPU.
            return cfg!(target_arch = "aarch64");
        }
        self.devices().iter().any(|d| d.kind == DeviceKind::Gpu)
    }

    /// The compute devices the engine can use (empty if it could not be asked).
    pub fn devices(&self) -> Vec<Device> {
        if let Some(devices) = lock(&self.devices).clone() {
            return devices;
        }
        let probe_on_gpu = self.gpu_available();
        let devices = match self.run_once("probe", &|_| Op::Probe, probe_on_gpu, &|_| {}) {
            Ok(Event::Devices { devices, .. }) => devices,
            _ => Vec::new(),
        };
        *lock(&self.devices) = Some(devices.clone());
        devices
    }

    /// Run a request, reporting progress. `make_op(gpu)` builds the request for a GPU or CPU run,
    /// so a GPU job can be retried on the CPU after the GPU engine died.
    pub fn run(&self, id: &str, make_op: &dyn Fn(bool) -> Op, gpu: bool, progress: &dyn Fn(f32)) -> Answer {
        self.busy.fetch_add(1, Ordering::Relaxed);
        let answer = self.run_with_fallback(id, make_op, gpu, progress);
        self.busy.fetch_sub(1, Ordering::Relaxed);
        *lock(&self.last_used) = Instant::now();
        answer
    }

    fn run_with_fallback(&self, id: &str, make_op: &dyn Fn(bool) -> Op, gpu: bool, progress: &dyn Fn(f32)) -> Answer {
        let gpu = gpu && self.gpu_available();
        match self.run_once(id, make_op, gpu, progress) {
            Err(AppError::EngineDied(exit)) if gpu => {
                log::warn!("GPU engine died ({}); retrying on the CPU and disabling the GPU", describe(exit));
                self.mark_gpu_failed();
                self.run_once(id, make_op, false, progress).map_err(|e| self.explain(e))
            }
            other => other.map_err(|e| self.explain(e)),
        }
    }

    fn run_once(&self, id: &str, make_op: &dyn Fn(bool) -> Op, gpu: bool, progress: &dyn Fn(f32)) -> Answer {
        let variant = if gpu && self.launcher.has_gpu_build() { Variant::Gpu } else { Variant::Cpu };
        let rx = {
            let mut slot = lock(&self.worker);
            let reusable = slot.as_ref().is_some_and(|w| {
                w.alive.load(Ordering::Relaxed) && (w.variant == variant || (variant == Variant::Cpu && !gpu))
            });
            if !reusable {
                // Drop (and kill) the old engine before starting another, so two models are
                // never resident at once.
                *slot = None;
                *slot = Some(self.spawn(variant)?);
            }
            let worker = slot.as_mut().expect("just ensured");
            let (tx, rx) = flume::unbounded();
            lock(&worker.pending).insert(id.to_string(), tx);
            let request = Request { id: id.to_string(), op: make_op(gpu) };
            if worker.stdin.write_all(to_line(&request).as_bytes()).and_then(|_| worker.stdin.flush()).is_err() {
                // The engine is gone; the reader thread reports how it ended.
                log::warn!("engine stdin closed while sending {}", id);
            }
            rx
        };

        loop {
            match rx.recv() {
                Ok(Msg::Event(Event::Progress { progress: p, .. })) => progress(p),
                Ok(Msg::Event(event)) => return Ok(event),
                Ok(Msg::Died(exit)) => return Err(AppError::EngineDied(exit)),
                Err(_) => return Err(AppError::EngineDied(Exit::Unknown)),
            }
        }
    }

    /// Ask the engine to stop job `id`. It answers the job with a `Cancelled` failure.
    pub fn cancel(&self, id: &str) {
        let mut slot = lock(&self.worker);
        if let Some(worker) = slot.as_mut() {
            let line = to_line(&Request { id: id.to_string(), op: Op::Cancel });
            let _ = worker.stdin.write_all(line.as_bytes()).and_then(|_| worker.stdin.flush());
        }
    }

    /// Stop the engine process, freeing all model memory. The next job starts a new one.
    pub fn shutdown(&self) {
        *lock(&self.worker) = None;
    }

    /// Stop the engine if nothing has used it for `idle`. Returns whether it was stopped.
    pub fn stop_if_idle(&self, idle: Duration) -> bool {
        if self.busy.load(Ordering::Relaxed) > 0 || lock(&self.last_used).elapsed() < idle {
            return false;
        }
        let mut slot = lock(&self.worker);
        if slot.is_some() {
            log::info!("stopping the idle engine to free memory");
            *slot = None;
            return true;
        }
        false
    }

    fn mark_gpu_failed(&self) {
        self.gpu_failed.store(true, Ordering::Relaxed);
        *lock(&self.devices) = None;
        if let Some(marker) = &self.gpu_marker {
            let _ = std::fs::write(marker, env!("CARGO_PKG_VERSION"));
        }
    }

    /// Turn a dead engine into a message a person can act on.
    fn explain(&self, e: AppError) -> AppError {
        match e {
            AppError::EngineDied(exit) if is_out_of_memory(exit) => AppError::Engine(
                "The speech engine ran out of memory. Close other apps, or pick a smaller or compressed model in Models."
                    .into(),
            ),
            AppError::EngineDied(exit) => AppError::Engine(format!(
                "The speech engine stopped unexpectedly ({}). It has been restarted, so please try again. \
                 If it keeps happening, try a smaller model.",
                describe(exit)
            )),
            other => other,
        }
    }

    fn spawn(&self, variant: Variant) -> Result<Worker> {
        let launched = self.launcher.launch(variant).map_err(|e| {
            if variant == Variant::Gpu {
                // Treat a GPU build that can't start (no Vulkan loader) like one that crashed.
                AppError::EngineDied(Exit::Unknown)
            } else {
                AppError::Engine(format!("The speech engine could not start: {}", e))
            }
        })?;
        log::info!("started the {:?} engine", variant);
        let pending: Arc<Pending> = Arc::default();
        let alive = Arc::new(AtomicBool::new(true));
        let process = Arc::new(Mutex::new(launched.process));

        if let Some(stderr) = launched.stderr {
            std::thread::Builder::new()
                .name("talkr-engine-log".into())
                .spawn(move || {
                    for line in BufReader::new(stderr).lines().map_while(|l| l.ok()) {
                        log::info!(target: "talkr_engine", "{}", line);
                    }
                })
                .map_err(AppError::Io)?;
        }

        // The reader delivers events. Its end of stream is not a reliable sign that the engine
        // is gone: on Windows, a process started from this app at the wrong moment (WebView2
        // starts its helpers from inside it) can inherit a copy of the engine's stdout, so the
        // pipe never closes. The watcher checks the process itself instead.
        let (reader_done_tx, reader_done) = flume::bounded::<()>(0);
        {
            let pending = pending.clone();
            let stdout = launched.stdout;
            std::thread::Builder::new()
                .name("talkr-engine-reader".into())
                .spawn(move || {
                    read_events(stdout, &pending);
                    drop(reader_done_tx);
                })
                .map_err(AppError::Io)?;
        }
        {
            let pending = pending.clone();
            let alive = alive.clone();
            let process = process.clone();
            std::thread::Builder::new()
                .name("talkr-engine-watch".into())
                .spawn(move || watch_process(&process, &pending, &alive, &reader_done))
                .map_err(AppError::Io)?;
        }

        Ok(Worker { variant, stdin: launched.stdin, process, pending, alive })
    }
}

fn read_events(stdout: Box<dyn Read + Send>, pending: &Pending) {
    for line in BufReader::new(stdout).lines() {
        let Ok(line) = line else { break };
        let event: Event = match from_line(&line) {
            Ok(e) => e,
            Err(e) => {
                log::warn!("engine sent something unreadable ({}): {}", e, line);
                continue;
            }
        };
        let Some(id) = event.id().map(str::to_string) else { continue };
        let mut pending = lock(pending);
        let tx = if event.is_final() { pending.remove(&id) } else { pending.get(&id).cloned() };
        if let Some(tx) = tx {
            let _ = tx.send(Msg::Event(event));
        }
    }
}

/// Wait for the engine process to end, then fail whatever was still waiting on it.
fn watch_process(
    process: &Mutex<Box<dyn EngineProcess>>,
    pending: &Pending,
    alive: &AtomicBool,
    reader_done: &flume::Receiver<()>,
) {
    let exit = loop {
        // Polled rather than a blocking wait, so `kill` can take the lock at any time.
        if let Some(exit) = lock(process).try_wait() {
            break exit;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    alive.store(false, Ordering::Relaxed);
    // Let the reader hand over anything the engine wrote just before it exited.
    let _ = reader_done.recv_timeout(Duration::from_secs(1));
    if exit != Exit::Code(0) {
        log::error!("engine process ended: {}", describe(exit));
    }
    for (_, tx) in lock(pending).drain() {
        let _ = tx.send(Msg::Died(exit));
    }
}

fn lock<T: ?Sized>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Whether an engine exit looks like the system ran out of memory.
pub fn is_out_of_memory(exit: Exit) -> bool {
    match exit {
        // Linux's OOM killer sends SIGKILL.
        Exit::Signal(9) => true,
        // STATUS_NO_MEMORY, STATUS_COMMITMENT_LIMIT
        Exit::Code(0xC000_0017) | Exit::Code(0xC000_012D) => true,
        _ => false,
    }
}

pub fn describe(exit: Exit) -> String {
    match exit {
        Exit::Code(0) => "it exited".into(),
        Exit::Code(0xC000_0005) => "memory access violation".into(),
        Exit::Code(0xC000_001D) => "the processor does not support an instruction it needs".into(),
        Exit::Code(0xC000_00FD) => "stack overflow".into(),
        Exit::Code(0xC000_0409) | Exit::Code(3) => "it aborted".into(),
        Exit::Code(0xC000_0135) | Exit::Code(0xC000_007B) => "a library it needs is missing".into(),
        Exit::Code(0xC000_0017) | Exit::Code(0xC000_012D) => "out of memory".into(),
        Exit::Code(code) if code > 0xFFFF => format!("crash code 0x{:X}", code),
        Exit::Code(code) => format!("exit code {}", code),
        Exit::Signal(9) => "killed by the system, usually for lack of memory".into(),
        Exit::Signal(4) => "the processor does not support an instruction it needs".into(),
        Exit::Signal(6) => "it aborted".into(),
        Exit::Signal(11) => "memory access violation".into(),
        Exit::Signal(s) => format!("signal {}", s),
        Exit::Unknown => "unknown reason".into(),
    }
}

/// Map an engine's failure event to an app error.
pub fn failure_to_error(kind: FailureKind, error: String) -> AppError {
    match kind {
        FailureKind::Cancelled => AppError::Cancelled,
        FailureKind::Invalid => AppError::Validation(error),
        FailureKind::OutOfMemory => AppError::Engine(format!(
            "{}. Close other apps, or pick a smaller or compressed model in Models.",
            error.trim_end_matches('.')
        )),
        FailureKind::Engine => AppError::Engine(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{PipeReader, PipeWriter};
    use std::sync::atomic::AtomicUsize;
    use talkr_protocol::{ModelRef, Transcript, TranscribeJob};

    /// How a fake engine behaves for each request.
    #[derive(Clone, Copy)]
    enum Script {
        /// Answer everything, emitting one progress event per job.
        Healthy,
        /// Die with this exit as soon as a transcription arrives.
        DieOnTranscribe(Exit),
        /// Fail to start at all.
        WontStart,
        /// Die on a transcription, but leave the event pipe open, as when another process
        /// inherited the engine's stdout (seen on Windows with WebView2).
        DieKeepingPipeOpen,
    }

    struct FakeProcess {
        exit: Arc<Mutex<Option<Exit>>>,
        done: flume::Receiver<()>,
        killed: Arc<AtomicBool>,
    }

    impl EngineProcess for FakeProcess {
        fn kill(&mut self) {
            self.killed.store(true, Ordering::Relaxed);
        }
        fn try_wait(&mut self) -> Option<Exit> {
            if self.killed.load(Ordering::Relaxed) {
                return Some(Exit::Code(1));
            }
            match self.done.try_recv() {
                Err(flume::TryRecvError::Empty) => None,
                _ => Some(lock(&self.exit).unwrap_or(Exit::Code(0))),
            }
        }
    }

    struct FakeLauncher {
        cpu: Script,
        gpu: Option<Script>,
        launches: Arc<Mutex<Vec<Variant>>>,
        requests: Arc<Mutex<Vec<Request>>>,
    }

    impl FakeLauncher {
        fn new(cpu: Script, gpu: Option<Script>) -> Self {
            Self { cpu, gpu, launches: Arc::default(), requests: Arc::default() }
        }
    }

    impl Launcher for FakeLauncher {
        fn launch(&self, variant: Variant) -> std::io::Result<Launched> {
            let script = match variant {
                Variant::Gpu => self.gpu.expect("no GPU build"),
                Variant::Cpu => self.cpu,
            };
            if let Script::WontStart = script {
                return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "vulkan-1.dll missing"));
            }
            lock(&self.launches).push(variant);
            let (req_r, req_w): (PipeReader, PipeWriter) = std::io::pipe()?;
            let (ev_r, mut ev_w) = std::io::pipe()?;
            let exit = Arc::new(Mutex::new(None));
            let (done_tx, done_rx) = flume::bounded(1);
            let requests = self.requests.clone();
            let thread_exit = exit.clone();
            std::thread::spawn(move || {
                let _done = done_tx;
                let _ = ev_w.write_all(to_line(&Event::Ready { version: "test".into() }).as_bytes());
                for line in BufReader::new(req_r).lines() {
                    let Ok(line) = line else { break };
                    let request: Request = from_line(&line).unwrap();
                    lock(&requests).push(request.clone());
                    let id = request.id.clone();
                    let reply = match (&request.op, script) {
                        (Op::Transcribe(_), Script::DieOnTranscribe(e)) => {
                            *lock(&thread_exit) = Some(e);
                            return; // drops the pipes: the process "died"
                        }
                        (Op::Transcribe(_), Script::DieKeepingPipeOpen) => {
                            *lock(&thread_exit) = Some(Exit::Code(0xC000_0005));
                            std::mem::forget(ev_w); // someone else still holds the pipe
                            return;
                        }
                        (Op::Transcribe(job), _) => {
                            let _ = ev_w.write_all(to_line(&Event::Progress { id: id.clone(), progress: 0.5 }).as_bytes());
                            Event::Transcribed {
                                id,
                                transcript: Transcript { text: "hi".into(), segments: vec![], language: None },
                                audio_ms: 1,
                                device: if job.model.gpu { "gpu".into() } else { "cpu".into() },
                            }
                        }
                        (Op::Probe, _) => Event::Devices {
                            id,
                            devices: vec![Device {
                                kind: DeviceKind::Gpu,
                                name: "Vulkan0".into(),
                                description: "Test GPU".into(),
                                memory_free: 8 << 30,
                                memory_total: 8 << 30,
                            }],
                        },
                        (Op::Cancel, _) => continue,
                        _ => Event::Unloaded { id },
                    };
                    let _ = ev_w.write_all(to_line(&reply).as_bytes());
                }
            });
            Ok(Launched {
                stdin: Box::new(req_w),
                stdout: Box::new(ev_r),
                stderr: None,
                process: Box::new(FakeProcess { exit, done: done_rx, killed: Arc::default() }),
            })
        }

        fn has_gpu_build(&self) -> bool {
            self.gpu.is_some()
        }

        fn cpu_build_has_gpu(&self) -> bool {
            false
        }
    }

    fn transcribe(gpu: bool) -> Op {
        Op::Transcribe(TranscribeJob {
            model: ModelRef { model_id: "m".into(), dir: "d".into(), threads: 1, gpu },
            audio_path: "a.wav".into(),
            language: None,
            translate: false,
        })
    }

    type Log<T> = Arc<Mutex<Vec<T>>>;

    fn make_host(launcher: FakeLauncher) -> (EngineHost, Log<Variant>, Log<Request>) {
        let launches = launcher.launches.clone();
        let requests = launcher.requests.clone();
        (EngineHost::new(Box::new(launcher), None), launches, requests)
    }

    #[test]
    fn runs_a_job_and_reports_progress() {
        let (host, launches, _) = make_host(FakeLauncher::new(Script::Healthy, None));
        let seen = AtomicUsize::new(0);
        let answer = host.run("j1", &transcribe, false, &|p| {
            assert_eq!(p, 0.5);
            seen.fetch_add(1, Ordering::Relaxed);
        });
        assert!(matches!(answer, Ok(Event::Transcribed { ref device, .. }) if device == "cpu"), "{answer:?}");
        assert_eq!(seen.load(Ordering::Relaxed), 1);
        // The engine is reused for the next job.
        host.run("j2", &transcribe, false, &|_| {}).unwrap();
        assert_eq!(*lock(&launches), vec![Variant::Cpu]);
    }

    #[test]
    fn a_crash_becomes_a_message_and_the_next_job_gets_a_fresh_engine() {
        let (host, launches, _) = make_host(FakeLauncher::new(Script::DieOnTranscribe(Exit::Code(0xC000_0005)), None));
        let err = host.run("j1", &transcribe, false, &|_| {}).unwrap_err().to_string();
        assert!(err.contains("stopped unexpectedly") && err.contains("memory access violation"), "{err}");
        let _ = host.run("j2", &transcribe, false, &|_| {});
        assert_eq!(*lock(&launches), vec![Variant::Cpu, Variant::Cpu], "restarted after the crash");
    }

    #[test]
    fn a_dead_engine_is_noticed_even_if_its_pipe_never_closes() {
        let (host, _, _) = make_host(FakeLauncher::new(Script::DieKeepingPipeOpen, None));
        let started = Instant::now();
        let err = host.run("j1", &transcribe, false, &|_| {}).unwrap_err().to_string();
        assert!(err.contains("stopped unexpectedly"), "{err}");
        assert!(started.elapsed() < Duration::from_secs(5), "took {:?}", started.elapsed());
    }

    #[test]
    fn out_of_memory_kills_are_explained_as_such() {
        let (host, _, _) = make_host(FakeLauncher::new(Script::DieOnTranscribe(Exit::Signal(9)), None));
        let err = host.run("j1", &transcribe, false, &|_| {}).unwrap_err().to_string();
        assert!(err.contains("ran out of memory"), "{err}");
    }

    #[test]
    fn a_gpu_crash_retries_on_the_cpu_and_disables_the_gpu() {
        let marker = tempfile::NamedTempFile::new().unwrap();
        let launcher = FakeLauncher::new(Script::Healthy, Some(Script::DieOnTranscribe(Exit::Code(0xC000_0005))));
        let launches = launcher.launches.clone();
        let requests = launcher.requests.clone();
        let host = EngineHost::new(Box::new(launcher), Some(marker.path().to_path_buf()));
        assert!(host.gpu_available());

        let answer = host.run("j1", &transcribe, true, &|_| {});
        assert!(matches!(answer, Ok(Event::Transcribed { ref device, .. }) if device == "cpu"), "{answer:?}");
        assert_eq!(*lock(&launches), vec![Variant::Gpu, Variant::Cpu]);
        // The retry asked for the CPU explicitly.
        let last = lock(&requests).last().cloned().unwrap();
        assert!(matches!(last.op, Op::Transcribe(ref j) if !j.model.gpu));

        assert!(host.gpu_failed() && !host.gpu_available());
        assert_eq!(std::fs::read_to_string(marker.path()).unwrap(), env!("CARGO_PKG_VERSION"));
        // A new host (next launch) remembers; resetting forgets.
        let again = EngineHost::new(Box::new(FakeLauncher::new(Script::Healthy, Some(Script::Healthy))), Some(marker.path().to_path_buf()));
        assert!(again.gpu_failed());
        again.reset_gpu();
        assert!(!again.gpu_failed() && again.gpu_available());
    }

    #[test]
    fn a_gpu_build_that_cannot_start_falls_back_to_the_cpu() {
        let (host, launches, _) = make_host(FakeLauncher::new(Script::Healthy, Some(Script::WontStart)));
        let answer = host.run("j1", &transcribe, true, &|_| {});
        assert!(matches!(answer, Ok(Event::Transcribed { .. })), "{answer:?}");
        assert_eq!(*lock(&launches), vec![Variant::Cpu]);
        assert!(host.gpu_failed());
    }

    #[test]
    fn auto_policy_uses_a_discrete_gpu_when_the_engine_reports_one() {
        let (host, _, _) = make_host(FakeLauncher::new(Script::Healthy, Some(Script::Healthy)));
        assert!(host.should_use_gpu(GpuPolicy::Auto));
        assert!(!host.should_use_gpu(GpuPolicy::Never));
        let (cpu_only, _, _) = make_host(FakeLauncher::new(Script::Healthy, None));
        assert!(!cpu_only.should_use_gpu(GpuPolicy::Prefer));
    }

    #[test]
    fn idle_engines_are_stopped_but_busy_ones_are_not() {
        let (host, launches, _) = make_host(FakeLauncher::new(Script::Healthy, None));
        host.run("j1", &transcribe, false, &|_| {}).unwrap();
        assert!(!host.stop_if_idle(Duration::from_secs(3600)));
        assert!(host.stop_if_idle(Duration::ZERO));
        host.run("j2", &transcribe, false, &|_| {}).unwrap();
        assert_eq!(lock(&launches).len(), 2, "a new engine after the idle stop");
    }

    #[test]
    fn failures_map_to_friendly_errors() {
        assert!(matches!(failure_to_error(FailureKind::Cancelled, "x".into()), AppError::Cancelled));
        assert!(matches!(failure_to_error(FailureKind::Invalid, "bad".into()), AppError::Validation(_)));
        let oom = failure_to_error(FailureKind::OutOfMemory, "Not enough memory to load m.".into()).to_string();
        assert!(oom.contains("compressed model") && !oom.contains(".."), "{oom}");
    }

    #[test]
    fn exit_descriptions() {
        assert!(is_out_of_memory(Exit::Signal(9)));
        assert!(!is_out_of_memory(Exit::Code(0xC000_0005)));
        assert_eq!(describe(Exit::Code(0xC000_001D)), "the processor does not support an instruction it needs");
        assert_eq!(describe(Exit::Code(1)), "exit code 1");
    }
}
