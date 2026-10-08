//! The X11 backend against a real X server, with a test window of its own standing in for an
//! app, a second client for the window manager, and XTEST standing in for the user's keyboard. Opt-in, since they need a display that no
//! one is using (CI runs them under Xvfb):
//! `xvfb-run -a cargo test -p talkr --lib dictation::linux::x11 -- --ignored --test-threads=1`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::xproto::{
    AtomEnum, ConnectionExt as _, CreateWindowAux, EventMask, GrabMode, InputFocus, ModMask, PropMode, Window, WindowClass,
};
use x11rb::protocol::xinput::{self, ConnectionExt as _};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::protocol::Event;
use x11rb::wrapper::ConnectionExt as _;
use super::chord::{ALT, CTRL, WIN};
use super::clipboard::{self, Saved, Selection};
use super::keymap::{Keymap, CONTROL_MASK};
use super::keys::{self, Keysym};
use super::xconn::Display;
use super::{insert, listener, target};
use crate::dictation::backend::Backend;
use crate::dictation::settings::{DictationSettings, InsertMethod, Shortcut};
use crate::dictation::HotkeyEvent as E;

const LEFT: Keysym = 0xff51;
const SPACE: Keysym = 0x20;
const KEY_K: Keysym = 0x6b;

/// One test at a time: they share the server, the listener and the clipboard.
fn serial() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn settle() {
    std::thread::sleep(Duration::from_millis(250));
}

/// The user's keyboard: the server's own keyboard device (Xvfb's), driven through XTEST on a
/// connection of its own. Not XTEST's virtual keyboard, which Talkr itself types with: the
/// server keeps key state per device, and handing keys back depends on it.
struct Keyboard {
    d: Display,
    map: Keymap,
    /// XInput's first event code and the keyboard's device id.
    device: (u8, u8),
}

impl Keyboard {
    fn new() -> Self {
        let d = Display::open().expect("an X server (run under xvfb-run)");
        let map = Keymap::load(&d.conn).unwrap();
        let first_event = d.conn.extension_information(xinput::X11_EXTENSION_NAME).unwrap().expect("XInput").first_event;
        let list = d.conn.xinput_list_input_devices().unwrap().reply().unwrap();
        let id = list
            .devices
            .iter()
            .zip(&list.names)
            .find(|(dev, name)| {
                dev.device_use == xinput::DeviceUse::IS_X_EXTENSION_KEYBOARD
                    && !String::from_utf8_lossy(&name.name).contains("XTEST")
            })
            .map(|(dev, _)| dev.device_id)
            .expect("the server's keyboard device");
        Self { d, map, device: (first_event, id) }
    }

    fn key(&self, ks: Keysym, down: bool) {
        let kc = self.map.keycode_for(ks).unwrap_or_else(|| panic!("no key for {ks:#x}"));
        let (first_event, id) = self.device;
        let kind = first_event + if down { xinput::DEVICE_KEY_PRESS_EVENT } else { xinput::DEVICE_KEY_RELEASE_EVENT };
        self.d.conn.xtest_fake_input(kind, kc, 0, self.d.root, 0, 0, id).unwrap();
        self.d.conn.sync().unwrap();
        std::thread::sleep(Duration::from_millis(60));
    }

    fn keycode(&self, ks: Keysym) -> u8 {
        self.map.keycode_for(ks).unwrap()
    }
}

#[derive(Default)]
struct Seen {
    /// Key presses that reached the window: keycode and modifier state.
    presses: Vec<(u8, u16)>,
    /// Text typed into it, read through the keyboard map as an app would.
    typed: String,
    /// Text it pasted from the clipboard on Ctrl + V.
    pasted: Vec<String>,
}

