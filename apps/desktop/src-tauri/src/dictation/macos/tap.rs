//! The global shortcut listener: a Core Graphics event tap on its own thread and run loop.
//!
//! An active tap sees every key press, release and modifier change in the session before apps
//! do, and can keep a key from them (the shortcut's own key, Escape while recording). macOS turns
//! a tap off when its callback is slow, so the callback only reads atomics, runs the `machine`
//! and sends to a channel; everything else happens on the dictation thread. It needs the
//! Accessibility permission: without it macOS refuses to create the tap.

use std::cell::RefCell;
use std::ffi::{c_ulong, c_void};
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use super::ffi::{self, CGEventRef};
use super::keymap;
use super::keys::{self, MARKER};
use super::machine::{modifier_bit, Chord, Config, Machine};
use crate::dictation::settings::Shortcut;
use crate::dictation::HotkeyEvent;

static DICTATE: AtomicU64 = AtomicU64::new(0);
static PASTE_LAST: AtomicU64 = AtomicU64::new(0);
static RECORDING: AtomicBool = AtomicBool::new(false);
static CAPTURING: AtomicBool = AtomicBool::new(false);
static RUNNING: AtomicBool = AtomicBool::new(false);
/// The dictation key shortcut is held, as the listener saw it (see `Machine::dictate_held`).
static DICTATE_HELD: AtomicBool = AtomicBool::new(false);
/// Asks the listener thread to end.
static STOP: AtomicBool = AtomicBool::new(false);
/// Dictation wants the listener: when it could not start for want of permission, it starts as
/// soon as permission is granted.
static WANTED: AtomicBool = AtomicBool::new(false);
static WAITING_FOR_PERMISSION: AtomicBool = AtomicBool::new(false);
/// The tap, for the callback to turn back on after macOS disabled it.
static TAP: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
/// The listener's run loop, retained; whoever takes it out releases it.
static RUN_LOOP: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static THREAD: Mutex<Option<std::thread::JoinHandle<()>>> = Mutex::new(None);
static EVENTS: OnceLock<flume::Sender<HotkeyEvent>> = OnceLock::new();

/// The listener could not start because Accessibility is not allowed.
pub const NEEDS_PERMISSION: &str = "Talkr needs the Accessibility permission to hear the dictation shortcut. \
     Allow Talkr in System Settings › Privacy & Security › Accessibility; dictation starts as soon as it is allowed.";

thread_local! {
    static MACHINE: RefCell<Machine> = RefCell::new(Machine::default());
    /// The label of the last key pressed while capturing, as the keyboard layout typed it.
    static CAPTURE_LABEL: RefCell<Option<(u16, String)>> = const { RefCell::new(None) };
}

pub fn configure(dictate: Option<&Shortcut>, paste_last: Option<&Shortcut>) {
    DICTATE.store(Chord::encode(dictate), Ordering::SeqCst);
    PASTE_LAST.store(Chord::encode(paste_last), Ordering::SeqCst);
}

pub fn set_recording(recording: bool) {
    RECORDING.store(recording, Ordering::SeqCst);
}

pub fn set_capturing(capturing: bool) {
    CAPTURING.store(capturing, Ordering::SeqCst);
}

pub fn is_running() -> bool {
    RUNNING.load(Ordering::SeqCst)
}

/// Whether the key of a key shortcut for dictation is held, as far as the listener knows. The
/// listener keeps that key from apps, so the system's key state may never show it down.
pub fn dictate_held() -> bool {
    DICTATE_HELD.load(Ordering::SeqCst)
}

fn config() -> Config {
    Config {
        dictate: Chord::decode(DICTATE.load(Ordering::Relaxed)),
        paste_last: Chord::decode(PASTE_LAST.load(Ordering::Relaxed)),
        recording: RECORDING.load(Ordering::SeqCst),
        capturing: CAPTURING.load(Ordering::SeqCst),
    }
}

fn emit(event: HotkeyEvent) {
    if let Some(tx) = EVENTS.get() {
        let _ = tx.try_send(event);
    }
}

