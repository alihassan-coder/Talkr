//! Runs the speech engines in a separate process (`talkr-engine`, see `talkr_protocol`).
//!
//! whisper.cpp and sherpa-onnx are native code. When one of them crashes, runs out of memory
//! (Linux's OOM killer) or hits a broken GPU driver, only the engine process dies; the app says
//! what happened and starts a fresh engine for the next job. A job that was on the GPU when the
//! engine died is retried once on the CPU, and the GPU is not used again until the next update
//! or until the user changes the compute setting.
//!
//! The app also stops the engine itself: when it sits idle, when the app exits, when a model it
//! may hold open is deleted, and when it hangs. Jobs caught by such a stop end as cancelled; they
//! are never reported as a crash and never count against the GPU.
//!
//! Binaries, installed next to the app's executable:
//! - `talkr-engine`: CPU on Windows/Linux; on macOS it also drives Metal.
//! - `talkr-engine-gpu` (Windows/Linux, optional): the Vulkan build. It needs the Vulkan
//!   loader to even start, which is why the CPU build exists alongside it.

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use talkr_protocol::{from_line, to_line, Device, DeviceKind, Event, FailureKind, Op, Request};
use uuid::Uuid;
use crate::error::{AppError, Result};

/// How long the engine gets to answer a cancelled job before it is killed. Native code can be
/// stuck somewhere that never checks the cancel flag.
pub const CANCEL_GRACE: Duration = Duration::from_secs(10);
/// How long a job may wait without hearing anything from the engine (no progress, no answer)
/// before the engine is considered hung and restarted. Model loading and long jobs both report
/// progress well within this.
pub const STALL_LIMIT: Duration = Duration::from_secs(10 * 60);
/// How long a job that needs the other engine build waits for the running one to finish its
/// jobs before replacing it anyway.
pub const SWITCH_WAIT: Duration = Duration::from_secs(10 * 60);
/// How long to wait for a stopped engine to actually exit (and let go of its files).
pub const EXIT_WAIT: Duration = Duration::from_secs(5);
/// Cancellations of jobs that never reached the engine are forgotten after this.
const CANCEL_MEMORY: Duration = Duration::from_secs(60 * 60);

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
    /// sidecars). In debug builds `TALKR_ENGINE` / `TALKR_ENGINE_GPU` override the paths; a
    /// release build only ever runs the engines it was installed with.
    pub fn locate() -> Self {
        let dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .unwrap_or_default();
        let exe = |name: &str| dir.join(format!("{}{}", name, std::env::consts::EXE_SUFFIX));
        let dev_override =
            |var: &str| if cfg!(debug_assertions) { std::env::var_os(var).map(PathBuf::from) } else { None };
        let cpu = dev_override("TALKR_ENGINE").unwrap_or_else(|| exe("talkr-engine"));
        let gpu = dev_override("TALKR_ENGINE_GPU")
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
    /// The app stopped the engine on purpose while the job was waiting.
    Stopped,
}

/// Jobs waiting for the engine's answer, by request id.
type Pending = Mutex<HashMap<String, flume::Sender<Msg>>>;

/// What one engine process's threads and the jobs waiting on it share.
struct Shared {
    process: Mutex<Box<dyn EngineProcess>>,
    pending: Pending,
    alive: AtomicBool,
    /// Set before the app kills the engine on purpose, so its jobs end as cancelled instead of
    /// being reported as a crash (a kill looks like the OOM killer's SIGKILL on Unix).
    stopping: AtomicBool,
    /// When the engine last said anything, for spotting a hung engine.
    last_event: Mutex<Instant>,
    /// The model the engine keeps loaded: the one its last successful job used.
    resident: Mutex<Option<String>>,
    /// Every model a request to this engine named. It may hold any of these files open.
    touched: Mutex<HashSet<String>>,
}

impl Shared {
    /// Running, and not on its way out: new jobs may be sent to it.
    fn usable(&self) -> bool {
        self.alive.load(Ordering::Relaxed) && !self.stopping.load(Ordering::SeqCst)
    }

    /// Kill the engine on purpose.
    fn stop(&self) {
        self.stopping.store(true, Ordering::SeqCst);
        lock(&self.process).kill();
    }

