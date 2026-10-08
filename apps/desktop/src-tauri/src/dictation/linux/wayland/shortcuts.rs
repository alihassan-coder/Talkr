//! The shortcut on Wayland, through the GlobalShortcuts portal. Talkr asks the desktop to bind
//! "dictate" (and "paste-last"); the desktop reports Activated and Deactivated, which are the
//! press and the release, so hold-to-talk works (where the desktop reports releases; see
//! `Presses`).
//!
//! The desktop owns the binding: it may ask the user to confirm it, the user changes it in the
//! system settings, and Talkr only suggests a trigger. One portal session lives on the portal
//! runtime while dictation is on; a new shortcut replaces the session. A binding the user
//! declined is not asked for again until the shortcut changes or the user asks (Allow, or Change
//! shortcut), so the dialog does not come back on every settings change.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
use ashpd::desktop::ResponseError;
use futures_util::StreamExt;
use tokio::sync::mpsc;
use super::portal::{self, describe, lock};
use super::trigger::{trigger, Trigger};
use crate::dictation::settings::Shortcut;
use crate::dictation::HotkeyEvent;

pub const DICTATE: &str = "dictate";
const PASTE_LAST: &str = "paste-last";

/// Activations closer together than this are the key repeating while held, not new presses.
/// Desktops start repeating after 400 to 660 ms (GNOME 500, KDE and sway 600, Xorg 660).
const REPEAT_GAP: Duration = Duration::from_millis(750);

/// The bound shortcuts the portal reports as held (see `Held`).
static HELD: Mutex<Held> = Mutex::new(Held { since: [None, None] });
/// The desktop does not report releases, so the shortcut starts and stops dictation. Learned
/// once per run: a new session (a changed shortcut) does not have to learn it again.
static NO_RELEASES: AtomicBool = AtomicBool::new(false);
/// A dictation is recording (the engine's `set_recording`): without releases, a press while
/// recording finishes it and a press otherwise starts one.
static RECORDING: AtomicBool = AtomicBool::new(false);

/// When each bound shortcut was pressed, while the portal has not reported its release. This is
/// only the portal's view: it says nothing about modifiers (a desktop may report the release
/// when the key goes up while Alt + Shift are still held), and a desktop without releases would
/// never clear it, so a press only counts for a little while.
#[derive(Debug)]
struct Held {
    since: [Option<Instant>; 2],
}

impl Held {
    fn slot(id: &str) -> Option<usize> {
        match id {
            DICTATE => Some(0),
            PASTE_LAST => Some(1),
            _ => None,
        }
    }

    fn press(&mut self, id: &str, at: Instant) {
        if let Some(i) = Self::slot(id) {
            self.since[i] = Some(at);
        }
    }

    fn release(&mut self, id: &str) {
        if let Some(i) = Self::slot(id) {
            self.since[i] = None;
        }
    }

    fn clear(&mut self) {
        self.since = [None, None];
    }

    /// A shortcut was pressed less than `limit` ago and not released since.
    fn busy(&self, now: Instant, limit: Duration) -> bool {
        self.since.iter().flatten().any(|at| now.saturating_duration_since(*at) < limit)
    }
}

/// The desktop does not report the shortcut's release: hold-to-talk cannot work here.
pub fn no_releases() -> bool {
    NO_RELEASES.load(Ordering::SeqCst)
}

pub fn set_recording(recording: bool) {
    RECORDING.store(recording, Ordering::SeqCst);
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
    /// The user declined the binding in the desktop's dialog.
    pub declined: bool,
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
    /// Bindings the user declined: not asked for again until they change or the user asks.
    declined: Option<Vec<Binding>>,
}

static STATE: Mutex<State> =
    Mutex::new(State { events: None, wanted: false, bindings: Vec::new(), listener: None, report: None, declined: None });

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
/// at once, and the desktop's own dialog for it opens where the portal has one. When no session
/// is listening (the user declined the binding, say), the binding is asked for again.
pub fn capture(capturing: bool) {
    if !capturing {
        return;
    }
    let mut state = lock(&STATE);
    if let Some(events) = &state.events {
        let _ = events.send(HotkeyEvent::CaptureCancelled);
    }
    let listening = state.listener.as_ref().filter(|l| l.alive()).map(|l| l.commands.clone());
    match listening {
        Some(commands) => {
            let _ = commands.send(Command::Configure);
        }
        None => ask_again(&mut state),
    }
}

