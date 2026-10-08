//! The shortcut listener: passive key grabs on the root window, on a thread of its own with its
//! own X connection. See `chord` for what it decides; this is the I/O around it.
//!
//! Each grab is synchronous: after delivering a key the server holds the keyboard until Talkr
//! answers, so a key that is not Talkr's can be replayed to the app as if never grabbed. The
//! thread therefore answers every key at once and never blocks while a grab is active.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use x11rb::connection::Connection;
use x11rb::protocol::xkb::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{
    Allow, ClientMessageEvent, ConnectionExt as _, EventMask, GrabMode, GrabStatus, KeyPressEvent, Mapping, ModMask, Window,
};
use x11rb::protocol::Event;
use super::chord::{self, Capture, Chord, Config, Machine, Verdict};
use super::input;
use super::keymap::Keymap;
use super::xconn::{Display, Fail};
use crate::dictation::settings::Shortcut;
use crate::dictation::HotkeyEvent;

static CONFIG: Mutex<Config> = Mutex::new(Config { dictate: None, paste: None, recording: false });
static CAPTURING: AtomicBool = AtomicBool::new(false);
static STOP: AtomicBool = AtomicBool::new(false);
static EVENTS: Mutex<Option<flume::Sender<HotkeyEvent>>> = Mutex::new(None);
static RUNNING: Mutex<Option<Running>> = Mutex::new(None);

struct Running {
    display: Arc<Display>,
    wake: Window,
    thread: std::thread::JoinHandle<()>,
}

fn emit(event: HotkeyEvent) {
    if let Some(tx) = EVENTS.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        let _ = tx.try_send(event);
    }
}

/// Tell the listener thread to look at the settings again.
fn wake() {
    let running = RUNNING.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(r) = running.as_ref() {
        let message = ClientMessageEvent::new(32, r.wake, r.display.atoms._TALKR_WAKE, [0u32; 5]);
        let _ = r.display.conn.send_event(false, r.wake, EventMask::NO_EVENT, message);
        let _ = r.display.conn.flush();
    }
}

pub fn configure(dictate: Option<&Shortcut>, paste_last: Option<&Shortcut>) {
    {
        let mut config = CONFIG.lock().unwrap_or_else(|e| e.into_inner());
        config.dictate = dictate.map(Chord::of);
        config.paste = paste_last.map(Chord::of);
    }
    wake();
}

pub fn set_recording(recording: bool) {
    CONFIG.lock().unwrap_or_else(|e| e.into_inner()).recording = recording;
    wake();
}

pub fn set_capturing(capturing: bool) {
    CAPTURING.store(capturing, Ordering::SeqCst);
    wake();
}

pub fn is_running() -> bool {
    RUNNING.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|r| !r.thread.is_finished())
}

pub fn start(events: flume::Sender<HotkeyEvent>) -> Result<(), String> {
    *EVENTS.lock().unwrap_or_else(|e| e.into_inner()) = Some(events);
    if is_running() {
        return Ok(());
    }
    stop(); // a thread that ended on its own (the server went away)
    let display = Arc::new(Display::open()?);
    let wake = display.helper_window(EventMask::NO_EVENT)?;
    // Without this, a held key repeats as release + press pairs, which would end the grab and
    // start the shortcut again on every repeat.
    let detectable = display.conn.xkb_use_extension(1, 0).x().and_then(|c| c.reply().x()).is_ok_and(|r| r.supported)
        && display
            .conn
            .xkb_per_client_flags(
                xkb::ID::USE_CORE_KBD.into(),
                xkb::PerClientFlag::DETECTABLE_AUTO_REPEAT,
                xkb::PerClientFlag::DETECTABLE_AUTO_REPEAT,
                xkb::BoolCtrl::from(0u32),
                xkb::BoolCtrl::from(0u32),
                xkb::BoolCtrl::from(0u32),
            )
            .x()
            .and_then(|c| c.reply().x())
            .is_ok_and(|r| u32::from(r.value) & u32::from(xkb::PerClientFlag::DETECTABLE_AUTO_REPEAT) != 0);
    if !detectable {
        log::warn!("the X server has no detectable auto-repeat; holding a key shortcut may restart it");
    }
    let keymap = Keymap::load(&display.conn)?;
    STOP.store(false, Ordering::SeqCst);
    let worker = display.clone();
    let thread = std::thread::Builder::new()
        .name("talkr-hotkeys".into())
        .spawn(move || {
            let mut listener = Listener::new(&worker, wake, keymap);
            listener.run();
            listener.release_all();
            log::info!("dictation shortcut listener stopped");
        })
        .map_err(|e| e.to_string())?;
    *RUNNING.lock().unwrap_or_else(|e| e.into_inner()) = Some(Running { display, wake, thread });
    log::info!("dictation shortcut listener started (X11)");
    Ok(())
}

/// Stop listening and wait for the thread to end; every grab goes with it.
pub fn stop() {
    STOP.store(true, Ordering::SeqCst);
    wake();
    let running = RUNNING.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some(r) = running {
        let _ = r.thread.join();
    }
}

struct Listener<'a> {
    d: &'a Display,
    wake: Window,
    keymap: Keymap,
    config: Config,
    /// Passive grabs in place: keycode and modifier mask.
    grabbed: BTreeSet<(u8, u16)>,
    machine: Machine,
    /// Recording a new shortcut, with the keyboard grabbed.
    capture: Option<Capture>,
}

impl<'a> Listener<'a> {
    fn new(d: &'a Display, wake: Window, keymap: Keymap) -> Self {
        Self { d, wake, keymap, config: Config::default(), grabbed: BTreeSet::new(), machine: Machine::default(), capture: None }
    }