    /// Keep track of what the engine holds loaded, from a job's final answer.
    fn note_answer(&self, model: Option<&str>, event: &Event) {
        let mut resident = lock(&self.resident);
        match event {
            Event::Transcribed { .. } | Event::Synthesized { .. } | Event::Voices { .. } => {
                *resident = model.map(str::to_string);
            }
            Event::Unloaded { .. } => *resident = None,
            // Switching models frees the old one before loading the new; if that load (or the
            // job) failed, there is no telling what is left, so assume nothing.
            Event::Failed { .. } if model.is_some() && resident.as_deref() != model => *resident = None,
            _ => {}
        }
    }
}

struct Worker {
    variant: Variant,
    stdin: Box<dyn Write + Send>,
    shared: Arc<Shared>,
}

impl Drop for Worker {
    fn drop(&mut self) {
        // Closing stdin asks the engine to stop; the kill covers one that is stuck in native code.
        // Either way it is deliberate, so jobs still waiting on it end as cancelled.
        self.shared.stop();
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

/// Time limits, a field so tests can shorten them.
#[derive(Debug, Clone, Copy)]
struct Limits {
    cancel_grace: Duration,
    stall: Duration,
    switch_wait: Duration,
    exit_wait: Duration,
    /// How often a waiting job checks the limits above.
    poll: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            cancel_grace: CANCEL_GRACE,
            stall: STALL_LIMIT,
            switch_wait: SWITCH_WAIT,
            exit_wait: EXIT_WAIT,
            poll: Duration::from_millis(250),
        }
    }
}

pub struct EngineHost {
    launcher: Box<dyn Launcher>,
    worker: Mutex<Option<Worker>>,
    /// Signalled (with `worker`'s mutex) whenever a job's answer arrives or an engine ends, for
    /// jobs waiting to switch engine builds.
    idle: Arc<Condvar>,
    gpu_failed: AtomicBool,
    /// Written when the GPU engine dies, so the next launch doesn't try it again.
    gpu_marker: Option<PathBuf>,
    /// GPUs the engine reported, probed once (on the GPU build if there is one).
    devices: Mutex<Option<Vec<Device>>>,
    /// Held while probing, so concurrent callers share one probe.
    probing: Mutex<()>,
    last_used: Mutex<Instant>,
    /// Jobs (and probes) in `run`, counted under `worker`'s lock.
    busy: AtomicU64,
    /// Models used by jobs in `run`, one entry per job, changed under `worker`'s lock.
    active_models: Mutex<Vec<String>>,
    /// Jobs the user cancelled, and when.
    cancels: Mutex<HashMap<String, Instant>>,
    limits: Limits,
}

/// A job's final answer: the engine's own event, or a description of how it failed.
pub type Answer = std::result::Result<Event, AppError>;

/// The model a request names, if any.
fn op_model(op: &Op) -> Option<&str> {
    match op {
        Op::Transcribe(job) => Some(&job.model.model_id),
        Op::Synthesize(job) => Some(&job.model.model_id),
        Op::Voices(model) => Some(&model.model_id),
        Op::Probe | Op::Cancel | Op::Unload => None,
    }
}