/// The characters a key event types on the current layout.
fn typed(event: CGEventRef) -> String {
    let mut buf = [0u16; 8];
    let mut len: c_ulong = 0;
    // SAFETY: the buffer and its capacity are passed together; `event` is the callback's event.
    unsafe { ffi::CGEventKeyboardGetUnicodeString(event, buf.len() as c_ulong, &mut len, buf.as_mut_ptr()) };
    String::from_utf16_lossy(&buf[..(len as usize).min(buf.len())])
}

/// Whether the listener last saw the Mac key `code` go down.
fn was_down(code: u16) -> bool {
    MACHINE.with(|m| m.borrow().is_down(keymap::vk_from_mac(code)))
}

/// One event from the tap; returns whether to keep it from the apps.
fn on_event(kind: u32, event: CGEventRef) -> bool {
    // SAFETY: `event` is valid for the duration of the callback.
    let (marker, code, flags) = unsafe {
        (
            ffi::CGEventGetIntegerValueField(event, ffi::FIELD_SOURCE_USER_DATA),
            ffi::CGEventGetIntegerValueField(event, ffi::FIELD_KEYCODE) as u16,
            ffi::CGEventGetFlags(event),
        )
    };
    if marker == MARKER {
        return false;
    }
    let (vk, down) = match kind {
        ffi::KEY_DOWN => (keymap::vk_from_mac(code), true),
        ffi::KEY_UP => (keymap::vk_from_mac(code), false),
        ffi::FLAGS_CHANGED => match keymap::modifier_change(code, flags, was_down(code)) {
            Some(change) => change,
            None => return false, // Caps Lock, Fn
        },
        _ => return false,
    };
    let config = config();
    if config.capturing && down && modifier_bit(vk) == 0 {
        let label = keymap::label_from_typed(&typed(event), flags);
        CAPTURE_LABEL.with(|l| *l.borrow_mut() = label.map(|label| (vk, label)));
    }
    // Modifiers are read from this event's own flags (always current); other keys from the
    // keyboard state.
    let physically_down = |vk: u16| match keymap::modifier_down(vk, flags) {
        Some(down) => down,
        None => keymap::mac_from_vk(vk).is_none_or(keys::key_held),
    };
    let label = |vk: u16| {
        CAPTURE_LABEL
            .with(|l| l.borrow().as_ref().filter(|(k, _)| *k == vk).map(|(_, label)| label.clone()))
            .unwrap_or_else(|| keymap::key_label(vk))
    };
    let step = MACHINE.with(|m| {
        let mut machine = m.borrow_mut();
        let step = machine.handle(&config, vk, down, &physically_down, &label);
        DICTATE_HELD.store(machine.dictate_held(), Ordering::SeqCst);
        step
    });
    if step.capture_ended {
        CAPTURING.store(false, Ordering::SeqCst);
        CAPTURE_LABEL.with(|l| *l.borrow_mut() = None);
    }
    for event in step.events {
        emit(event);
    }
    step.swallow
}

unsafe extern "C" fn callback(_proxy: *mut c_void, kind: u32, event: CGEventRef, _info: *mut c_void) -> CGEventRef {
    if kind == ffi::TAP_DISABLED_BY_TIMEOUT || kind == ffi::TAP_DISABLED_BY_USER_INPUT {
        // macOS turned the tap off (a slow callback, or secure input): turn it back on. Releases
        // missed meanwhile are caught by the machine's check of what is really held. Without
        // permission (it was just taken away) the listener's loop ends it instead.
        let tap = TAP.load(Ordering::SeqCst);
        if !tap.is_null() && ffi::trusted() {
            // SAFETY: the tap stays valid while the listener thread, which runs this, lives.
            unsafe { ffi::CGEventTapEnable(tap, true) };
        }
        log::info!("macOS paused the dictation shortcut listener; resumed it");
        return event;
    }
    // A panic must not unwind into Core Graphics (that aborts the process).
    let swallow = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| on_event(kind, event))).unwrap_or(false);
    if swallow {
        std::ptr::null_mut()
    } else {
        event
    }
}

