//! System-wide dictation: hold a shortcut in any app, speak, let go, and the words are typed
//! where the cursor was.
//!
//! This module is the engine every system shares: the state machine, the microphone,
//! transcription, text clean-up, history and the pill. What differs per system (the shortcut,
//! the focused app, inserting text, placing the pill) sits behind `backend::Backend`, with one
//! implementation each for Windows (`win`), macOS (`macos`) and Linux (`linux`).
//!
//! Three threads share the work. The backend's listener only reports the shortcut. The
//! controller owns the state machine (idle, recording, hands-free) and the microphone, and keeps
//! the pill up to date. The worker turns each finished clip into text and inserts it, one clip at
//! a time, so a new dictation can start while the last one is still being typed.

pub mod backend;
pub mod cues;
pub mod overlay;
pub mod settings;
pub mod speech;
pub mod sys;
pub mod text;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod win;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use chrono::Utc;
use serde::Serialize;
use talkr_protocol::{Decoding, Event, ModelRef, Op, TranscribeJob};
use tauri::{AppHandle, Emitter, Manager};
use crate::audio::record::{PendingStop, RecorderSignals};
use crate::config::SttQuality;
use crate::db::{insert_history, HistoryItem, HistoryKind};
use crate::engine_host::failure_to_error;
use crate::error::{AppError, Result};
use crate::AppState;
use backend::{Backend, Capabilities, Cue, Delivery, Permission};
use overlay::{Overlay, State as Pill, Tone};
use settings::{ActivationMode, DictationSettings, Shortcut};
use sys::{Os, Target};

/// Main-window events.
pub const EVENT_STATUS: &str = "dictation://status";
pub const EVENT_CAPTURED: &str = "dictation://captured";
/// The keys held so far while recording a shortcut (payload: Shortcut).
pub const EVENT_CAPTURING: &str = "dictation://capturing";
/// Recording a shortcut could not start (payload: { message }).
pub const EVENT_CAPTURE_FAILED: &str = "dictation://capture-failed";
pub const EVENT_SETTINGS: &str = "dictation://settings";
pub const EVENT_DONE: &str = "dictation://done";

/// Whether dictation works on this system at all.
pub fn supported() -> bool {
    Os::capabilities().supported
}

/// The paste keystroke, for messages.
pub const PASTE_KEYS: &str = if cfg!(target_os = "macos") { "⌘V" } else { "Ctrl+V" };

/// A press shorter than this, in Auto mode, starts hands-free dictation instead of hold-to-talk.
const TAP: Duration = Duration::from_millis(320);
/// In Hold mode, a press shorter than this was an accident and is dropped.
const MIN_HOLD: Duration = Duration::from_millis(250);
/// Modifier-only shortcuts are also the start of Windows shortcuts (Ctrl + Win + Left): wait
/// this long before showing the pill, so those do not flash it.
const SHOW_DELAY: Duration = Duration::from_millis(140);
/// Longest single dictation.
const MAX_LENGTH: Duration = Duration::from_secs(10 * 60);
const TICK: Duration = Duration::from_millis(33);

/// What the keyboard hook reports.
#[derive(Debug, Clone, PartialEq, Eq)]
// A backend that is not implemented yet constructs none of these.
#[cfg_attr(not(windows), allow(dead_code))]
pub enum HotkeyEvent {
    DictateDown,
    DictateUp,
    /// The modifiers of a modifier-only shortcut met another key: a system shortcut.
    Interrupted,
    /// Escape while recording.
    Cancel,
    PasteLast,
    /// While recording a new shortcut: the keys held so far, for the page to show live.
    Capturing(Shortcut),
    Captured(Shortcut),
    CaptureCancelled,
}

enum Input {
    Hotkey(HotkeyEvent),
    /// The pill's stop button.
    Stop,
    /// The pill's cancel button.
    Cancel,
    CopyLast,
    /// Start a dictation, or finish the one running: `talkr --dictate`, for systems where Talkr
    /// cannot listen for a shortcut itself (bind the command to a key in the system settings).
    Toggle,
    /// Settings changed: reconfigure the listener, autostart and warm-up.
    Reconfigure,
    Capture(bool),
}

#[derive(Debug, Clone, Serialize)]
struct CaptureFailed {
    message: String,
}

/// For the settings page.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// This platform supports dictation.
    pub supported: bool,
    /// What dictation can do here (hold-to-talk, recording shortcuts, inserting text…).
    pub capabilities: Capabilities,
    /// What the system still has to allow.
    pub permission: Permission,
    /// The shortcut is being listened for.
    pub active: bool,
    pub error: Option<String>,
    /// The model dictation will use, if one is installed.
    pub model_id: Option<String>,
    /// That model is loaded and ready.
    pub warm: bool,
    pub has_last: bool,
    /// A dictation is recording right now.
    pub recording: bool,
}