impl EngineHost {
    pub fn new(launcher: Box<dyn Launcher>, gpu_marker: Option<PathBuf>) -> Self {
        let gpu_failed = gpu_marker
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .is_some_and(|version| version.trim() == env!("CARGO_PKG_VERSION"));
        Self {
            launcher,
            worker: Mutex::new(None),
            idle: Arc::new(Condvar::new()),
            gpu_failed: AtomicBool::new(gpu_failed),
            gpu_marker,
            devices: Mutex::new(None),
            probing: Mutex::new(()),
            last_used: Mutex::new(Instant::now()),
            busy: AtomicU64::new(0),
            active_models: Mutex::new(Vec::new()),
            cancels: Mutex::new(HashMap::new()),
            limits: Limits::default(),
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
        // One probe at a time: callers that arrive meanwhile wait for its result instead of
        // starting their own.
        let _probing = lock(&self.probing);
        if let Some(devices) = lock(&self.devices).clone() {
            return devices;
        }
        self.enter(None);
        let probe = |gpu: bool| {
            let id = format!("probe-{}", Uuid::new_v4());
            self.run_once(&id, &|_| Op::Probe, gpu, &|_| {})
        };
        let probe_on_gpu = self.gpu_available();
        let mut answer = probe(probe_on_gpu);
        if probe_on_gpu && matches!(answer, Err(AppError::EngineDied(_))) {
            // The GPU engine can't start or crashed on start-up (no Vulkan loader, bad driver):
            // remember that, and still report what the CPU engine can do.
            self.mark_gpu_failed();
            answer = probe(false);
        }
        self.leave(None);
        let devices = match answer {
            Ok(Event::Devices { devices, .. }) => devices,
            // The engine was stopped under the probe (app exit, a hang elsewhere): ask again next time.
            Err(AppError::Cancelled) => return Vec::new(),
            _ => Vec::new(),
        };
        *lock(&self.devices) = Some(devices.clone());
        devices
    }

    /// The model the running engine keeps loaded (the last one a job used successfully), or
    /// `None` when no engine is running.
    pub fn resident_model(&self) -> Option<String> {
        let slot = lock(&self.worker);
        slot.as_ref().filter(|w| w.shared.usable()).and_then(|w| lock(&w.shared.resident).clone())
    }

    /// Run a request, reporting progress. `make_op(gpu)` builds the request for a GPU or CPU run,
    /// so a GPU job can be retried on the CPU after the GPU engine died.
    pub fn run(&self, id: &str, make_op: &dyn Fn(bool) -> Op, gpu: bool, progress: &dyn Fn(f32)) -> Answer {
        let model = op_model(&make_op(gpu)).map(str::to_string);
        self.enter(model.as_deref());
        let answer = self.run_with_fallback(id, make_op, gpu, progress);
        lock(&self.cancels).remove(id);
        self.leave(model.as_deref());
        answer
    }

    /// Count a job in. Under the worker lock, so `stop_if_idle` and `with_model_released` (which
    /// hold it while they decide) see a job either as running or as not yet started.
    fn enter(&self, model: Option<&str>) {
        let _slot = lock(&self.worker);
        self.busy.fetch_add(1, Ordering::SeqCst);
        if let Some(model) = model {
            lock(&self.active_models).push(model.to_string());
        }
    }

    fn leave(&self, model: Option<&str>) {
        *lock(&self.last_used) = Instant::now();
        if let Some(model) = model {
            let mut active = lock(&self.active_models);
            if let Some(i) = active.iter().position(|m| m == model) {
                active.swap_remove(i);
            }
        }
        self.busy.fetch_sub(1, Ordering::SeqCst);
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
        let op = make_op(gpu);
        let model = op_model(&op).map(str::to_string);
        let (rx, shared) = {
            let mut slot = lock(&self.worker);
            let switch_deadline = Instant::now() + self.limits.switch_wait;
            loop {
                // Cancelled before it reached the engine (`cancel` holds this lock too, so a
                // cancellation lands either here or after the request was sent).
                if lock(&self.cancels).contains_key(id) {
                    return Err(AppError::Cancelled);
                }
                let reusable = slot.as_ref().is_some_and(|w| {
                    w.shared.usable() && (w.variant == variant || (variant == Variant::Cpu && !gpu))
                });
                if reusable {
                    break;
                }
                // Replacing an engine kills its jobs: let the other build finish what it is doing.
                let busy = slot
                    .as_ref()
                    .is_some_and(|w| w.shared.usable() && !lock(&w.shared.pending).is_empty());
                if busy && Instant::now() < switch_deadline {
                    slot = self.idle.wait_timeout(slot, self.limits.poll).unwrap_or_else(|e| e.into_inner()).0;
                    continue;
                }
                if busy {
                    log::warn!("the {:?} engine is still busy; replacing it anyway", slot.as_ref().map(|w| w.variant));
                }
                // Drop (and kill) the old engine before starting another, so two models are
                // never resident at once.
                *slot = None;
                *slot = Some(self.spawn(variant)?);
                break;
            }
            let worker = slot.as_mut().expect("just ensured");
            let (tx, rx) = flume::unbounded();
            if lock(&worker.shared.pending).insert(id.to_string(), tx).is_some() {
                // Two jobs under one id would steal each other's answers.
                log::error!("engine request id {} was already waiting for an answer", id);
                debug_assert!(false, "duplicate engine request id {id}");
            }
            if let Some(model) = &model {
                lock(&worker.shared.touched).insert(model.clone());
            }
            let request = Request { id: id.to_string(), op };
            if worker.stdin.write_all(to_line(&request).as_bytes()).and_then(|_| worker.stdin.flush()).is_err() {
                // The engine is gone; the reader thread reports how it ended.
                log::warn!("engine stdin closed while sending {}", id);
            }
            (rx, worker.shared.clone())
        };

        let waiting_since = Instant::now();
        loop {
            match rx.recv_timeout(self.limits.poll) {
                Ok(Msg::Event(Event::Progress { progress: p, .. })) => progress(p),
                Ok(Msg::Event(event)) => {
                    shared.note_answer(model.as_deref(), &event);
                    return Ok(event);
                }
                Ok(Msg::Died(exit)) => return Err(AppError::EngineDied(exit)),
                Ok(Msg::Stopped) => return Err(AppError::Cancelled),
                Err(flume::RecvTimeoutError::Disconnected) => {
                    return Err(AppError::Engine("The speech engine lost track of this job. Please try again.".into()))
                }
                Err(flume::RecvTimeoutError::Timeout) => {
                    let cancelled_at = lock(&self.cancels).get(id).copied();
                    if cancelled_at.is_some_and(|at| at.elapsed() >= self.limits.cancel_grace) {
                        log::warn!("the engine did not stop job {} when asked; restarting it", id);
                        self.abandon(&shared, id);
                        return Err(AppError::Cancelled);
                    }
                    // Quiet since this job was sent, and since anything else the engine said.
                    let heard = (*lock(&shared.last_event)).max(waiting_since);
                    if heard.elapsed() >= self.limits.stall {
                        log::error!("the engine stopped responding during job {}; restarting it", id);
                        self.abandon(&shared, id);
                        return Err(AppError::Engine(
                            "The speech engine stopped responding and has been restarted. Please try again. \
                             If it keeps happening, try a smaller model."
                                .into(),
                        ));
                    }
                }
            }
        }
    }

    /// Give up on job `id` and kill its engine on purpose; its other jobs end as cancelled.
    fn abandon(&self, shared: &Shared, id: &str) {
        lock(&shared.pending).remove(id);
        shared.stop();
        self.idle.notify_all();
    }

    /// Ask the engine to stop job `id`. It answers the job with a `Cancelled` failure; if it has
    /// not within `CANCEL_GRACE`, the engine is killed. A job that has not reached the engine yet
    /// is never sent.
    pub fn cancel(&self, id: &str) {
        let mut slot = lock(&self.worker);
        {
            let mut cancels = lock(&self.cancels);
            cancels.retain(|_, at| at.elapsed() < CANCEL_MEMORY);
            cancels.insert(id.to_string(), Instant::now());
        }
        if let Some(worker) = slot.as_mut() {
            if lock(&worker.shared.pending).contains_key(id) {
                let line = to_line(&Request { id: id.to_string(), op: Op::Cancel });
                let _ = worker.stdin.write_all(line.as_bytes()).and_then(|_| worker.stdin.flush());
            }
        }
    }

    /// Stop the engine process, freeing all model memory. The next job starts a new one.
    pub fn shutdown(&self) {
        let worker = lock(&self.worker).take();
        drop(worker);
    }

    /// Stop the engine if nothing has used it for `idle`. Returns whether it was stopped.
    pub fn stop_if_idle(&self, idle: Duration) -> bool {
        // Decided under the worker lock: a job counts itself in under it too (`enter`).
        let mut slot = lock(&self.worker);
        if self.busy.load(Ordering::SeqCst) > 0 || lock(&self.last_used).elapsed() < idle {
            return false;
        }
        if slot.is_some() {
            log::info!("stopping the idle engine to free memory");
            *slot = None;
            return true;
        }
        false
    }

    /// Run `remove` (deleting `model_id`'s files) once no engine can be holding them open.
    /// Refuses while a job uses the model. Otherwise, if the running engine has used the model, it
    /// is stopped and given up to `EXIT_WAIT` to exit, since Windows cannot delete open files.
    /// No job reaches the engine while `remove` runs (so `remove` must not call back into the host).
    pub fn with_model_released<T>(&self, model_id: &str, remove: impl FnOnce() -> Result<T>) -> Result<T> {
        let mut slot = lock(&self.worker);
        if lock(&self.active_models).iter().any(|m| m == model_id) {
            return Err(AppError::Validation(
                "This model is being used by a job that is still running. Wait for it to finish or cancel it, \
                 then delete the model."
                    .into(),
            ));
        }
        let holds_model = slot.as_ref().is_some_and(|w| lock(&w.shared.touched).contains(model_id));
        if holds_model {
            let worker = slot.take().expect("checked above");
            let shared = worker.shared.clone();
            drop(worker);
            let deadline = Instant::now() + self.limits.exit_wait;
            while shared.alive.load(Ordering::Relaxed) {
                if Instant::now() >= deadline {
                    log::warn!("the engine has not exited yet; deleting {} anyway", model_id);
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        remove()
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
        let shared = Arc::new(Shared {
            process: Mutex::new(launched.process),
            pending: Mutex::default(),
            alive: AtomicBool::new(true),
            stopping: AtomicBool::new(false),
            last_event: Mutex::new(Instant::now()),
            resident: Mutex::new(None),
            touched: Mutex::default(),
        });

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
            let shared = shared.clone();
            let idle = self.idle.clone();
            let stdout = launched.stdout;
            std::thread::Builder::new()
                .name("talkr-engine-reader".into())
                .spawn(move || {
                    read_events(stdout, &shared, &idle);
                    drop(reader_done_tx);
                })
                .map_err(AppError::Io)?;
        }
        {
            let shared = shared.clone();
            let idle = self.idle.clone();
            std::thread::Builder::new()
                .name("talkr-engine-watch".into())
                .spawn(move || watch_process(&shared, &idle, &reader_done))
                .map_err(AppError::Io)?;
        }

        Ok(Worker { variant, stdin: launched.stdin, shared })
    }
}

fn read_events(stdout: Box<dyn Read + Send>, shared: &Shared, idle: &Condvar) {
    let mut reader = BufReader::new(stdout);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) => break,
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
        // Decoded leniently: one garbled line (a native library printing to stdout) must not
        // stop the events behind it.
        let line = String::from_utf8_lossy(&buf);
        if line.trim().is_empty() {
            continue;
        }
        let event: Event = match from_line(&line) {
            Ok(e) => e,
            Err(e) => {
                // Not the line itself: it may hold a transcript or the text being spoken.
                log::warn!("engine sent something unreadable ({}, {} bytes)", e, buf.len());
                continue;
            }
        };
        *lock(&shared.last_event) = Instant::now();
        let Some(id) = event.id().map(str::to_string) else { continue };
        let is_final = event.is_final();
        let tx = {
            let mut pending = lock(&shared.pending);
            if is_final { pending.remove(&id) } else { pending.get(&id).cloned() }
        };
        if let Some(tx) = tx {
            let _ = tx.send(Msg::Event(event));
        }
        if is_final {
            idle.notify_all();
        }
    }
}

/// Wait for the engine process to end, then fail whatever was still waiting on it.
fn watch_process(shared: &Shared, idle: &Condvar, reader_done: &flume::Receiver<()>) {
    let (exit, stopped) = loop {
        // Polled rather than a blocking wait, so `kill` can take the lock at any time.
        if let Some(exit) = lock(&shared.process).try_wait() {
            // Read now: a crashed engine that is replaced afterwards is still a crash.
            break (exit, shared.stopping.load(Ordering::SeqCst));
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    shared.alive.store(false, Ordering::Relaxed);
    // Let the reader hand over anything the engine wrote just before it exited.
    let _ = reader_done.recv_timeout(Duration::from_secs(1));
    if stopped {
        log::info!("engine process stopped");
    } else if exit != Exit::Code(0) {
        log::error!("engine process ended: {}", describe(exit));
    }
    for (_, tx) in lock(&shared.pending).drain() {
        let _ = tx.send(if stopped { Msg::Stopped } else { Msg::Died(exit) });
    }
    idle.notify_all();
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
    use std::collections::HashSet;
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
        /// Never answer a transcription, and ignore requests to cancel it (stuck in native code).
        Hang,
        /// Write a line that is not UTF-8 before each answer.
        Garbage,
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
                        (Op::Transcribe(_), Script::Hang) => continue,
                        (Op::Transcribe(job), _) => {
                            if let Script::Garbage = script {
                                let _ = ev_w.write_all(b"\xff\xfe not text \xc3\n");
                            }
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

    fn transcribe_with(model_id: &'static str) -> impl Fn(bool) -> Op {
        move |gpu| match transcribe(gpu) {
            Op::Transcribe(mut job) => {
                job.model.model_id = model_id.into();
                Op::Transcribe(job)
            }
            _ => unreachable!(),
        }
    }

    /// Short limits, so tests about hung engines finish quickly.
    fn quick_limits() -> Limits {
        Limits {
            cancel_grace: Duration::from_millis(300),
            stall: Duration::from_secs(60),
            switch_wait: Duration::from_secs(60),
            exit_wait: Duration::from_secs(5),
            poll: Duration::from_millis(20),
        }
    }

    /// Wait until the fake engine has received `n` requests.
    fn wait_for_requests(requests: &Log<Request>, n: usize) {
        let started = Instant::now();
        while lock(requests).len() < n {
            assert!(started.elapsed() < Duration::from_secs(10), "the engine never got the request");
            std::thread::sleep(Duration::from_millis(10));
        }
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
    fn probing_falls_back_to_the_cpu_when_the_gpu_engine_cannot_start() {
        let (host, launches, _) = make_host(FakeLauncher::new(Script::Healthy, Some(Script::WontStart)));
        let devices = host.devices();
        assert!(!devices.is_empty(), "the CPU engine answered the probe");
        assert!(host.gpu_failed());
        assert_eq!(*lock(&launches), vec![Variant::Cpu]);
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
    fn concurrent_probes_share_one_probe_and_keep_the_gpu() {
        let (host, _, requests) = make_host(FakeLauncher::new(Script::Healthy, Some(Script::Healthy)));
        let host = Arc::new(host);
        let callers: Vec<_> = (0..8)
            .map(|_| {
                let host = host.clone();
                std::thread::spawn(move || host.devices())
            })
            .collect();
        for caller in callers {
            assert_eq!(caller.join().unwrap().len(), 1, "every caller gets the probe's answer");
        }
        assert!(!host.gpu_failed());
        assert_eq!(lock(&requests).iter().filter(|r| r.op == Op::Probe).count(), 1);
        // Probes never share an id.
        host.reset_gpu();
        host.devices();
        let ids: HashSet<String> = lock(&requests).iter().map(|r| r.id.clone()).collect();
        assert_eq!(ids.len(), 2, "{ids:?}");
    }

    #[test]
    fn stopping_the_engine_during_a_job_cancels_it_without_blaming_the_gpu() {
        let (host, launches, requests) = make_host(FakeLauncher::new(Script::Healthy, Some(Script::Hang)));
        let host = Arc::new(host);
        let job = {
            let host = host.clone();
            std::thread::spawn(move || host.run("j1", &transcribe, true, &|_| {}))
        };
        wait_for_requests(&requests, 1);
        host.shutdown();
        let answer = job.join().unwrap();
        assert!(matches!(answer, Err(AppError::Cancelled)), "{answer:?}");
        assert!(!host.gpu_failed() && host.gpu_available());
        assert_eq!(*lock(&launches), vec![Variant::Gpu], "not retried on the CPU");
    }

    #[test]
    fn an_engine_that_ignores_a_cancel_is_killed() {
        let (mut host, launches, requests) = make_host(FakeLauncher::new(Script::Hang, None));
        host.limits = quick_limits();
        let host = Arc::new(host);
        let job = {
            let host = host.clone();
            std::thread::spawn(move || host.run("j1", &transcribe, false, &|_| {}))
        };
        wait_for_requests(&requests, 1);
        let started = Instant::now();
        host.cancel("j1");
        let answer = job.join().unwrap();
        assert!(matches!(answer, Err(AppError::Cancelled)), "{answer:?}");
        assert!(started.elapsed() < Duration::from_secs(5), "took {:?}", started.elapsed());
        assert!(lock(&requests).iter().any(|r| r.id == "j1" && r.op == Op::Cancel), "asked politely first");
        // The hung engine is gone; the next job gets a fresh one.
        assert!(matches!(host.run("j2", &|_| Op::Unload, false, &|_| {}), Ok(Event::Unloaded { .. })));
        assert_eq!(lock(&launches).len(), 2);
    }

    #[test]
    fn a_silent_engine_is_restarted() {
        let (mut host, launches, _) = make_host(FakeLauncher::new(Script::Hang, None));
        host.limits = Limits { stall: Duration::from_millis(300), ..quick_limits() };
        let err = host.run("j1", &transcribe, false, &|_| {}).unwrap_err().to_string();
        assert!(err.contains("stopped responding"), "{err}");
        host.run("j2", &|_| Op::Unload, false, &|_| {}).unwrap();
        assert_eq!(lock(&launches).len(), 2);
    }

    #[test]
    fn a_job_cancelled_before_it_is_sent_never_reaches_the_engine() {
        let (host, launches, requests) = make_host(FakeLauncher::new(Script::Healthy, None));
        host.cancel("j1");
        let answer = host.run("j1", &transcribe, false, &|_| {});
        assert!(matches!(answer, Err(AppError::Cancelled)), "{answer:?}");
        assert!(lock(&requests).is_empty() && lock(&launches).is_empty());
        host.run("j2", &transcribe, false, &|_| {}).unwrap();
    }

    #[test]
    fn unreadable_lines_are_skipped() {
        let (host, _, _) = make_host(FakeLauncher::new(Script::Garbage, None));
        let answer = host.run("j1", &transcribe, false, &|_| {});
        assert!(matches!(answer, Ok(Event::Transcribed { .. })), "{answer:?}");
        host.run("j2", &transcribe, false, &|_| {}).unwrap();
    }

    #[test]
    fn tracks_the_resident_model() {
        let (host, _, _) = make_host(FakeLauncher::new(Script::Healthy, None));
        assert_eq!(host.resident_model(), None);
        host.run("j1", &transcribe_with("first"), false, &|_| {}).unwrap();
        assert_eq!(host.resident_model().as_deref(), Some("first"));
        host.run("j2", &transcribe_with("second"), false, &|_| {}).unwrap();
        assert_eq!(host.resident_model().as_deref(), Some("second"));
        host.run("j3", &|_| Op::Unload, false, &|_| {}).unwrap();
        assert_eq!(host.resident_model(), None);
        host.run("j4", &transcribe_with("first"), false, &|_| {}).unwrap();
        host.shutdown();
        assert_eq!(host.resident_model(), None);
    }

    #[test]
    fn a_model_in_use_cannot_be_released() {
        let (mut host, launches, requests) = make_host(FakeLauncher::new(Script::Hang, None));
        host.limits = quick_limits();
        let host = Arc::new(host);
        let job = {
            let host = host.clone();
            std::thread::spawn(move || host.run("j1", &transcribe_with("busy"), false, &|_| {}))
        };
        wait_for_requests(&requests, 1);
        let refused = host.with_model_released("busy", || Ok(()));
        assert!(matches!(refused, Err(AppError::Validation(_))), "{refused:?}");
        // Another model is not in this engine: it is left alone, job and all.
        let mut removed = false;
        host.with_model_released("other", || {
            removed = true;
            Ok(())
        })
        .unwrap();
        assert!(removed && !job.is_finished());
        host.cancel("j1");
        assert!(matches!(job.join().unwrap(), Err(AppError::Cancelled)));
        host.with_model_released("busy", || Ok(())).unwrap();
        assert_eq!(lock(&launches).len(), 1);
    }

    #[test]
    fn releasing_a_loaded_model_stops_the_engine_first() {
        let (host, launches, _) = make_host(FakeLauncher::new(Script::Healthy, None));
        host.run("j1", &transcribe_with("m"), false, &|_| {}).unwrap();
        let worker_alive = {
            let slot = lock(&host.worker);
            slot.as_ref().unwrap().shared.clone()
        };
        host.with_model_released("m", || {
            assert!(!worker_alive.alive.load(Ordering::Relaxed), "the engine exited before the files go");
            Ok(())
        })
        .unwrap();
        assert_eq!(host.resident_model(), None);
        host.run("j2", &transcribe_with("n"), false, &|_| {}).unwrap();
        assert_eq!(lock(&launches).len(), 2);
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