    fn run(&mut self) {
        self.apply();
        loop {
            let event = match self.d.conn.wait_for_event() {
                Ok(event) => event,
                Err(e) => {
                    log::warn!("the shortcut listener's X connection closed: {}", e);
                    return;
                }
            };
            match event {
                Event::KeyPress(e) => self.key(&e, true),
                Event::KeyRelease(e) => self.key(&e, false),
                Event::ClientMessage(m) if m.window == self.wake => {
                    if STOP.load(Ordering::SeqCst) {
                        return;
                    }
                    self.apply();
                }
                Event::MappingNotify(m) if m.request != Mapping::POINTER => match Keymap::load(&self.d.conn) {
                    Ok(keymap) => {
                        self.keymap = keymap;
                        self.apply();
                    }
                    Err(e) => log::warn!("could not reload the keyboard map: {}", e),
                },
                Event::Error(e) => log::debug!("X error in the shortcut listener: {:?}", e.error_kind),
                _ => {}
            }
            let _ = self.d.conn.flush();
        }
    }

    fn key(&mut self, e: &KeyPressEvent, down: bool) {
        let vk = self.keymap.vk(e.detail);
        if let Some(capture) = self.capture.as_mut() {
            if let Some(event) = capture.key(vk, down) {
                self.end_capture();
                emit(event);
            }
            return;
        }
        let verdict = if input::injecting() && !self.machine.grabbed() {
            // Talkr's own paste or typing matched a grab: it belongs to the app.
            Verdict::Replay
        } else {
            let mods = self.keymap.mods_of(u16::from(e.state));
            let mut out = Vec::new();
            let verdict = self.machine.key(&self.config, vk, down, mods, &mut out);
            out.into_iter().for_each(emit);
            verdict
        };
        let mode = match verdict {
            Verdict::Keep => Allow::SYNC_KEYBOARD,
            Verdict::Replay => Allow::REPLAY_KEYBOARD,
        };
        let _ = self.d.conn.allow_events(mode, x11rb::CURRENT_TIME);
        let _ = self.d.conn.flush();
    }

    /// Bring the grabs in line with the settings.
    fn apply(&mut self) {
        self.config = CONFIG.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let mut wanted = BTreeSet::new();
        for grab in chord::grabs(&self.config) {
            let mods = self.keymap.mask_of(grab.mods);
            for keycode in self.keymap.keycodes_for_vk(grab.vk) {
                for lock in self.keymap.lock_masks() {
                    wanted.insert((keycode, mods | lock));
                }
            }
        }
        for &(keycode, mask) in self.grabbed.difference(&wanted) {
            let _ = self.d.conn.ungrab_key(keycode, self.d.root, ModMask::from(mask));
        }
        let mut refused = 0;
        for &(keycode, mask) in wanted.difference(&self.grabbed) {
            let checked = self
                .d
                .conn
                .grab_key(false, self.d.root, ModMask::from(mask), keycode, GrabMode::ASYNC, GrabMode::SYNC)
                .x()
                .and_then(|cookie| cookie.check().x());
            if checked.is_err() {
                refused += 1;
            }
        }
        if refused > 0 {
            // BadAccess: another program grabs the same keys. The others still work.
            log::warn!("{} of the shortcut's key combinations are taken by another program", refused);
        }
        self.grabbed = wanted;

        let capturing = CAPTURING.load(Ordering::SeqCst);
        if capturing && self.capture.is_none() {
            if self.grab_keyboard() {
                self.capture = Some(Capture::default());
            } else {
                log::warn!("could not take the keyboard to record a shortcut");
                CAPTURING.store(false, Ordering::SeqCst);
                emit(HotkeyEvent::CaptureCancelled);
            }
        } else if !capturing && self.capture.is_some() {
            self.end_capture();
        }
        let _ = self.d.conn.flush();
    }

    /// The whole keyboard, only while the settings page records a shortcut. Another program
    /// (an open menu) may hold it for a moment: retry briefly.
    fn grab_keyboard(&mut self) -> bool {
        if self.machine.grabbed() {
            // Let a grab of Talkr's own run its course first.
            let _ = self.d.conn.allow_events(Allow::ASYNC_KEYBOARD, x11rb::CURRENT_TIME);
            self.machine = Machine::default();
        }
        for _ in 0..20 {
            let status = self
                .d
                .conn
                .grab_keyboard(false, self.d.root, x11rb::CURRENT_TIME, GrabMode::ASYNC, GrabMode::ASYNC)
                .x()
                .and_then(|c| c.reply().x())
                .map(|r| r.status);
            match status {
                Ok(GrabStatus::SUCCESS) => return true,
                Ok(_) => std::thread::sleep(Duration::from_millis(25)),
                Err(_) => return false,
            }
        }
        false
    }

    fn end_capture(&mut self) {
        self.capture = None;
        CAPTURING.store(false, Ordering::SeqCst);
        let _ = self.d.conn.ungrab_keyboard(x11rb::CURRENT_TIME);
        let _ = self.d.conn.flush();
    }

    fn release_all(&mut self) {
        for &(keycode, mask) in &self.grabbed {
            let _ = self.d.conn.ungrab_key(keycode, self.d.root, ModMask::from(mask));
        }
        self.grabbed.clear();
        if self.machine.grabbed() {
            let _ = self.d.conn.allow_events(Allow::ASYNC_KEYBOARD, x11rb::CURRENT_TIME);
        }
        if self.capture.take().is_some() {
            let _ = self.d.conn.ungrab_keyboard(x11rb::CURRENT_TIME);
        }
        let _ = self.d.conn.flush();
    }
}
