//! The shortcut on Wayland, through the GlobalShortcuts portal. Talkr asks the desktop to bind
//! "dictate" (and "paste-last"); the desktop reports Activated and Deactivated, which are the
//! press and the release, so hold-to-talk works.
//!
//! The desktop owns the binding: it may ask the user to confirm it, the user changes it in the
//! system settings, and Talkr only suggests a trigger. One portal session lives on the portal
//! runtime while dictation is on; a new shortcut replaces the session.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
use futures_util::StreamExt;
use tokio::sync::mpsc;
use super::portal::{self, describe, lock};
use super::trigger::{trigger, Trigger};
use crate::dictation::settings::Shortcut;
use crate::dictation::HotkeyEvent;

pub const DICTATE: &str = "dictate";
const PASTE_LAST: &str = "paste-last";

/// Which bound shortcuts are held right now (bit 0: dictate, bit 1: paste-last).
static HELD: AtomicU8 = AtomicU8::new(0);

fn bit(id: &str) -> u8 {
    match id {
        DICTATE => 1,
        PASTE_LAST => 2,
        _ => 0,
    }
}

/// What Talkr asks the desktop to bind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub id: &'static str,
    pub description: &'static str,
    pub trigger: Trigger,
}

/// The bindings for the configured shortcuts.
pub fn bindings(dictate: Option<&Shortcut>, paste_last: Option<&Shortcut>) -> Vec<Binding> {
    let mut out = Vec::new();
    if let Some(s) = dictate {
        out.push(Binding { id: DICTATE, description: "Dictate (hold to talk)", trigger: trigger(s) });
    }
    if let Some(s) = paste_last {
        out.push(Binding { id: PASTE_LAST, description: "Paste the last dictation again", trigger: trigger(s) });
    }
    out
}

/// What the last session found out, for the settings page.
#[derive(Debug, Clone, Default)]
pub struct Report {
    /// How the desktop describes each bound trigger, by id ("Ctrl+Super+Space"); empty until the
    /// user confirmed the binding.
    pub bound: Vec<(String, String)>,
    pub error: Option<String>,
}

enum Command {
    Stop,
    /// Open the desktop's dialog for changing the shortcuts.
    Configure,
}

struct Listener {
    bindings: Vec<Binding>,
    commands: mpsc::UnboundedSender<Command>,
    /// Disconnects when the session task ends.
    done: flume::Receiver<()>,
}

impl Listener {
    fn alive(&self) -> bool {
        !self.done.is_disconnected()
    }

    fn stop(self) {
        let _ = self.commands.send(Command::Stop);
        // Closing the session is one D-Bus call; do not hang the caller on a stuck bus.
        let _ = self.done.recv_timeout(Duration::from_secs(2));
    }
}

struct State {
    events: Option<flume::Sender<HotkeyEvent>>,
    /// `start` was called and `stop` was not.
    wanted: bool,
    bindings: Vec<Binding>,
    listener: Option<Listener>,
    /// The current (or last) session's report; each session has its own, so one that is ending
    /// cannot overwrite the next one's.
    report: Option<Arc<Mutex<Report>>>,
}

static STATE: Mutex<State> = Mutex::new(State { events: None, wanted: false, bindings: Vec::new(), listener: None, report: None });

pub fn start(events: flume::Sender<HotkeyEvent>) {
    let mut state = lock(&STATE);
    state.events = Some(events);
    state.wanted = true;
    sync(&mut state);
}

pub fn stop() {
    let mut state = lock(&STATE);
    state.wanted = false;
    sync(&mut state);
}

pub fn configure(bindings: Vec<Binding>) {
    let mut state = lock(&STATE);
    state.bindings = bindings;
    sync(&mut state);
}

/// A session is listening (or waiting for the user to confirm the binding).
pub fn running() -> bool {
    lock(&STATE).listener.as_ref().is_some_and(Listener::alive)
}