/// Start the listener thread (once; the event sender is fixed at the first start). Fails with a
/// message for the settings page when macOS does not allow it.
pub fn start(events: flume::Sender<HotkeyEvent>) -> Result<(), String> {
    let _lifecycle = lifecycle();
    WANTED.store(true, Ordering::SeqCst);
    let _ = EVENTS.set(events);
    launch()
}

/// Starts and stops happen one at a time: the permission watcher may start the listener while
/// dictation is being turned off.
fn lifecycle() -> std::sync::MutexGuard<'static, ()> {
    static LIFECYCLE: Mutex<()> = Mutex::new(());
    LIFECYCLE.lock().unwrap_or_else(|e| e.into_inner())
}

fn launch() -> Result<(), String> {
    if is_running() {
        return Ok(());
    }
    if !ffi::trusted() {
        wait_for_permission();
        return Err(NEEDS_PERMISSION.into());
    }
    // A previous thread that ended on its own (it could not create the tap) is joined first.
    if let Some(old) = THREAD.lock().unwrap_or_else(|e| e.into_inner()).take() {
        let _ = old.join();
    }
    STOP.store(false, Ordering::SeqCst);
    let (ready_tx, ready_rx) = flume::bounded::<Result<(), String>>(1);
    let thread = std::thread::Builder::new()
        .name("talkr-hotkeys".into())
        .spawn(move || run(ready_tx))
        .map_err(|e| e.to_string())?;
    *THREAD.lock().unwrap_or_else(|e| e.into_inner()) = Some(thread);
    ready_rx
        .recv_timeout(Duration::from_secs(5))
        .unwrap_or_else(|_| Err("The dictation shortcut listener did not start".into()))
}

fn run(ready: flume::Sender<Result<(), String>>) {
    let mask = (1u64 << ffi::KEY_DOWN) | (1u64 << ffi::KEY_UP) | (1u64 << ffi::FLAGS_CHANGED);
    // SAFETY: a session tap with a valid callback that lives for the whole program.
    let tap = unsafe {
        ffi::CGEventTapCreate(ffi::SESSION_EVENT_TAP, ffi::HEAD_INSERT, ffi::TAP_DEFAULT, mask, callback, std::ptr::null_mut())
    };
    if tap.is_null() {
        let _ = ready.send(Err(
            "macOS did not let Talkr listen for the dictation shortcut. Allow Talkr in System Settings › Privacy & \
             Security › Accessibility, then turn dictation off and on."
                .into(),
        ));
        return;
    }
    let mut revoked = false;
    // SAFETY: the tap is valid; the source and run loop belong to this thread, and everything
    // created here is released below, after the loop ends.
    unsafe {
        let source = ffi::CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);
        let run_loop = ffi::CFRunLoopGetCurrent();
        ffi::CFRunLoopAddSource(run_loop, source, ffi::kCFRunLoopCommonModes);
        ffi::CGEventTapEnable(tap, true);
        TAP.store(tap, Ordering::SeqCst);
        // Retained for `stop`, which may wake the loop from another thread.
        ffi::CFRetain(run_loop.cast_const());
        RUN_LOOP.store(run_loop, Ordering::SeqCst);
        RUNNING.store(true, Ordering::SeqCst);
        let _ = ready.send(Ok(()));
        log::info!("dictation shortcut listener started");

        while !STOP.load(Ordering::SeqCst) {
            ffi::CFRunLoopRunInMode(ffi::kCFRunLoopDefaultMode, 1.0, 0);
            if STOP.load(Ordering::SeqCst) {
                break;
            }
            // Checked every second: macOS can disable a tap without telling the callback, and
            // the user can take the permission away while Talkr runs.
            match health(ffi::trusted(), ffi::CGEventTapIsEnabled(tap)) {
                Health::Fine => {}
                Health::Disabled => {
                    log::info!("the dictation shortcut listener was disabled; turning it back on");
                    ffi::CGEventTapEnable(tap, true);
                }
                Health::Revoked => {
                    log::warn!("Accessibility was turned off for Talkr: the dictation shortcut stops until it is allowed again");
                    revoked = true;
                    break;
                }
            }
        }

        RUNNING.store(false, Ordering::SeqCst);
        DICTATE_HELD.store(false, Ordering::SeqCst);
        ffi::CGEventTapEnable(tap, false);
        TAP.store(std::ptr::null_mut(), Ordering::SeqCst);
        ffi::CFRunLoopRemoveSource(run_loop, source, ffi::kCFRunLoopCommonModes);
        ffi::CFMachPortInvalidate(tap);
        ffi::CFRelease(source.cast_const());
        ffi::CFRelease(tap.cast_const());
        let retained = RUN_LOOP.swap(std::ptr::null_mut(), Ordering::SeqCst);
        if !retained.is_null() {
            ffi::CFRelease(retained.cast_const());
        }
    }
    MACHINE.with(|m| *m.borrow_mut() = Machine::default());
    log::info!("dictation shortcut listener stopped");
    if revoked {
        // Dictation still wants the listener: start it again the moment permission is back.
        wait_for_permission();
    }
}