/// Managed state: the controller's inbox and what the settings page can ask about.
pub struct Dictation {
    tx: flume::Sender<Input>,
    shared: Arc<Shared>,
}

struct Shared {
    error: Mutex<Option<String>>,
    last_text: Mutex<Option<String>>,
    /// A dictation is recording: the worker keeps the pill for it.
    recording: AtomicBool,
}

impl Dictation {
    /// Start the controller and worker. The hook starts once dictation is enabled.
    pub fn start(app: &AppHandle) -> Self {
        let (tx, rx) = flume::unbounded::<Input>();
        let (hotkey_tx, hotkey_rx) = flume::unbounded::<HotkeyEvent>();
        let shared = Arc::new(Shared {
            error: Mutex::new(None),
            last_text: Mutex::new(None),
            recording: AtomicBool::new(false),
        });
        let overlay = match Overlay::create(app) {
            Ok(o) => Some(o),
            Err(e) => {
                log::error!("could not create the dictation overlay: {}", e);
                None
            }
        };
        sweep_clips(&app.state::<AppState>().paths.cache.join("dictation"));
        let (job_tx, job_rx) = flume::unbounded::<Job>();
        let worker = Worker { app: app.clone(), overlay: overlay.clone(), shared: shared.clone() };
        let _ = std::thread::Builder::new().name("talkr-dictation-worker".into()).spawn(move || {
            for job in job_rx.iter() {
                worker.handle(job);
            }
        });
        let controller = Controller {
            app: app.clone(),
            overlay,
            shared: shared.clone(),
            jobs: job_tx,
            hotkeys: hotkey_tx,
            session: None,
            next_session: 1,
            capturing: false,
        };
        let _ = std::thread::Builder::new()
            .name("talkr-dictation".into())
            .spawn(move || controller.run(rx, hotkey_rx));
        let this = Self { tx, shared };
        this.reconfigure();
        this
    }

    pub fn reconfigure(&self) {
        let _ = self.tx.send(Input::Reconfigure);
    }

    pub fn stop(&self) {
        let _ = self.tx.send(Input::Stop);
    }

    pub fn cancel(&self) {
        let _ = self.tx.send(Input::Cancel);
    }

    /// Put the last dictation on the clipboard. For the tray menu and Talkr's own buttons: a
    /// click there leaves no text field focused to paste into.
    pub fn copy_last(&self) {
        let _ = self.tx.send(Input::CopyLast);
    }

    pub fn capture(&self, active: bool) {
        let _ = self.tx.send(Input::Capture(active));
    }

    /// Start a dictation or finish the running one (`talkr --dictate`).
    pub fn toggle(&self) {
        let _ = self.tx.send(Input::Toggle);
    }

    /// Ask the system for what dictation still needs, then refresh.
    pub fn request_permission(&self) {
        Os::request_permission();
        self.reconfigure();
    }

    pub fn status(&self, state: &AppState) -> Status {
        let settings = state.settings().dictation.clone();
        let model_id = resolve_model(state, &settings);
        let warm = model_id.is_some() && state.engine.resident_model() == model_id;
        let capabilities = Os::capabilities();
        Status {
            supported: capabilities.supported,
            active: capabilities.supported && settings.enabled && Os::hotkeys_running(),
            capabilities,
            permission: Os::permission(),
            error: lock(&self.shared.error).clone(),
            model_id,
            warm,
            has_last: lock(&self.shared.last_text).is_some(),
            recording: self.shared.recording.load(Ordering::SeqCst),
        }
    }
}