pub fn report() -> Report {
    let report = lock(&STATE).report.clone();
    report.map(|r| lock(&r).clone()).unwrap_or_default()
}

/// The configured dictation trigger, for the settings page until the desktop reports its own.
pub fn requested() -> Option<Trigger> {
    lock(&STATE).bindings.iter().find(|b| b.id == DICTATE).map(|b| b.trigger.clone())
}

/// Talkr cannot record a shortcut here: the desktop chooses it. The page's request is answered
/// at once, and the desktop's own dialog for it opens where the portal has one.
pub fn capture(capturing: bool) {
    if !capturing {
        return;
    }
    let state = lock(&STATE);
    if let Some(events) = &state.events {
        let _ = events.send(HotkeyEvent::CaptureCancelled);
    }
    if let Some(listener) = state.listener.as_ref().filter(|l| l.alive()) {
        let _ = listener.commands.send(Command::Configure);
    }
}

/// Wait until no bound shortcut is held, at most `limit`: keys pressed for pasting while the
/// user still holds Alt + Shift (the paste-again shortcut) would arrive as another shortcut.
pub fn wait_released(limit: Duration) {
    let deadline = Instant::now() + limit;
    while HELD.load(Ordering::SeqCst) != 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(15));
    }
}

/// Bring the session in line with what is wanted: none when stopped or nothing is configured,
/// otherwise one for exactly the configured bindings.
fn sync(state: &mut State) {
    let desired = state.wanted && !state.bindings.is_empty();
    let current_ok = state.listener.as_ref().is_some_and(|l| l.alive() && l.bindings == state.bindings);
    if desired && current_ok {
        return;
    }
    if let Some(old) = state.listener.take() {
        old.stop();
        HELD.store(0, Ordering::SeqCst);
    }
    if !desired {
        // An old session's failure says nothing about the next one.
        state.report = None;
        return;
    }
    let Some(events) = state.events.clone() else { return };
    let (commands, inbox) = mpsc::unbounded_channel();
    let (done_tx, done) = flume::bounded::<()>(0);
    let report = Arc::new(Mutex::new(Report::default()));
    let bindings = state.bindings.clone();
    let task = listen(bindings.clone(), events, inbox, report.clone(), done_tx);
    if portal::spawn(task) {
        state.listener = Some(Listener { bindings, commands, done });
    } else {
        lock(&report).error = Some("the portal runtime is not running".into());
    }
    state.report = Some(report);
}

async fn listen(
    bindings: Vec<Binding>,
    events: flume::Sender<HotkeyEvent>,
    mut commands: mpsc::UnboundedReceiver<Command>,
    report: Arc<Mutex<Report>>,
    _done: flume::Sender<()>,
) {
    if let Err(e) = session(&bindings, &events, &mut commands, &report).await {
        log::warn!("dictation: global shortcuts: {}", e);
        lock(&report).error = Some(e);
    }
    HELD.store(0, Ordering::SeqCst);
}