/// A focused window on its own thread that acts like a simple text app.
struct Lab {
    window: Window,
    seen: Arc<Mutex<Seen>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Lab {
    fn open() -> Self {
        let d = Display::open().unwrap();
        let window = d.conn.generate_id().unwrap();
        let events = EventMask::KEY_PRESS | EventMask::KEY_RELEASE | EventMask::STRUCTURE_NOTIFY | EventMask::PROPERTY_CHANGE;
        d.conn
            .create_window(0, window, d.root, 50, 50, 300, 120, 0, WindowClass::INPUT_OUTPUT, 0, &CreateWindowAux::new().event_mask(events))
            .unwrap();
        d.conn.change_property8(PropMode::REPLACE, window, AtomEnum::WM_CLASS, AtomEnum::STRING, b"talkr-lab\0TalkrLab\0").unwrap();
        d.conn.map_window(window).unwrap();
        d.conn.flush().unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(Event::MapNotify(m)) = d.conn.poll_for_event().unwrap() {
                if m.window == window {
                    break;
                }
            }
            assert!(Instant::now() < deadline, "the lab window was not mapped");
            std::thread::sleep(Duration::from_millis(5));
        }
        d.conn.set_input_focus(InputFocus::PARENT, window, x11rb::CURRENT_TIME).unwrap();
        d.conn.sync().unwrap();

        let seen = Arc::new(Mutex::new(Seen::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let (seen2, stop2) = (seen.clone(), stop.clone());
        let thread = std::thread::spawn(move || {
            while !stop2.load(Ordering::SeqCst) {
                let Ok(Some(event)) = d.conn.poll_for_event() else {
                    std::thread::sleep(Duration::from_millis(2));
                    continue;
                };
                match event {
                    Event::KeyPress(e) => {
                        let state = u16::from(e.state);
                        seen2.lock().unwrap().presses.push((e.detail, state));
                        let syms = d.conn.get_keyboard_mapping(e.detail, 1).unwrap().reply().unwrap().keysyms;
                        let ks = syms.first().copied().unwrap_or(0);
                        if state & CONTROL_MASK != 0 {
                            if ks == keys::XK_V {
                                d.conn
                                    .convert_selection(window, d.atoms.CLIPBOARD, d.atoms.UTF8_STRING, d.atoms._TALKR_SELECTION, x11rb::CURRENT_TIME)
                                    .unwrap();
                                d.conn.flush().unwrap();
                            }
                        } else if ks == keys::XK_RETURN {
                            seen2.lock().unwrap().typed.push('\n');
                        } else if let Some(c) = keysym_char(ks) {
                            seen2.lock().unwrap().typed.push(c);
                        }
                    }
                    Event::SelectionNotify(n) if n.property != x11rb::NONE => {
                        let reply = d.conn.get_property(true, window, n.property, AtomEnum::ANY, 0, 1 << 20).unwrap().reply().unwrap();
                        seen2.lock().unwrap().pasted.push(String::from_utf8_lossy(&reply.value).into_owned());
                    }
                    _ => {}
                }
            }
        });
        settle();
        Self { window, seen, stop, thread: Some(thread) }
    }

    fn pressed(&self, kc: u8) -> bool {
        self.seen.lock().unwrap().presses.iter().any(|(k, _)| *k == kc)
    }

    fn clear(&self) {
        *self.seen.lock().unwrap() = Seen::default();
    }

    fn wait_for(&self, what: impl Fn(&Seen) -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if what(&self.seen.lock().unwrap()) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }
}

impl Drop for Lab {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn keysym_char(ks: Keysym) -> Option<char> {
    match ks {
        0x20..=0x7e | 0xa0..=0xff => char::from_u32(ks),
        0x0100_0000..=0x0110_ffff => char::from_u32(ks - 0x0100_0000),
        _ => None,
    }
}

struct Events(flume::Receiver<crate::dictation::HotkeyEvent>);

impl Events {
    fn start() -> Self {
        let (tx, rx) = flume::unbounded();
        listener::start(tx).expect("the listener starts");
        Self(rx)
    }

    fn next(&self) -> Option<crate::dictation::HotkeyEvent> {
        self.0.recv_timeout(Duration::from_secs(2)).ok()
    }

    fn none(&self) -> bool {
        std::thread::sleep(Duration::from_millis(300));
        self.0.try_iter().next().is_none()
    }
}

#[test]
#[ignore = "needs an X server of its own (xvfb-run)"]
fn xvfb_modifier_chord_reports_press_release_and_interruptions() {
    let _serial = serial();
    let events = Events::start();
    listener::configure(Some(&Shortcut::ctrl_win()), None);
    let lab = Lab::open();
    let kb = Keyboard::new();

    kb.key(keys::XK_CONTROL_L, true);
    kb.key(keys::XK_SUPER_L, true);
    assert_eq!(events.next(), Some(E::DictateDown));
    kb.key(keys::XK_SUPER_L, false);
    assert_eq!(events.next(), Some(E::DictateUp));
    kb.key(keys::XK_CONTROL_L, false);
    assert!(lab.pressed(kb.keycode(keys::XK_CONTROL_L)), "Ctrl, pressed first, reached the app");
    assert!(!lab.pressed(kb.keycode(keys::XK_SUPER_L)), "the chord's own key did not");

    // The other order works too.
    kb.key(keys::XK_SUPER_L, true);
    kb.key(keys::XK_CONTROL_L, true);
    assert_eq!(events.next(), Some(E::DictateDown));
    kb.key(keys::XK_CONTROL_L, false);
    assert_eq!(events.next(), Some(E::DictateUp));
    kb.key(keys::XK_SUPER_L, false);

    // Ctrl + Super + Left is the desktop's: reported as an interruption, and the arrow is
    // replayed to the focused window.
    lab.clear();
    kb.key(keys::XK_CONTROL_L, true);
    kb.key(keys::XK_SUPER_L, true);
    assert_eq!(events.next(), Some(E::DictateDown));
    kb.key(LEFT, true);
    assert_eq!(events.next(), Some(E::Interrupted));
    let left = kb.keycode(LEFT);
    assert!(lab.wait_for(|s| s.presses.iter().any(|(k, _)| *k == left)), "the arrow reached the app");
    let state = lab.seen.lock().unwrap().presses.iter().find(|(k, _)| *k == left).map(|(_, s)| *s).unwrap();
    assert_eq!(state & CONTROL_MASK, CONTROL_MASK, "with its modifiers");
    kb.key(LEFT, false);
    kb.key(keys::XK_SUPER_L, false);
    kb.key(keys::XK_CONTROL_L, false);
    assert!(events.none());

    // Plain typing is never seen by Talkr.
    kb.key(KEY_K, true);
    kb.key(KEY_K, false);
    assert!(events.none());
    assert!(lab.pressed(kb.keycode(KEY_K)));
    listener::stop();
    assert!(!listener::is_running());
}

#[test]
#[ignore = "needs an X server of its own (xvfb-run)"]
fn xvfb_key_chords_escape_and_paste_again() {
    let _serial = serial();
    let events = Events::start();
    let chord = Shortcut { ctrl: true, shift: false, alt: true, win: false, key: Some(0x20), key_label: Some("Space".into()) };
    listener::configure(Some(&chord), Some(&Shortcut::alt_shift_v()));
    let lab = Lab::open();
    let kb = Keyboard::new();

    kb.key(keys::XK_CONTROL_L, true);
    kb.key(keys::XK_ALT_L, true);
    kb.key(SPACE, true);
    assert_eq!(events.next(), Some(E::DictateDown));
    kb.key(SPACE, false);
    assert_eq!(events.next(), Some(E::DictateUp));
    kb.key(keys::XK_ALT_L, false);
    kb.key(keys::XK_CONTROL_L, false);
    assert!(!lab.pressed(kb.keycode(SPACE)), "the shortcut's key is kept from the app");

    // Escape is Talkr's only while a dictation records.
    listener::set_recording(true);
    settle();
    kb.key(keys::XK_ESCAPE, true);
    kb.key(keys::XK_ESCAPE, false);
    assert_eq!(events.next(), Some(E::Cancel));
    assert!(!lab.pressed(kb.keycode(keys::XK_ESCAPE)));
    listener::set_recording(false);
    settle();
    kb.key(keys::XK_ESCAPE, true);
    kb.key(keys::XK_ESCAPE, false);
    assert!(events.none());
    assert!(lab.pressed(kb.keycode(keys::XK_ESCAPE)));

    kb.key(keys::XK_ALT_L, true);
    kb.key(keys::XK_SHIFT_L, true);
    kb.key(keys::XK_V, true);
    kb.key(keys::XK_V, false);
    kb.key(keys::XK_SHIFT_L, false);
    kb.key(keys::XK_ALT_L, false);
    assert_eq!(events.next(), Some(E::PasteLast));
    assert!(events.none());
    listener::stop();
}

#[test]
#[ignore = "needs an X server of its own (xvfb-run)"]
fn xvfb_records_a_new_shortcut_with_the_keyboard_grabbed() {
    let _serial = serial();
    let events = Events::start();
    listener::configure(None, None);
    let lab = Lab::open();
    let kb = Keyboard::new();

    listener::set_capturing(true);
    settle();
    kb.key(keys::XK_CONTROL_L, true);
    kb.key(keys::XK_ALT_L, true);
    kb.key(KEY_K, true);
    kb.key(KEY_K, false);
    kb.key(keys::XK_ALT_L, false);
    kb.key(keys::XK_CONTROL_L, false);
    match events.next() {
        Some(E::Captured(s)) => {
            assert!(s.ctrl && s.alt && !s.shift && !s.win);
            assert_eq!(s.key, Some(0x4B));
            assert_eq!(s.key_label.as_deref(), Some("K"));
        }
        other => panic!("{other:?}"),
    }
    assert!(!lab.pressed(kb.keycode(KEY_K)), "keys went to the recorder, not the app");
    // The keyboard is the app's again.
    kb.key(KEY_K, true);
    kb.key(KEY_K, false);
    assert!(lab.wait_for(|s| !s.presses.is_empty()));

    listener::set_capturing(true);
    settle();
    kb.key(keys::XK_ESCAPE, true);
    kb.key(keys::XK_ESCAPE, false);
    assert_eq!(events.next(), Some(E::CaptureCancelled));
    listener::stop();
}

#[test]
#[ignore = "needs an X server of its own (xvfb-run)"]
fn xvfb_clipboard_round_trip_and_restore_rules() {
    let _serial = serial();
    assert!(clipboard::put(Selection::Clipboard, "first ✓"));
    assert_eq!(clipboard::read(Selection::Clipboard), Some(Saved::Text("first ✓".into())));
    assert!(clipboard::holds(Selection::Clipboard, "first ✓"));

    // Another app copies something: Talkr must not put its own content back over it.
    let other = Display::open().unwrap();
    let window = other.helper_window(EventMask::NO_EVENT).unwrap();
    other.conn.set_selection_owner(window, other.atoms.CLIPBOARD, x11rb::CURRENT_TIME).unwrap();
    other.conn.sync().unwrap();
    settle();
    assert!(!clipboard::holds(Selection::Clipboard, "first ✓"));
    insert::put_back(&[(Selection::Clipboard, Saved::Text("older".into()))], "first ✓");
    let owner = other.conn.get_selection_owner(other.atoms.CLIPBOARD).unwrap().reply().unwrap().owner;
    assert_eq!(owner, window, "the other app's copy stays");
    drop(other);
    settle();
    assert_eq!(clipboard::read(Selection::Clipboard), Some(Saved::Empty), "its owner is gone");
}

#[test]
#[ignore = "needs an X server of its own (xvfb-run)"]
fn xvfb_pastes_into_the_focused_window_and_restores_the_clipboard() {
    let _serial = serial();
    let events = Events::start();
    listener::configure(Some(&Shortcut::ctrl_win()), None);
    let lab = Lab::open();
    assert!(clipboard::put(Selection::Clipboard, "the user's clipboard"));

    let target = xconn_snapshot().expect("the lab window is the target");
    assert_eq!(target.window, lab.window);
    assert_eq!(target.app_id().as_deref(), Some("talkrlab"));
    assert_eq!(<super::Os as Backend>::app_label(&target).as_deref(), Some("TalkrLab"));
    let settings = DictationSettings { insert_method: InsertMethod::Paste, ..Default::default() };
    let delivery = insert::deliver("Hello from Talkr", Some(&target), &settings);
    assert!(delivery.inserted && delivery.copied.is_none(), "{delivery:?}");
    assert_eq!(delivery.method, Some("paste"));
    assert!(lab.wait_for(|s| s.pasted.iter().any(|t| t == "Hello from Talkr")), "the app pasted the text");
    assert_eq!(clipboard::read(Selection::Clipboard), Some(Saved::Text("the user's clipboard".into())));
    assert!(events.none(), "Talkr's own Ctrl + V is not a shortcut");
    listener::stop();
}

#[test]
#[ignore = "needs an X server of its own (xvfb-run)"]
fn xvfb_types_unicode_and_line_breaks() {
    let _serial = serial();
    let lab = Lab::open();
    let target = xconn_snapshot().expect("the lab window is the target");
    let settings = DictationSettings { insert_method: InsertMethod::Type, ..Default::default() };
    let text = "Typed by Talkr: wörld ✓ 1+1\nzweite Zeile";
    let delivery = insert::deliver(text, Some(&target), &settings);
    assert_eq!(delivery.method, Some("type"), "{delivery:?}");
    assert!(lab.wait_for(|s| s.typed == text), "typed {:?}", lab.seen.lock().unwrap().typed);
    // The borrowed keys are empty again.
    let d = Display::open().unwrap();
    assert!(!Keymap::load(&d.conn).unwrap().spare_keycodes().is_empty());
}

fn xconn_snapshot() -> Option<target::Target> {
    super::xconn::with(target::snapshot).flatten()
}

/// A second client acting as the window manager: a passive grab on the root window for one
/// shortcut, counting the presses and releases of its key that it is given.
struct WindowManager {
    counts: Arc<Mutex<(usize, usize)>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl WindowManager {
    /// Grab `ks` with `mods` (Ctrl/Shift/Alt/Super bits), under every lock combination.
    fn grab(ks: Keysym, mods: u8) -> Self {
        let d = Display::open().unwrap();
        let map = Keymap::load(&d.conn).unwrap();
        let kc = map.keycode_for(ks).unwrap();
        for lock in map.lock_masks() {
            d.conn
                .grab_key(false, d.root, ModMask::from(map.mask_of(mods) | lock), kc, GrabMode::ASYNC, GrabMode::ASYNC)
                .unwrap()
                .check()
                .expect("the window manager's grab");
        }
        let counts = Arc::new(Mutex::new((0, 0)));
        let stop = Arc::new(AtomicBool::new(false));
        let (seen, stop2) = (counts.clone(), stop.clone());
        let thread = std::thread::spawn(move || {
            while !stop2.load(Ordering::SeqCst) {
                match d.conn.poll_for_event() {
                    Ok(Some(Event::KeyPress(e))) if e.detail == kc => seen.lock().unwrap().0 += 1,
                    Ok(Some(Event::KeyRelease(e))) if e.detail == kc => seen.lock().unwrap().1 += 1,
                    Ok(Some(_)) => {}
                    _ => std::thread::sleep(Duration::from_millis(2)),
                }
            }
        });
        Self { counts, stop, thread: Some(thread) }
    }

    fn counts(&self) -> (usize, usize) {
        *self.counts.lock().unwrap()
    }
}

impl Drop for WindowManager {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn key_down(kc: u8) -> bool {
    let d = Display::open().unwrap();
    let keys = d.conn.query_keymap().unwrap().reply().unwrap().keys;
    keys[kc as usize / 8] & (1 << (kc % 8)) != 0
}

#[test]
#[ignore = "needs an X server of its own (xvfb-run)"]
fn xvfb_window_manager_shortcuts_keep_working_during_the_chord() {
    let _serial = serial();
    let wm = WindowManager::grab(LEFT, CTRL | WIN);
    let events = Events::start();
    listener::configure(Some(&Shortcut::ctrl_win()), None);
    assert_eq!(listener::problem(), None, "Ctrl + Super itself is free");
    let lab = Lab::open();
    let kb = Keyboard::new();
    let left = kb.keycode(LEFT);

    for round in 1..=2 {
        lab.clear();
        kb.key(keys::XK_CONTROL_L, true);
        kb.key(keys::XK_SUPER_L, true);
        assert_eq!(events.next(), Some(E::DictateDown));
        kb.key(LEFT, true);
        assert_eq!(events.next(), Some(E::Interrupted));
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(wm.counts().0, round, "the window manager got Ctrl + Super + Left, once");
        assert!(!lab.pressed(left), "the app did not");
        assert!(key_down(left), "still held");
        kb.key(LEFT, false);
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(wm.counts().1, round, "and its release");
        kb.key(keys::XK_SUPER_L, false);
        kb.key(keys::XK_CONTROL_L, false);
        assert!(!key_down(left), "no key is left down");
        assert!(events.none());
    }

    // Left alone is the app's, and arrives as a plain press: nothing is stuck.
    lab.clear();
    kb.key(LEFT, true);
    kb.key(LEFT, false);
    assert!(lab.wait_for(|s| s.presses.iter().any(|&(k, state)| k == left && state & CONTROL_MASK == 0)));
    assert_eq!(wm.counts(), (2, 2));
    assert!(events.none());
    listener::stop();
}

#[test]
#[ignore = "needs an X server of its own (xvfb-run)"]
fn xvfb_a_shortcut_another_program_owns_is_reported() {
    let _serial = serial();
    let _other = WindowManager::grab(SPACE, CTRL | ALT);
    let _events = Events::start();
    let taken = Shortcut { ctrl: true, shift: false, alt: true, win: false, key: Some(0x20), key_label: Some("Space".into()) };
    listener::configure(Some(&taken), Some(&Shortcut::alt_shift_v()));
    let problem = listener::problem().expect("the conflict is reported");
    assert!(problem.contains("Ctrl + Alt + Space") && problem.contains("dictation shortcut"), "{problem}");
    assert!(!problem.contains("paste-again"), "{problem}");
    assert_eq!(<super::Os as Backend>::hotkeys_problem(), Some(problem));
    listener::configure(Some(&Shortcut::ctrl_win()), Some(&Shortcut::alt_shift_v()));
    assert_eq!(listener::problem(), None, "a free shortcut clears it");
    listener::stop();
}