/// Delete recordings left by a dictation that never finished (Talkr quit or crashed mid-way):
/// they are the user's voice and must not linger. Keeps the warm-up clip.
fn sweep_clips(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        let leftover = path.extension().is_some_and(|e| e == "wav") && path.file_name().is_some_and(|n| n != "warm-up.wav");
        if leftover {
            if let Err(e) = std::fs::remove_file(&path) {
                log::warn!("could not delete the leftover recording {}: {}", path.display(), e);
            }
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Preferred dictation models, best first, when the user has not picked one: fast and accurate
/// for short clips.
const PREFERRED: &[&str] = &[
    "whisper-large-v3-turbo-q5",
    "whisper-large-v3-turbo",
    "whisper-small-en-q5",
    "whisper-small-en",
    "whisper-small-q5",
    "whisper-small",
    "whisper-base-en",
    "whisper-base-q5",
    "whisper-base",
    "whisper-medium",
    "whisper-tiny-en",
];

fn installed(state: &AppState, id: &str) -> bool {
    crate::commands::models::locate_model(&state.paths, id).is_some_and(|(kind, dir)| {
        kind == crate::catalog::ModelKind::Stt && dir.join("manifest.json").is_file()
    })
}

/// The model dictation uses: its own setting, else the default transcription model, else the
/// best installed one.
pub fn resolve_model(state: &AppState, s: &DictationSettings) -> Option<String> {
    let default = state.settings().default_stt_model.clone();
    for candidate in [s.model.clone(), default].into_iter().flatten() {
        if installed(state, &candidate) {
            return Some(candidate);
        }
    }
    if let Some(id) = PREFERRED.iter().find(|id| installed(state, id)) {
        return Some(id.to_string());
    }
    // An imported model.
    std::fs::read_dir(&state.paths.models_stt)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().join("manifest.json").is_file())
        .filter_map(|e| e.file_name().into_string().ok())
        .min()
}

/// The model to keep loaded while dictation is on, if any.
pub fn warm_model(state: &AppState) -> Option<String> {
    let s = state.settings().dictation.clone();
    if !Os::capabilities().supported || !s.enabled || !s.keep_warm {
        return None;
    }
    resolve_model(state, &s)
}

/// Load the dictation model in the background, so the first dictation does not wait for it.
pub fn warm_up(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        let s = state.settings().dictation.clone();
        let Some(model_id) = resolve_model(&state, &s) else { return };
        if state.engine.resident_model().as_deref() == Some(model_id.as_str()) || state.engine.is_busy() {
            return;
        }
        let dir = state.paths.cache.join("dictation");
        let clip = dir.join("warm-up.wav");
        if !clip.is_file() {
            let _ = std::fs::create_dir_all(&dir);
            if speech::write_span(&clip, &[0.0; 8_000], (0, 8_000)).is_err() {
                return;
            }
        }
        let gpu = state.engine.should_use_gpu(state.gpu_policy());
        let Ok(model) = crate::commands::stt::stt_model(&state, &model_id, gpu, 32_000) else { return };
        log::info!("loading {} for dictation", model_id);
        let make_op = |gpu: bool| {
            Op::Transcribe(TranscribeJob {
                model: ModelRef { gpu, ..model.clone() },
                audio_path: clip.clone(),
                language: Some("en".into()),
                translate: false,
                decoding: Decoding::Greedy,
                prompt: None,
            })
        };
        let id = format!("dictation-warm-{}", uuid::Uuid::new_v4());
        let _ = state.engine.run(&id, &make_op, gpu, &|_| {});
        emit_status(&app);
    });
}

fn emit_status(app: &AppHandle) {
    if let Some(d) = app.try_state::<Dictation>() {
        let state = app.state::<AppState>();
        let _ = app.emit(EVENT_STATUS, d.status(&state));
    }
}

struct Session {
    id: u64,
    started: Instant,
    locked: bool,
    target: Option<Target>,
    app_label: Option<String>,
    signals: RecorderSignals,
    clip: PathBuf,
    shown: bool,
    show_at: Instant,
    settings: DictationSettings,
    model_id: String,
    /// Ticks in a row the shortcut has not been physically held (see `tick`).
    released_ticks: u8,
}

/// A finished recording, for the worker.
struct Clip {
    session: u64,
    pending: PendingStop,
    clip: PathBuf,
    target: Option<Target>,
    app_label: Option<String>,
    settings: DictationSettings,
    model_id: String,
}

enum Job {
    Transcribe(Box<Clip>),
    PasteLast,
    CopyLast,
}

struct Controller {
    app: AppHandle,
    overlay: Option<Overlay>,
    shared: Arc<Shared>,
    jobs: flume::Sender<Job>,
    hotkeys: flume::Sender<HotkeyEvent>,
    session: Option<Session>,
    next_session: u64,
    capturing: bool,
}

impl Controller {
    fn run(mut self, inbox: flume::Receiver<Input>, hotkeys: flume::Receiver<HotkeyEvent>) {
        loop {
            let timeout = if self.session.is_some() { TICK } else { Duration::from_secs(3600) };
            let input = flume::Selector::new()
                .recv(&inbox, |r| r.ok())
                .recv(&hotkeys, |r| r.ok().map(Input::Hotkey))
                .wait_timeout(timeout);
            match input {
                Ok(Some(input)) => self.handle(input),
                Ok(None) => return, // the app is gone
                Err(_) => {}
            }
            self.tick();
        }
    }