/// What the listener's once-a-second check found.
#[derive(Debug, PartialEq, Eq)]
enum Health {
    Fine,
    /// macOS turned the tap off: turn it back on.
    Disabled,
    /// Accessibility was taken away: the tap is dead (re-enabling it would do nothing, for
    /// good), so the listener ends and waits for the permission to come back.
    Revoked,
}

fn health(trusted: bool, enabled: bool) -> Health {
    match (trusted, enabled) {
        (false, _) => Health::Revoked,
        (true, false) => Health::Disabled,
        (true, true) => Health::Fine,
    }
}

/// Stop the listener and wait for it to end. Keys pass straight to apps again.
pub fn stop() {
    let _lifecycle = lifecycle();
    WANTED.store(false, Ordering::SeqCst);
    STOP.store(true, Ordering::SeqCst);
    let run_loop = RUN_LOOP.swap(std::ptr::null_mut(), Ordering::SeqCst);
    if !run_loop.is_null() {
        // SAFETY: we took over the reference the listener retained; stopping a run loop from
        // another thread is allowed.
        unsafe {
            ffi::CFRunLoopStop(run_loop);
            ffi::CFRelease(run_loop.cast_const());
        }
    }
    if let Some(thread) = THREAD.lock().unwrap_or_else(|e| e.into_inner()).take() {
        let _ = thread.join();
    }
}