async fn session(
    bindings: &[Binding],
    events: &flume::Sender<HotkeyEvent>,
    commands: &mut mpsc::UnboundedReceiver<Command>,
    report: &Mutex<Report>,
) -> Result<(), String> {
    portal::ready().await;
    let fail = |what: &str, e: ashpd::Error| format!("could not {} ({})", what, describe(&e));
    let portal = GlobalShortcuts::new().await.map_err(|e| fail("reach the global-shortcuts portal", e))?;
    let session = portal.create_session(Default::default()).await.map_err(|e| fail("start a shortcut session", e))?;
    // Listening before binding: the first press can come right after the user confirms.
    let activated = portal.receive_activated().await.map_err(|e| fail("listen for the shortcut", e))?;
    let deactivated = portal.receive_deactivated().await.map_err(|e| fail("listen for the shortcut", e))?;
    let changed = portal.receive_shortcuts_changed().await.map_err(|e| fail("listen for the shortcut", e))?;
    let closed = session.receive_closed().await.map_err(|e| fail("watch the shortcut session", e))?;
    let mut activated = std::pin::pin!(activated);
    let mut deactivated = std::pin::pin!(deactivated);
    let mut changed = std::pin::pin!(changed);
    let mut closed = std::pin::pin!(closed);

    let shortcuts: Vec<NewShortcut> = bindings
        .iter()
        .map(|b| NewShortcut::new(b.id, b.description).preferred_trigger(b.trigger.spec.as_str()))
        .collect();
    // The desktop may show a dialog first. Stopping meanwhile closes the session, and its dialog.
    let bind = async {
        let request = portal.bind_shortcuts(&session, &shortcuts, None, Default::default()).await?;
        request.response()
    };
    let bound = tokio::select! {
        bound = bind => bound,
        _ = stopped(commands) => {
            let _ = session.close().await;
            return Ok(());
        }
    };
    match bound {
        Ok(bound) => {
            let described: Vec<(String, String)> =
                bound.shortcuts().iter().map(|s| (s.id().to_string(), s.trigger_description().to_string())).collect();
            log::info!("dictation: the desktop bound {:?}", described);
            lock(report).bound = described;
        }
        Err(e) => {
            let _ = session.close().await;
            return Err(fail("bind the shortcut", e));
        }
    }

    let mut pressed = Presses::default();
    loop {
        tokio::select! {
            Some(signal) = activated.next() => {
                for event in pressed.activated(signal.shortcut_id()) {
                    let _ = events.send(event);
                }
            }
            Some(signal) = deactivated.next() => {
                for event in pressed.deactivated(signal.shortcut_id()) {
                    let _ = events.send(event);
                }
            }
            Some(signal) = changed.next() => {
                lock(report).bound =
                    signal.shortcuts().iter().map(|s| (s.id().to_string(), s.trigger_description().to_string())).collect();
            }
            _ = closed.next() => return Err("the desktop ended the shortcut session".into()),
            command = commands.recv() => match command {
                Some(Command::Configure) => {
                    if let Err(e) = portal.configure_shortcuts(&session, None, Default::default()).await {
                        log::info!("dictation: could not open the desktop's shortcut settings ({})", describe(&e));
                    }
                }
                Some(Command::Stop) | None => {
                    let _ = session.close().await;
                    return Ok(());
                }
            },
        }
    }
}

/// Resolves when the session should stop (other commands wait until the binding is done).
async fn stopped(commands: &mut mpsc::UnboundedReceiver<Command>) {
    loop {
        match commands.recv().await {
            Some(Command::Stop) | None => return,
            Some(Command::Configure) => {}
        }
    }
}

/// Turns the portal's Activated and Deactivated into the engine's events.
///
/// A desktop that never reports releases (Deactivated is optional in the portal) cannot do
/// hold-to-talk. That shows when the shortcut is pressed again while still "held": that press
/// ends the dictation, and later presses are taps (start and lock, then stop), so the shortcut
/// starts and stops dictation instead of restarting it on every press.
#[derive(Debug, Default)]
struct Presses {
    dictate_down: bool,
    /// A release was reported in this session.
    releases: bool,
    /// Releases are not reported here.
    no_releases: bool,
}

impl Presses {
    fn activated(&mut self, id: &str) -> Vec<HotkeyEvent> {
        if id != DICTATE {
            HELD.fetch_or(bit(id), Ordering::SeqCst);
            return if id == PASTE_LAST { vec![HotkeyEvent::PasteLast] } else { Vec::new() };
        }
        if self.no_releases {
            return vec![HotkeyEvent::DictateDown, HotkeyEvent::DictateUp];
        }
        if std::mem::take(&mut self.dictate_down) {
            if self.releases {
                // One release went missing: end that press first, as the keyboard would have.
                self.dictate_down = true;
                return vec![HotkeyEvent::DictateUp, HotkeyEvent::DictateDown];
            }
            log::info!("dictation: the desktop does not report the shortcut's release; it starts and stops dictation");
            self.no_releases = true;
            HELD.fetch_and(!bit(id), Ordering::SeqCst);
            return vec![HotkeyEvent::DictateUp];
        }
        HELD.fetch_or(bit(id), Ordering::SeqCst);
        self.dictate_down = true;
        vec![HotkeyEvent::DictateDown]
    }