    fn pill(&self, state: Pill) {
        if let Some(o) = &self.overlay {
            o.set(state);
        }
    }

    fn handle(&mut self, input: Input) {
        let mode = self.app.state::<AppState>().settings().dictation.mode;
        match input {
            Input::Hotkey(HotkeyEvent::DictateDown) => {
                let shortcut = self.app.state::<AppState>().settings().dictation.shortcut.clone();
                Os::on_shortcut_pressed(&shortcut);
                if let Some(session) = &self.session {
                    if session.locked || mode == ActivationMode::Toggle {
                        self.finish();
                    }
                } else {
                    self.begin(mode == ActivationMode::Toggle);
                }
            }
            Input::Hotkey(HotkeyEvent::DictateUp) => {
                let Some(session) = &mut self.session else { return };
                if session.locked {
                    return;
                }
                let held = session.started.elapsed();
                match mode {
                    ActivationMode::Auto if held < TAP => {
                        session.locked = true;
                        let (id, app_label) = (session.id, session.app_label.clone());
                        self.pill(Pill::Listening { session: id, locked: true, app: app_label });
                        if let Some(o) = &self.overlay {
                            o.set_interactive(true);
                        }
                    }
                    ActivationMode::Hold if held < MIN_HOLD => self.cancel(false),
                    ActivationMode::Toggle => {}
                    _ => self.finish(),
                }
            }
            Input::Hotkey(HotkeyEvent::Interrupted) => {
                if self.session.as_ref().is_some_and(|s| !s.locked) {
                    self.cancel(false);
                }
            }
            Input::Hotkey(HotkeyEvent::Cancel) | Input::Cancel => {
                if self.session.is_some() {
                    self.cancel(true);
                }
            }
            Input::Stop => {
                if self.session.is_some() {
                    self.finish();
                }
            }
            Input::Toggle => {
                if self.session.is_some() {
                    self.finish();
                } else {
                    self.begin(true);
                }
            }
            Input::Hotkey(HotkeyEvent::PasteLast) => {
                let _ = self.jobs.send(Job::PasteLast);
            }
            Input::CopyLast => {
                let _ = self.jobs.send(Job::CopyLast);
            }
            Input::Hotkey(HotkeyEvent::Capturing(shortcut)) => {
                let _ = self.app.emit(EVENT_CAPTURING, shortcut);
            }
            Input::Hotkey(HotkeyEvent::Captured(shortcut)) => {
                self.capturing = false;
                let _ = self.app.emit(EVENT_CAPTURED, Some(shortcut));
                self.apply_settings();
            }
            Input::Hotkey(HotkeyEvent::CaptureCancelled) => {
                self.capturing = false;
                let _ = self.app.emit(EVENT_CAPTURED, Option::<Shortcut>::None);
                self.apply_settings();
            }
            Input::Capture(active) => {
                self.capturing = active;
                if active {
                    if let Err(e) = Os::start_hotkeys(self.hotkeys.clone()) {
                        *lock(&self.shared.error) = Some(e.clone());
                        // The page is waiting for a shortcut that cannot come: tell it why.
                        self.capturing = false;
                        let _ = self.app.emit(EVENT_CAPTURE_FAILED, CaptureFailed { message: e });
                    }
                }
                Os::set_capturing(self.capturing);
                if !active {
                    self.apply_settings();
                }
            }
            Input::Reconfigure => self.apply_settings(),
        }
    }

    /// Bring the hook, autostart and model in line with the settings.
    fn apply_settings(&mut self) {
        let settings = self.app.state::<AppState>().settings().dictation.clone();
        if settings.enabled && Os::capabilities().supported {
            match Os::start_hotkeys(self.hotkeys.clone()) {
                Ok(()) => *lock(&self.shared.error) = None,
                Err(e) => {
                    log::error!("{}", e);
                    *lock(&self.shared.error) = Some(e);
                }
            }
            Os::configure_hotkeys(
                Some(&settings.shortcut),
                settings.paste_last_enabled.then_some(&settings.paste_last_shortcut),
            );
            if let Some(problem) = Os::hotkeys_problem() {
                let mut error = lock(&self.shared.error);
                if error.is_none() {
                    log::warn!("{}", problem);
                    *error = Some(problem);
                }
            }
        } else {
            Os::configure_hotkeys(None, None);
            if !self.capturing {
                Os::stop_hotkeys();
            }
            if self.session.is_some() {
                self.cancel(false);
            }
        }
        sync_autostart(&self.app, settings.launch_at_login);
        if settings.enabled && settings.keep_warm {
            warm_up(&self.app);
        }
        emit_status(&self.app);
    }