/// The user asked for the shortcut (Allow on the settings page): bind it again even if they
/// declined it before.
pub fn retry() {
    ask_again(&mut lock(&STATE));
}

fn ask_again(state: &mut State) {
    harvest(state);
    state.declined = None;
    if !state.listener.as_ref().is_some_and(Listener::alive) {
        state.listener = None;
        sync(state);
    }
}

/// Wait until no bound shortcut is held, at most `limit`: keys pressed for pasting while the
/// user still holds Alt + Shift (the paste-again shortcut) would arrive as another shortcut.
/// The portal cannot tell about modifiers, so this only gives the user a moment to let go.
pub fn wait_released(limit: Duration) {
    let deadline = Instant::now() + limit;
    while lock(&HELD).busy(Instant::now(), limit) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(15));
    }
}

/// What to do with the shortcut session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Next {
    Keep,
    Stop,
    Start,
}

/// `listening`: a session is alive for exactly the configured bindings. `declined`: the user
/// declined exactly these bindings.
fn next(desired: bool, listening: bool, declined: bool) -> Next {
    match (desired, listening, declined) {
        (true, true, _) => Next::Keep,
        // Asking again would show the dialog the user just dismissed, on every settings change.
        (true, false, true) => Next::Keep,
        (true, false, false) => Next::Start,
        (false, ..) => Next::Stop,
    }
}

/// Remember the bindings an ended session's user declined.
fn harvest(state: &mut State) {
    let Some(listener) = state.listener.as_ref().filter(|l| !l.alive()) else { return };
    if state.report.as_ref().is_some_and(|r| lock(r).declined) {
        state.declined = Some(listener.bindings.clone());
    }
}