    fn deactivated(&mut self, id: &str) -> Vec<HotkeyEvent> {
        HELD.fetch_and(!bit(id), Ordering::SeqCst);
        if id != DICTATE {
            return Vec::new();
        }
        self.releases = true;
        if self.no_releases {
            // Releases do come after all: back to hold-to-talk from the next press.
            self.no_releases = false;
            return Vec::new();
        }
        if std::mem::take(&mut self.dictate_down) {
            vec![HotkeyEvent::DictateUp]
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn press_and_release_become_down_and_up() {
        let mut p = Presses::default();
        assert_eq!(p.activated(DICTATE), vec![HotkeyEvent::DictateDown]);
        assert_eq!(p.deactivated(DICTATE), vec![HotkeyEvent::DictateUp]);
        // A second release (or one for a press Talkr never saw) is not reported.
        assert!(p.deactivated(DICTATE).is_empty());
    }

    #[test]
    fn a_lost_release_is_made_up() {
        let mut p = Presses::default();
        p.activated(DICTATE);
        p.deactivated(DICTATE);
        p.activated(DICTATE);
        assert_eq!(p.activated(DICTATE), vec![HotkeyEvent::DictateUp, HotkeyEvent::DictateDown]);
        assert_eq!(p.deactivated(DICTATE), vec![HotkeyEvent::DictateUp]);
    }

    #[test]
    fn without_releases_presses_start_and_stop() {
        let mut p = Presses::default();
        assert_eq!(p.activated(DICTATE), vec![HotkeyEvent::DictateDown]);
        // Pressed again, never released: this press ends the dictation, and does not start one.
        assert_eq!(p.activated(DICTATE), vec![HotkeyEvent::DictateUp]);
        // From now on each press is a tap (the engine locks a tapped dictation, then stops it).
        assert_eq!(p.activated(DICTATE), vec![HotkeyEvent::DictateDown, HotkeyEvent::DictateUp]);
        assert_eq!(p.activated(DICTATE), vec![HotkeyEvent::DictateDown, HotkeyEvent::DictateUp]);
        // A release after all: hold-to-talk again.
        assert!(p.deactivated(DICTATE).is_empty());
        assert_eq!(p.activated(DICTATE), vec![HotkeyEvent::DictateDown]);
        assert_eq!(p.deactivated(DICTATE), vec![HotkeyEvent::DictateUp]);
    }

    #[test]
    fn paste_last_fires_on_press_only() {
        let mut p = Presses::default();
        assert_eq!(p.activated(PASTE_LAST), vec![HotkeyEvent::PasteLast]);
        assert!(p.deactivated(PASTE_LAST).is_empty());
        assert!(p.activated("something-else").is_empty());
        assert!(p.deactivated("something-else").is_empty());
    }

    #[test]
    fn bindings_follow_the_settings() {
        assert!(bindings(None, None).is_empty());
        let both = bindings(Some(&Shortcut::ctrl_win()), Some(&Shortcut::alt_shift_v()));
        assert_eq!(both.len(), 2);
        assert_eq!((both[0].id, both[0].trigger.spec.as_str()), (DICTATE, "CTRL+LOGO+space"));
        assert_eq!((both[1].id, both[1].trigger.spec.as_str()), (PASTE_LAST, "ALT+SHIFT+v"));
        let dictate_only = bindings(Some(&Shortcut::ctrl_win()), None);
        assert_eq!(dictate_only.len(), 1);
        assert_ne!(dictate_only, both);
    }

    #[test]
    fn ids_have_their_own_bits() {
        assert_eq!(bit(DICTATE) & bit(PASTE_LAST), 0);
        assert_eq!(bit("other"), 0);
    }
}