    fn notice(&self, tone: Tone, title: &str, detail: Option<String>, anchor: Option<&Target>) {
        let session = self.next_session;
        self.pill(Pill::Notice { session, tone, title: title.into(), detail });
        if let Some(o) = &self.overlay {
            let position = self.app.state::<AppState>().settings().dictation.overlay_position;
            o.show(anchor, position);
            o.hide_latest_after(Duration::from_millis(2_800));
        }
        if self.app.state::<AppState>().settings().dictation.sounds {
            Os::play(Cue::Problem);
        }
    }

    fn begin(&mut self, locked: bool) {
        let state = self.app.state::<AppState>();
        let settings = state.settings().dictation.clone();
        if !settings.enabled {
            return;
        }
        let target = Os::snapshot();
        let anchor = target.as_ref();

        let Some(model_id) = resolve_model(&state, &settings) else {
            self.notice(Tone::Warn, "No speech model yet", Some("Open Talkr › Dictation to download one".into()), anchor);
            return;
        };
        if state.engine.resident_model().as_deref() != Some(model_id.as_str()) {
            // Load it while the user speaks.
            warm_up(&self.app);
        }
        if state.recorder().is_recording() {
            self.notice(Tone::Warn, "Talkr is already recording", Some("Finish the recording in Talkr first".into()), anchor);
            return;
        }

        let dir = state.paths.cache.join("dictation");
        let _ = std::fs::create_dir_all(&dir);
        let clip = dir.join(format!("{}.wav", uuid::Uuid::new_v4()));
        let started = {
            let mut recorder = state.recorder();
            recorder.start_with(&clip, settings.microphone.as_deref()).map(|_| recorder.signals())
        };
        let signals = match started {
            Ok(signals) => signals,
            Err(e) => {
                let _ = std::fs::remove_file(&clip);
                let message = match e {
                    AppError::Audio(m) => m,
                    other => other.to_string(),
                };
                self.notice(Tone::Error, "Microphone unavailable", Some(message), anchor);
                return;
            }
        };

        let id = self.next_session;
        self.next_session += 1;
        let app_label = target.as_ref().and_then(Os::app_label).filter(|_| settings.show_target);
        Os::set_recording(true);
        self.shared.recording.store(true, Ordering::SeqCst);

        log::info!(
            "dictation started{} into {}",
            if locked { " hands-free" } else { "" },
            target.as_ref().and_then(Os::app_id).unwrap_or_else(|| "an unknown window".into())
        );
        let modifier_only = settings.shortcut.key.is_none();
        let now = Instant::now();
        self.pill(Pill::Listening { session: id, locked, app: app_label.clone() });
        if locked {
            if let Some(o) = &self.overlay {
                o.set_interactive(true);
            }
        }
        emit_status(&self.app);
        self.session = Some(Session {
            id,
            started: now,
            locked,
            target,
            app_label,
            signals,
            clip,
            shown: false,
            show_at: if modifier_only && !locked { now + SHOW_DELAY } else { now },
            settings,
            model_id,
            released_ticks: 0,
        });
        self.tick();
    }

    fn tick(&mut self) {
        let Some(session) = &mut self.session else { return };
        if !session.shown && Instant::now() >= session.show_at {
            session.shown = true;
            if let Some(o) = &self.overlay {
                o.show(session.target.as_ref(), session.settings.overlay_position);
            }
            // With the pill, not on the key press: Ctrl + Win + Left (switching desktops) starts
            // like a dictation and should stay silent.
            if session.settings.sounds {
                Os::play(Cue::Start);
            }
        }
        if session.shown {
            if let Some(o) = &self.overlay {
                o.level(session.id, session.signals.level());
            }
        }
        if let Some(error) = session.signals.error() {
            log::warn!("dictation microphone error: {}", error);
            self.finish();
            return;
        }
        // Backstop for a release the hook never saw (let go while an elevated window or the lock
        // screen had the keyboard): a held dictation whose keys are up ends as if released.
        if let (false, Some(held)) = (session.locked, Os::shortcut_held(&session.settings.shortcut)) {
            if held {
                session.released_ticks = 0;
            } else {
                session.released_ticks += 1;
                if session.released_ticks >= 4 {
                    log::info!("dictation: the shortcut is no longer held; treating it as released");
                    self.handle(Input::Hotkey(HotkeyEvent::DictateUp));
                    return;
                }
            }
        }
        if session.started.elapsed() >= MAX_LENGTH {
            log::info!("dictation reached {} minutes; stopping", MAX_LENGTH.as_secs() / 60);
            self.finish();
        }
    }