/// Without permission the listener cannot start. Check now and then, and start it the moment the
/// user allows Talkr in System Settings, unless dictation was turned off meanwhile.
fn wait_for_permission() {
    if WAITING_FOR_PERMISSION.swap(true, Ordering::SeqCst) {
        return;
    }
    let spawned = std::thread::Builder::new().name("talkr-permission".into()).spawn(|| {
        while WANTED.load(Ordering::SeqCst) && !is_running() {
            std::thread::sleep(Duration::from_millis(1_500));
            if ffi::trusted() {
                let _lifecycle = lifecycle();
                if WANTED.load(Ordering::SeqCst) {
                    match launch() {
                        Ok(()) => log::info!("Accessibility allowed: the dictation shortcut works now"),
                        Err(e) => log::warn!("{}", e),
                    }
                }
                break;
            }
        }
        WAITING_FOR_PERMISSION.store(false, Ordering::SeqCst);
    });
    if spawned.is_err() {
        WAITING_FOR_PERMISSION.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::super::machine::{CTRL, WIN};
    use super::*;
    use std::time::Instant;

    /// The listener's settings are global: tests that touch them take turns.
    static LOCK: Mutex<()> = Mutex::new(());

    fn turn() -> std::sync::MutexGuard<'static, ()> {
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Without Accessibility, starting must fail at once with a message that says what to do,
    /// never hang.
    #[test]
    fn without_permission_start_explains_and_does_not_hang() {
        let _turn = turn();
        if ffi::trusted() {
            return;
        }
        let (tx, _rx) = flume::unbounded();
        let started = Instant::now();
        let error = start(tx).unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(error.contains("Accessibility"), "{error}");
        assert!(!is_running());
        stop();
        assert!(!WANTED.load(Ordering::SeqCst), "the permission watcher gives up once stopped");
    }

    /// A key event from an anonymous source (no marker), as the keyboard would send it.
    fn press(code: u16, down: bool) {
        // SAFETY: owned source and event, released when dropped; posting a live event.
        unsafe {
            let source = ffi::Cf::from_owned(ffi::CGEventSourceCreate(ffi::STATE_PRIVATE)).unwrap();
            let event = ffi::Cf::from_owned(ffi::CGEventCreateKeyboardEvent(source.as_ptr(), code, down)).unwrap();
            ffi::CGEventSetFlags(event.as_ptr().cast_mut(), 0);
            ffi::CGEventPost(ffi::HID_EVENT_TAP, event.as_ptr().cast_mut());
        }
    }

    /// The whole listener on a machine that allows it (a CI runner with Accessibility granted):
    /// the tap starts on its run loop, hears a key press and release that went through the
    /// system, and stops. F19 is used as the shortcut: it does nothing on its own.
    #[test]
    fn a_trusted_listener_hears_the_shortcut() {
        let _turn = turn();
        if std::env::var_os("CI").is_none() || !ffi::trusted() {
            return;
        }
        let _keyboard = super::super::pasteboard::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let f19 = Shortcut { ctrl: false, shift: false, alt: false, win: false, key: Some(0x82), key_label: None };
        let (tx, rx) = flume::unbounded();
        configure(Some(&f19), None);
        start(tx).expect("the listener starts");
        assert!(is_running());
        start(flume::unbounded().0).expect("starting again is harmless");
        press(0x50, true);
        let pressed = rx.recv_timeout(Duration::from_secs(3)).ok();
        let held = dictate_held();
        // Does the system's key state show a key the tap kept from apps? Logged for the record:
        // `shortcut_held` asks the listener instead, so it does not depend on the answer.
        {
            use std::io::Write;
            // SAFETY: plain queries.
            let (session, hid) = unsafe {
                (
                    ffi::CGEventSourceKeyState(ffi::STATE_COMBINED_SESSION, 0x50),
                    ffi::CGEventSourceKeyState(ffi::STATE_HID_SYSTEM, 0x50),
                )
            };
            let _ = writeln!(std::io::stderr(), "a swallowed (posted) key: combined session state {session}, HID state {hid}");
        }
        press(0x50, false);
        let released = rx.recv_timeout(Duration::from_secs(3)).ok();
        let held_after = dictate_held();
        stop();
        configure(None, None);
        assert!(!is_running());
        assert_eq!((pressed, released), (Some(HotkeyEvent::DictateDown), Some(HotkeyEvent::DictateUp)));
        assert!(held && !held_after, "the listener knows the key is held, then let go");
    }

    #[test]
    fn the_listener_ends_when_permission_is_taken_away() {
        assert_eq!(health(true, true), Health::Fine);
        assert_eq!(health(true, false), Health::Disabled, "turned back on");
        assert_eq!(health(false, true), Health::Revoked, "a tap without permission hears nothing");
        assert_eq!(health(false, false), Health::Revoked, "and is never turned back on");
    }

    #[test]
    fn shortcuts_reach_the_listener_through_atomics() {
        let _turn = turn();
        configure(Some(&Shortcut::ctrl_win()), Some(&Shortcut::alt_shift_v()));
        let c = config();
        assert_eq!(c.dictate, Some(Chord { mods: CTRL | WIN, key: None }));
        assert_eq!(c.paste_last.and_then(|p| p.key), Some(0x56));
        configure(None, None);
        assert_eq!(config().dictate, None);
    }
}