/// Bring the session in line with what is wanted: none when stopped or nothing is configured,
/// otherwise one for exactly the configured bindings (unless the user declined those).
fn sync(state: &mut State) {
    harvest(state);
    let desired = state.wanted && !state.bindings.is_empty();
    let listening = state.listener.as_ref().is_some_and(|l| l.alive() && l.bindings == state.bindings);
    let declined = state.declined.as_ref() == Some(&state.bindings);
    match next(desired, listening, declined) {
        Next::Keep if listening => return,
        Next::Keep => {
            // Keep the reason on the page (start_hotkeys reports it), without asking again.
            if !state.report.as_ref().is_some_and(|r| lock(r).declined) {
                state.report = Some(Arc::new(Mutex::new(Report {
                    error: Some("you declined the shortcut in the desktop's dialog (click Change shortcut to be asked again)".into()),
                    declined: true,
                    ..Report::default()
                })));
            }
            if let Some(old) = state.listener.take() {
                old.stop();
            }
            return;
        }
        Next::Stop | Next::Start => {}
    }
    if let Some(old) = state.listener.take() {
        old.stop();
        lock(&HELD).clear();
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
    lock(&HELD).clear();
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
            if matches!(e, ashpd::Error::Response(ResponseError::Cancelled)) {
                lock(report).declined = true;
            }
            return Err(fail("bind the shortcut", e));
        }
    }

    let mut pressed = Presses { no_releases: no_releases(), ..Presses::default() };
    loop {
        tokio::select! {
            Some(signal) = activated.next() => {
                let (id, now) = (signal.shortcut_id(), Instant::now());
                let out = pressed.activated(id, now, RECORDING.load(Ordering::SeqCst));
                // Without releases a press would count as held for good.
                if !out.is_empty() && !pressed.no_releases {
                    lock(&HELD).press(id, now);
                }
                NO_RELEASES.store(pressed.no_releases, Ordering::SeqCst);
                for event in out {
                    let _ = events.send(event);
                }
            }
            Some(signal) = deactivated.next() => {
                let id = signal.shortcut_id();
                lock(&HELD).release(id);
                let out = pressed.deactivated(id);
                NO_RELEASES.store(pressed.no_releases, Ordering::SeqCst);
                for event in out {
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
/// hold-to-talk. That shows when the shortcut is pressed again while still "held", well after
/// the first press (activations close together are the key repeating, not a press). From then
/// on the shortcut toggles: a press while nothing records starts a dictation (`DictateDown`), a
/// press while one records finishes it (`DictateDown` then `DictateUp`, which ends it in every
/// activation mode: the engine finishes on the down in Toggle mode and on the up otherwise, a
/// whole dictation after it started, so Hold mode does not drop it as too short). Following the
/// engine's recording state rather than counting presses keeps the two in step when a dictation
/// ended some other way (the pill, an error).
#[derive(Debug, Default)]
struct Presses {
    dictate_down: bool,
    /// A release was reported in this session.
    releases: bool,
    /// Releases are not reported here.
    no_releases: bool,
    /// The last activation of the dictation shortcut, repeats included.
    last: Option<Instant>,
}

impl Presses {
    fn activated(&mut self, id: &str, now: Instant, recording: bool) -> Vec<HotkeyEvent> {
        if id != DICTATE {
            return if id == PASTE_LAST { vec![HotkeyEvent::PasteLast] } else { Vec::new() };
        }
        let repeat = self.last.is_some_and(|at| now.saturating_duration_since(at) < REPEAT_GAP);
        self.last = Some(now);
        if self.no_releases {
            return if repeat { Vec::new() } else { toggle(recording) };
        }
        if self.dictate_down {
            if repeat {
                return Vec::new();
            }
            if self.releases {
                // One release went missing: end that press first, as the keyboard would have.
                return vec![HotkeyEvent::DictateUp, HotkeyEvent::DictateDown];
            }
            log::info!("dictation: the desktop does not report the shortcut's release; it starts and stops dictation");
            self.no_releases = true;
            self.dictate_down = false;
            return toggle(recording);
        }
        self.dictate_down = true;
        vec![HotkeyEvent::DictateDown]
    }

    fn deactivated(&mut self, id: &str) -> Vec<HotkeyEvent> {
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

/// One press of a shortcut whose release is never reported.
fn toggle(recording: bool) -> Vec<HotkeyEvent> {
    if recording {
        vec![HotkeyEvent::DictateDown, HotkeyEvent::DictateUp]
    } else {
        vec![HotkeyEvent::DictateDown]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    #[test]
    fn press_and_release_become_down_and_up() {
        let (mut p, t) = (Presses::default(), Instant::now());
        assert_eq!(p.activated(DICTATE, t, false), vec![HotkeyEvent::DictateDown]);
        assert_eq!(p.deactivated(DICTATE), vec![HotkeyEvent::DictateUp]);
        // A second release (or one for a press Talkr never saw) is not reported.
        assert!(p.deactivated(DICTATE).is_empty());
    }

    #[test]
    fn a_lost_release_is_made_up() {
        let (mut p, t) = (Presses::default(), Instant::now());
        p.activated(DICTATE, t, false);
        p.deactivated(DICTATE);
        p.activated(DICTATE, ms(t, 2_000), false);
        assert_eq!(p.activated(DICTATE, ms(t, 5_000), true), vec![HotkeyEvent::DictateUp, HotkeyEvent::DictateDown]);
        assert_eq!(p.deactivated(DICTATE), vec![HotkeyEvent::DictateUp]);
    }

    #[test]
    fn key_repeat_is_not_a_press() {
        let (mut p, t) = (Presses::default(), Instant::now());
        assert_eq!(p.activated(DICTATE, t, false), vec![HotkeyEvent::DictateDown]);
        // Held: the desktop repeats the activation after its repeat delay, then quickly.
        for at in [600, 633, 666, 1_300, 2_000] {
            assert!(p.activated(DICTATE, ms(t, at), true).is_empty(), "{} ms", at);
        }
        assert!(!p.no_releases);
        assert_eq!(p.deactivated(DICTATE), vec![HotkeyEvent::DictateUp]);

        // Nor do repeats of a press whose release went missing restart the dictation.
        assert_eq!(p.activated(DICTATE, ms(t, 5_000), false), vec![HotkeyEvent::DictateDown]);
        assert!(p.activated(DICTATE, ms(t, 5_600), true).is_empty());
    }

    #[test]
    fn without_releases_presses_toggle() {
        let (mut p, t) = (Presses::default(), Instant::now());
        assert_eq!(p.activated(DICTATE, t, false), vec![HotkeyEvent::DictateDown]);
        // Pressed again a while later, never released: this press finishes the dictation.
        assert_eq!(p.activated(DICTATE, ms(t, 3_000), true), vec![HotkeyEvent::DictateDown, HotkeyEvent::DictateUp]);
        assert!(p.no_releases);
        // From now on one press starts a dictation and the next one finishes it, never both at
        // once (which Hold mode would drop as too short).
        assert_eq!(p.activated(DICTATE, ms(t, 6_000), false), vec![HotkeyEvent::DictateDown]);
        assert_eq!(p.activated(DICTATE, ms(t, 9_000), true), vec![HotkeyEvent::DictateDown, HotkeyEvent::DictateUp]);
        // A dictation that ended another way (the pill) does not throw the next press off.
        assert_eq!(p.activated(DICTATE, ms(t, 12_000), false), vec![HotkeyEvent::DictateDown]);
        // Repeats are ignored here too.
        assert!(p.activated(DICTATE, ms(t, 12_500), true).is_empty());
        // A release after all: hold-to-talk again.
        assert!(p.deactivated(DICTATE).is_empty());
        assert!(!p.no_releases);
        assert_eq!(p.activated(DICTATE, ms(t, 20_000), false), vec![HotkeyEvent::DictateDown]);
        assert_eq!(p.deactivated(DICTATE), vec![HotkeyEvent::DictateUp]);
    }

    #[test]
    fn a_learned_desktop_toggles_from_the_first_press() {
        let mut p = Presses { no_releases: true, ..Presses::default() };
        assert_eq!(p.activated(DICTATE, Instant::now(), false), vec![HotkeyEvent::DictateDown]);
    }

    #[test]
    fn paste_last_fires_on_press_only() {
        let (mut p, t) = (Presses::default(), Instant::now());
        assert_eq!(p.activated(PASTE_LAST, t, false), vec![HotkeyEvent::PasteLast]);
        assert!(p.deactivated(PASTE_LAST).is_empty());
        assert!(p.activated("something-else", t, false).is_empty());
        assert!(p.deactivated("something-else").is_empty());
    }

    #[test]
    fn a_held_shortcut_only_counts_for_a_while() {
        let (mut held, t) = (Held { since: [None, None] }, Instant::now());
        let limit = Duration::from_millis(1_500);
        assert!(!held.busy(t, limit));
        held.press(PASTE_LAST, t);
        assert!(held.busy(ms(t, 100), limit));
        // Never released (the desktop does not say): no longer waited for.
        assert!(!held.busy(ms(t, 1_600), limit));
        held.press(DICTATE, t);
        held.release(DICTATE);
        held.release(PASTE_LAST);
        assert!(!held.busy(ms(t, 10), limit));
        held.press("other", t);
        assert!(!held.busy(ms(t, 10), limit));
        held.press(DICTATE, t);
        held.clear();
        assert!(!held.busy(ms(t, 10), limit));
    }

    #[test]
    fn a_declined_binding_is_not_asked_again() {
        assert_eq!(next(true, true, false), Next::Keep);
        assert_eq!(next(true, false, false), Next::Start);
        // Declined: stay quiet until the bindings change (then `declined` no longer matches).
        assert_eq!(next(true, false, true), Next::Keep);
        assert_eq!(next(false, true, false), Next::Stop);
        assert_eq!(next(false, false, true), Next::Stop);
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
    fn ids_have_their_own_slots() {
        assert_ne!(Held::slot(DICTATE), Held::slot(PASTE_LAST));
        assert_eq!(Held::slot("other"), None);
    }
}