    fn end_recording(&mut self) -> Option<(Session, Option<PendingStop>)> {
        let session = self.session.take()?;
        Os::set_recording(false);
        self.shared.recording.store(false, Ordering::SeqCst);
        if let Some(o) = &self.overlay {
            o.set_interactive(false);
        }
        let pending = self.app.state::<AppState>().recorder().request_stop().ok();
        emit_status(&self.app);
        Some((session, pending))
    }

    fn finish(&mut self) {
        let Some((session, pending)) = self.end_recording() else { return };
        if session.settings.sounds {
            Os::play(Cue::Stop);
        }
        let Some(pending) = pending else {
            let _ = std::fs::remove_file(&session.clip);
            self.pill(Pill::Notice {
                session: session.id,
                tone: Tone::Error,
                title: "The recording stopped unexpectedly".into(),
                detail: None,
            });
            if let Some(o) = &self.overlay {
                o.hide_latest_after(Duration::from_millis(2_500));
            }
            return;
        };
        if !session.shown {
            // Released before the pill appeared: show it now for the result.
            session.shown_now(self.overlay.as_ref());
        }
        self.pill(Pill::Working { session: session.id, label: "Transcribing".into() });
        let _ = self.jobs.send(Job::Transcribe(Box::new(Clip {
            session: session.id,
            pending,
            clip: session.clip,
            target: session.target,
            app_label: session.app_label,
            settings: session.settings,
            model_id: session.model_id,
        })));
    }

    /// Drop the recording. `visible`: show that it was cancelled (Escape); otherwise just go
    /// (a Windows shortcut, an accidental tap).
    fn cancel(&mut self, visible: bool) {
        let Some((session, pending)) = self.end_recording() else { return };
        log::info!(
            "dictation cancelled after {} ms{}",
            session.started.elapsed().as_millis(),
            if visible { "" } else { " (another shortcut, or too short)" }
        );
        let clip = session.clip.clone();
        std::thread::spawn(move || {
            if let Some(pending) = pending {
                let _ = pending.wait();
            }
            let _ = std::fs::remove_file(&clip);
        });
        if let Some(o) = &self.overlay {
            if visible && session.shown {
                o.set(Pill::Cancelled { session: session.id });
                o.hide_latest_after(Duration::from_millis(650));
            } else {
                o.set(Pill::Hidden);
                o.hide_latest_after(Duration::from_millis(250));
            }
        }
    }
}

impl Session {
    fn shown_now(&self, overlay: Option<&Overlay>) {
        if let Some(o) = overlay {
            o.show(self.target.as_ref(), self.settings.overlay_position);
        }
    }
}

/// Turn launching at login on or off to match the setting.
fn sync_autostart(app: &AppHandle, wanted: bool) {
    use tauri_plugin_autostart::ManagerExt;
    let launcher = app.autolaunch();
    match launcher.is_enabled() {
        Ok(on) if on == wanted => {}
        Ok(_) => {
            let result = if wanted { launcher.enable() } else { launcher.disable() };
            if let Err(e) = result {
                log::warn!("could not change launch at login: {}", e);
            }
        }
        Err(e) => log::warn!("could not read launch at login: {}", e),
    }
}

struct Worker {
    app: AppHandle,
    overlay: Option<Overlay>,
    shared: Arc<Shared>,
}

/// Payload of `dictation://done`, for the settings page's try-it box and History.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Done {
    text: String,
    inserted: bool,
    method: Option<&'static str>,
    app: Option<String>,
}

impl Worker {
    /// Show `state` unless a newer dictation is recording (its pill wins).
    fn pill(&self, state: Pill, hide_after: Option<Duration>) {
        if let Some(o) = &self.overlay {
            if let (Some(generation), Some(delay)) = (o.set_unless(&self.shared.recording, state), hide_after) {
                o.hide_after(generation, delay);
            }
        }
    }

    fn problem(&self, session: u64, tone: Tone, title: &str, detail: Option<String>, sounds: bool) {
        if sounds && !self.shared.recording.load(Ordering::SeqCst) {
            Os::play(Cue::Problem);
        }
        self.pill(Pill::Notice { session, tone, title: title.into(), detail }, Some(Duration::from_millis(2_800)));
    }

    fn handle(&self, job: Job) {
        match job {
            Job::Transcribe(job) => {
                let Clip { session, pending, clip, target, app_label, settings, model_id } = *job;
                let sounds = settings.sounds;
                let result = crate::commands::catch_panic(|| {
                    self.transcribe_and_insert(session, pending, &clip, target, app_label, &settings, &model_id)
                });
                if let Err(e) = result {
                    log::error!("dictation failed: {}", e);
                    self.problem(session, Tone::Error, "Dictation failed", Some(short_error(&e)), sounds);
                }
                let _ = std::fs::remove_file(&clip);
            }
            Job::PasteLast => self.paste_last(),
            Job::CopyLast => self.copy_last(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn transcribe_and_insert(
        &self,
        session: u64,
        pending: PendingStop,
        clip: &Path,
        target: Option<Target>,
        app_label: Option<String>,
        settings: &DictationSettings,
        model_id: &str,
    ) -> Result<()> {
        let start = Instant::now();
        let recorded = pending.wait()?;
        let stopped_ms = start.elapsed().as_millis();
        if recorded.samples == 0 {
            self.problem(session, Tone::Warn, "Nothing was recorded", recorded.error.clone(), settings.sounds);
            return Ok(());
        }
        // Debug builds only: dictate a prepared WAV (16 kHz mono) instead of the microphone, for
        // end-to-end tests on machines without a usable microphone.
        #[cfg(debug_assertions)]
        if let Some(fake) = std::env::var_os("TALKR_DICTATION_TEST_AUDIO") {
            std::fs::copy(&fake, clip)?;
        }
        let analysis = speech::prepare_clip(clip)?;
        if analysis.peak < 1e-6 {
            // Exact zeros: no signal at all, which is a muted or blocked microphone, not a quiet room.
            log::warn!("dictation: the microphone sent only silence for {} ms", recorded.duration_ms());
            self.problem(
                session,
                Tone::Warn,
                "Your microphone is silent",
                Some("Check it isn't muted, and that your system lets Talkr use it".into()),
                settings.sounds,
            );
            return Ok(());
        }
        if !analysis.has_speech() {
            log::info!(
                "dictation: no speech in {} ms of audio (loudest {:.4}, room {:.4}, {} ms above the threshold)",
                recorded.duration_ms(),
                analysis.peak,
                analysis.floor,
                analysis.speech_ms
            );
            self.pill(
                Pill::Notice { session, tone: Tone::Info, title: "No speech heard".into(), detail: None },
                Some(Duration::from_millis(1_600)),
            );
            return Ok(());
        }

        let state = self.app.state::<AppState>();
        if state.engine.is_busy() {
            self.pill(Pill::Working { session, label: "Waiting for the engine".into() }, None);
        }
        let (language, quality) = {
            let s = state.settings();
            (settings.language.clone().unwrap_or_else(|| s.stt_language.clone()), s.stt_quality)
        };
        let gpu = state.engine.should_use_gpu(state.gpu_policy());
        let audio_bytes = analysis.span.map(|(a, b)| (b - a) as u64 * 4).unwrap_or(0);
        let model = crate::commands::stt::stt_model(&state, model_id, gpu, audio_bytes)?;
        let decoding = if quality == SttQuality::Accurate { Decoding::Beam } else { Decoding::Greedy };
        let prompt = settings.vocabulary_prompt();
        let make_op = |gpu: bool| {
            Op::Transcribe(TranscribeJob {
                model: ModelRef { gpu, ..model.clone() },
                audio_path: clip.to_path_buf(),
                language: Some(language.clone()).filter(|l| !l.is_empty()),
                translate: false,
                decoding,
                prompt: prompt.clone(),
            })
        };
        let job_id = format!("dictation-{}", uuid::Uuid::new_v4());
        let engine_start = Instant::now();
        let (transcript, audio_ms, device) = match state.engine.run(&job_id, &make_op, gpu, &|_| {})? {
            Event::Transcribed { transcript, audio_ms, device, .. } => (transcript, audio_ms, device),
            Event::Failed { kind, error, .. } => return Err(failure_to_error(kind, error)),
            other => return Err(AppError::Engine(format!("Unexpected reply from the engine: {:?}", other))),
        };

        let options = text::Options { smart_spacing: false, ..text::Options::from_settings(settings) };
        let Some(clean) = text::finish(&transcript.text, &options, None, analysis.speech_ms) else {
            self.pill(
                Pill::Notice { session, tone: Tone::Info, title: "No words recognised".into(), detail: None },
                Some(Duration::from_millis(1_800)),
            );
            return Ok(());
        };
        *lock(&self.shared.last_text) = Some(clean.clone());
        let transcribe_ms = engine_start.elapsed().as_millis();

        let insert_start = Instant::now();
        let delivery = self.deliver(&clean, target.as_ref(), settings);
        log::info!(
            "dictation: {} chars from {} ms of speech in {} ms (stop {}, transcribe {} on {}, insert {}), {}",
            clean.chars().count(),
            analysis.speech_ms,
            start.elapsed().as_millis(),
            stopped_ms,
            transcribe_ms,
            device,
            insert_start.elapsed().as_millis(),
            delivery.summary()
        );

        if settings.save_history && !delivery.password {
            let item = HistoryItem {
                id: job_id.trim_start_matches("dictation-").to_string(),
                kind: HistoryKind::Stt,
                created_at: Utc::now().timestamp_millis(),
                title: crate::commands::tts::make_title(&clean),
                text: clean.clone(),
                audio_path: None,
                duration_ms: Some(audio_ms),
                model_id: model_id.to_string(),
                voice_id: None,
                language: transcript.language.clone(),
                device,
                processing_ms: start.elapsed().as_millis() as i64,
                favorite: false,
                segments_json: Some(serde_json::to_string(&transcript.segments)?),
            };
            if let Err(e) = insert_history(&state.paths, &item) {
                log::warn!("could not save the dictation to history: {}", e);
            }
        }
        let _ = self.app.emit(
            EVENT_DONE,
            Done { text: clean.clone(), inserted: delivery.inserted, method: delivery.method, app: app_label.clone() },
        );

        match delivery.copied {
            None => self.pill(
                Pill::Done { session, preview: text::preview(&clean, 46), app: app_label },
                Some(Duration::from_millis(1_150)),
            ),
            Some(ref reason) => self.problem(
                session,
                Tone::Warn,
                &delivery.not_inserted_title(),
                Some(reason.to_string()),
                settings.sounds,
            ),
        }
        if let Some(exe) = delivery.learned_typing_for {
            self.learn_typing(&exe);
        }
        emit_status(&self.app);
        Ok(())
    }

    fn deliver(&self, clean: &str, target: Option<&Target>, settings: &DictationSettings) -> Delivery {
        let owner = self.overlay.as_ref().and_then(Overlay::window);
        Os::deliver(clean, target, settings, owner.as_ref())
    }

    /// Remember that typing works in `exe` where pasting did not.
    fn learn_typing(&self, exe: &str) {
        let state = self.app.state::<AppState>();
        let mut current = state.settings();
        if current.dictation.rule_for(exe).is_some() || current.dictation.app_rules.len() >= settings::MAX_APP_RULES {
            return;
        }
        let mut updated = current.clone();
        updated.dictation.app_rules.push(settings::AppRule {
            app: exe.to_string(),
            method: settings::AppMethod::Type,
            learned: true,
        });
        match updated.save(&state.paths) {
            Ok(()) => {
                log::info!("dictation: {} takes typing, not pasting; remembered", exe);
                *current = updated.clone();
                drop(current);
                let _ = self.app.emit(EVENT_SETTINGS, updated);
            }
            Err(e) => log::warn!("could not save the learned rule for {}: {}", exe, e),
        }
    }

    fn copy_last(&self) {
        let Some(text) = lock(&self.shared.last_text).clone() else {
            self.problem(0, Tone::Info, "Nothing dictated yet", None, false);
            return;
        };
        let owner = self.overlay.as_ref().and_then(Overlay::window);
        if Os::copy(&text, owner.as_ref()) {
            let copied = Pill::Notice {
                session: 0,
                tone: Tone::Info,
                title: "Last dictation copied".into(),
                detail: Some(format!("Press {} to paste", PASTE_KEYS)),
            };
            self.pill(copied, Some(Duration::from_millis(2_000)));
        } else {
            self.problem(0, Tone::Warn, "The clipboard is busy", Some("Try again in a moment".into()), false);
        }
    }

    fn paste_last(&self) {
        let Some(text) = lock(&self.shared.last_text).clone() else {
            self.problem(0, Tone::Info, "Nothing dictated yet", None, false);
            return;
        };
        let settings = self.app.state::<AppState>().settings().dictation.clone();
        let target = Os::snapshot();
        let delivery = self.deliver(&text, target.as_ref(), &DictationSettings {
            focus_policy: settings::FocusPolicy::Current,
            ..settings.clone()
        });
        if let Some(reason) = &delivery.copied {
            self.problem(0, Tone::Warn, &delivery.not_inserted_title(), Some(reason.clone()), settings.sounds);
        }
    }
}

/// The first sentence of an error, short enough for the pill.
fn short_error(e: &AppError) -> String {
    let message = match e {
        AppError::Audio(m) | AppError::Engine(m) | AppError::Validation(m) | AppError::NotFound(m) => m.clone(),
        other => other.to_string(),
    };
    text::preview(message.split(". ").next().unwrap_or(&message), 70)
}
