//! The global shortcut listener: a low-level keyboard hook on its own thread.
//!
//! `RegisterHotKey` (and Tauri's global-shortcut plugin, built on it) cannot report a shortcut
//! being released, nor a shortcut made of modifiers only, and hold-to-talk on Ctrl + Win needs
//! both. The hook sees every key, so it must be quick: Windows silently removes a hook that takes
//! longer than about 300 ms to answer. The callback only reads atomics, updates thread-local
//! state and sends to a channel; everything else happens on the dictation thread.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::OnceLock;
use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, KillTimer, PostThreadMessageW, SetTimer, SetWindowsHookExW, UnhookWindowsHookEx,
    HC_ACTION, HHOOK, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_QUIT, WM_SYSKEYDOWN, WM_TIMER,
};
use super::keys::{self, MARKER, VK_ESCAPE};
use crate::dictation::settings::Shortcut;
use crate::dictation::HotkeyEvent;

const CTRL: u8 = 1;
const SHIFT: u8 = 2;
const ALT: u8 = 4;
const WIN: u8 = 8;

/// How often the hook is reinstalled while nothing is held. A hook Windows dropped after a stall
/// (a frozen system, a debugger) would otherwise stay dead until Talkr restarts.
const REINSTALL_EVERY_MS: u32 = 5 * 60 * 1000;

static DICTATE: AtomicU64 = AtomicU64::new(0);
static PASTE_LAST: AtomicU64 = AtomicU64::new(0);
/// While a dictation records, Escape cancels it (and does not reach the app).
static RECORDING: AtomicBool = AtomicBool::new(false);
/// While the settings page records a new shortcut, every key goes to it instead.
static CAPTURING: AtomicBool = AtomicBool::new(false);
static THREAD_ID: AtomicU32 = AtomicU32::new(0);
static EVENTS: OnceLock<flume::Sender<HotkeyEvent>> = OnceLock::new();

const ENABLED_BIT: u64 = 1 << 32;

fn encode(shortcut: Option<&Shortcut>) -> u64 {
    let Some(s) = shortcut else { return 0 };
    let mut mods = 0u8;
    for (on, bit) in [(s.ctrl, CTRL), (s.shift, SHIFT), (s.alt, ALT), (s.win, WIN)] {
        if on {
            mods |= bit;
        }
    }
    ENABLED_BIT | (mods as u64) << 16 | s.key.unwrap_or(0) as u64
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Chord {
    mods: u8,
    key: Option<u16>,
}

fn decode(v: u64) -> Option<Chord> {
    (v & ENABLED_BIT != 0).then(|| {
        let key = (v & 0xFFFF) as u16;
        Chord { mods: ((v >> 16) & 0xFF) as u8, key: (key != 0).then_some(key) }
    })
}

/// Point the hook at new shortcuts. `None` turns one off.
pub fn configure(dictate: Option<&Shortcut>, paste_last: Option<&Shortcut>) {
    DICTATE.store(encode(dictate), Ordering::SeqCst);
    PASTE_LAST.store(encode(paste_last), Ordering::SeqCst);
}

pub fn set_recording(recording: bool) {
    RECORDING.store(recording, Ordering::SeqCst);
}

pub fn set_capturing(capturing: bool) {
    CAPTURING.store(capturing, Ordering::SeqCst);
}

pub fn is_running() -> bool {
    THREAD_ID.load(Ordering::SeqCst) != 0
}

fn modifier_bit(vk: u16) -> u8 {
    match vk {
        keys::VK_CONTROL | keys::VK_LCONTROL | keys::VK_RCONTROL => CTRL,
        keys::VK_SHIFT | keys::VK_LSHIFT | keys::VK_RSHIFT => SHIFT,
        keys::VK_MENU | keys::VK_LMENU | keys::VK_RMENU => ALT,
        keys::VK_LWIN | keys::VK_RWIN => WIN,
        _ => 0,
    }
}

fn is_modifier(vk: u16) -> bool {
    modifier_bit(vk) != 0
}

#[derive(Default)]
struct State {
    /// Keys held right now, as this hook saw them.
    down: Vec<u16>,
    /// Keys whose press the hook kept from the app: their release must not reach it either, or
    /// the app would see a key go up that never went down.
    swallowed: Vec<u16>,
    dictate_active: bool,
    /// A modifier-only dictation shortcut met another key (Ctrl + Win + Left): ignore it until
    /// its modifiers are released.
    dictate_blocked: bool,
    paste_active: bool,
    capture: Option<Chord>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

fn emit(event: HotkeyEvent) {
    if let Some(tx) = EVENTS.get() {
        let _ = tx.try_send(event);
    }
}

fn mods_of(down: &[u16]) -> u8 {
    down.iter().fold(0, |m, &vk| m | modifier_bit(vk))
}

fn remove(list: &mut Vec<u16>, vk: u16) -> bool {
    match list.iter().position(|&k| k == vk) {
        Some(i) => {
            list.swap_remove(i);
            true
        }
        None => false,
    }
}

/// One key event; returns whether to keep it from the focused app.
fn handle(state: &mut State, vk: u16, down: bool, physically_down: &dyn Fn(u16) -> bool) -> bool {
    let repeat = down && state.down.contains(&vk);
    if down && !repeat {
        // A key released while another app held the keyboard (an elevated window, the secure
        // desktop) never reached this hook: drop keys the system says are up.
        state.down.retain(|&k| k == vk || physically_down(k));
        state.down.push(vk);
    }
    if !down {
        remove(&mut state.down, vk);
    }
    let was_swallowed = !down && remove(&mut state.swallowed, vk);

    if CAPTURING.load(Ordering::SeqCst) {
        return capture(state, vk, down, repeat) || was_swallowed;
    }
    if state.capture.take().is_some() {
        // Capture was ended from outside mid-way.
        state.swallowed.clear();
    }

    if vk == VK_ESCAPE && RECORDING.load(Ordering::SeqCst) {
        if down {
            if !repeat {
                emit(HotkeyEvent::Cancel);
            }
            state.swallowed.push(vk);
            return true;
        }
        return was_swallowed;
    }

    let mods = mods_of(&state.down);
    let mut swallow = was_swallowed;
    if let Some(chord) = decode(DICTATE.load(Ordering::Relaxed)) {
        swallow |= dictate(state, chord, vk, down, repeat, mods);
    }
    if let Some(chord) = decode(PASTE_LAST.load(Ordering::Relaxed)) {
        swallow |= paste_last(state, chord, vk, down, repeat, mods);
    }
    swallow
}

fn dictate(state: &mut State, chord: Chord, vk: u16, down: bool, repeat: bool, mods: u8) -> bool {
    match chord.key {
        Some(key) => {
            if vk == key {
                if down {
                    if state.dictate_active {
                        return true; // auto-repeat while held
                    }
                    if !repeat && mods == chord.mods {
                        state.dictate_active = true;
                        state.swallowed.push(vk);
                        emit(HotkeyEvent::DictateDown);
                        return true;
                    }
                } else if state.dictate_active {
                    state.dictate_active = false;
                    emit(HotkeyEvent::DictateUp);
                }
            } else if state.dictate_active && !down && is_modifier(vk) && mods & chord.mods != chord.mods {
                // A modifier of the shortcut let go first.
                state.dictate_active = false;
                emit(HotkeyEvent::DictateUp);
            }
            false
        }
        None => {
            if is_modifier(vk) {
                if down && !repeat {
                    if state.dictate_active {
                        if mods != chord.mods {
                            // Another modifier joined: that is a different shortcut.
                            state.dictate_active = false;
                            state.dictate_blocked = true;
                            emit(HotkeyEvent::Interrupted);
                        }
                    } else if !state.dictate_blocked && mods == chord.mods && state.down.iter().all(|&k| is_modifier(k)) {
                        state.dictate_active = true;
                        emit(HotkeyEvent::DictateDown);
                    }
                } else if !down {
                    if state.dictate_active && mods & chord.mods != chord.mods {
                        state.dictate_active = false;
                        emit(HotkeyEvent::DictateUp);
                    }
                    if mods & chord.mods == 0 {
                        state.dictate_blocked = false;
                    }
                }
            } else if down && !repeat && state.dictate_active {
                // Ctrl + Win + Left and friends: Windows' shortcut, not dictation.
                state.dictate_active = false;
                state.dictate_blocked = true;
                emit(HotkeyEvent::Interrupted);
            }
            false
        }
    }
}

fn paste_last(state: &mut State, chord: Chord, vk: u16, down: bool, repeat: bool, mods: u8) -> bool {
    match chord.key {
        Some(key) if vk == key => {
            if down && !repeat && mods == chord.mods {
                state.paste_active = true;
                state.swallowed.push(vk);
                emit(HotkeyEvent::PasteLast);
                return true;
            }
            if down && state.paste_active {
                return true;
            }
            if !down {
                state.paste_active = false;
            }
            false
        }
        Some(_) => false,
        None => {
            if is_modifier(vk) && down && !repeat && !state.paste_active && mods == chord.mods {
                state.paste_active = true;
                emit(HotkeyEvent::PasteLast);
            } else if !down && mods & chord.mods != chord.mods {
                state.paste_active = false;
            }
            false
        }
    }
}

/// Recording a new shortcut: every key is kept from the apps, and the combination is reported
/// once all keys are up. Escape alone cancels.
fn capture(state: &mut State, vk: u16, down: bool, repeat: bool) -> bool {
    if down {
        if repeat {
            return true;
        }
        if vk == VK_ESCAPE && state.capture.is_none() {
            CAPTURING.store(false, Ordering::SeqCst);
            state.swallowed.push(vk);
            emit(HotkeyEvent::CaptureCancelled);
            return true;
        }
        let chord = state.capture.get_or_insert(Chord { mods: 0, key: None });
        chord.mods |= modifier_bit(vk);
        if !is_modifier(vk) {
            chord.key = Some(vk);
        }
        state.swallowed.push(vk);
        return true;
    }
    if state.down.is_empty() {
        if let Some(chord) = state.capture.take() {
            CAPTURING.store(false, Ordering::SeqCst);
            emit(HotkeyEvent::Captured(Shortcut {
                ctrl: chord.mods & CTRL != 0,
                shift: chord.mods & SHIFT != 0,
                alt: chord.mods & ALT != 0,
                win: chord.mods & WIN != 0,
                key: chord.key,
                key_label: chord.key.map(keys::key_label),
            }));
        }
    }
    // Releases of keys pressed before capture began belong to the app.
    false
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: for WH_KEYBOARD_LL with HC_ACTION, lparam points at a KBDLLHOOKSTRUCT.
        let kb = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        if kb.dwExtraInfo != MARKER {
            let message = wparam.0 as u32;
            let down = message == WM_KEYDOWN || message == WM_SYSKEYDOWN;
            let vk = kb.vkCode as u16;
            // A panic must not unwind into Windows (that aborts the process).
            let swallow = std::panic::catch_unwind(|| {
                STATE.with(|s| handle(&mut s.borrow_mut(), vk, down, &keys::is_down))
            })
            .unwrap_or(false);
            if swallow {
                return LRESULT(1);
            }
        }
    }
    // SAFETY: forwarding the event unchanged, as every hook must.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn install() -> windows::core::Result<HHOOK> {
    // SAFETY: a module handle for this executable and a valid hook procedure.
    unsafe {
        let module = GetModuleHandleW(None)?;
        SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), Some(HINSTANCE(module.0)), 0)
    }
}

/// Start the hook thread (once; later calls only update where events go is not supported, the
/// sender is fixed at the first start). Returns an error if the hook cannot be installed.
pub fn start(events: flume::Sender<HotkeyEvent>) -> Result<(), String> {
    if is_running() {
        return Ok(());
    }
    let _ = EVENTS.set(events);
    let (ready_tx, ready_rx) = flume::bounded::<Result<(), String>>(1);
    std::thread::Builder::new()
        .name("talkr-hotkeys".into())
        .spawn(move || {
            let mut hook = match install() {
                Ok(hook) => hook,
                Err(e) => {
                    let _ = ready_tx.send(Err(format!("Could not listen for the dictation shortcut: {}", e)));
                    return;
                }
            };
            // SAFETY: plain thread query and a thread timer on this thread's queue.
            let thread_id = unsafe { GetCurrentThreadId() };
            THREAD_ID.store(thread_id, Ordering::SeqCst);
            let timer = unsafe { SetTimer(None, 0, REINSTALL_EVERY_MS, None) };
            let _ = ready_tx.send(Ok(()));
            log::info!("dictation shortcut listener started");

            let mut msg = MSG::default();
            // SAFETY: a standard message loop; hook callbacks run inside GetMessageW.
            unsafe {
                loop {
                    let got = GetMessageW(&mut msg, None, 0, 0);
                    if got.0 <= 0 || msg.message == WM_QUIT {
                        break;
                    }
                    if msg.message == WM_TIMER {
                        let idle = STATE.with(|s| {
                            let s = s.borrow();
                            s.down.is_empty() && !s.dictate_active && s.capture.is_none()
                        });
                        if idle {
                            // Install the new hook before removing the old one. No event is
                            // handled in between: hooks run inside GetMessageW, on this thread.
                            match install() {
                                Ok(fresh) => {
                                    let _ = UnhookWindowsHookEx(hook);
                                    hook = fresh;
                                }
                                Err(e) => log::warn!("could not refresh the keyboard hook: {}", e),
                            }
                        }
                    }
                }
                let _ = KillTimer(None, timer);
                let _ = UnhookWindowsHookEx(hook);
            }
            THREAD_ID.store(0, Ordering::SeqCst);
            STATE.with(|s| *s.borrow_mut() = State::default());
            log::info!("dictation shortcut listener stopped");
        })
        .map_err(|e| e.to_string())?;
    ready_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap_or_else(|_| Err("The dictation shortcut listener did not start".into()))
}

/// Stop the hook thread. Keys pass straight to apps again.
pub fn stop() {
    let id = THREAD_ID.load(Ordering::SeqCst);
    if id != 0 {
        // SAFETY: posting WM_QUIT to a thread that runs a message loop.
        let _ = unsafe { PostThreadMessageW(id, WM_QUIT, WPARAM(0), LPARAM(0)) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dictation::HotkeyEvent as E;

    const LCTRL: u16 = keys::VK_LCONTROL;
    const LWIN: u16 = keys::VK_LWIN;
    const LEFT: u16 = 0x25;
    const SPACE: u16 = 0x20;

    /// Run key events through the hook logic and collect what it reports.
    struct Harness {
        state: State,
        rx: flume::Receiver<HotkeyEvent>,
    }

    // The sender is global; tests that read events run one at a time.
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    impl Harness {
        fn new(dictate: Option<Shortcut>, paste: Option<Shortcut>) -> (Self, std::sync::MutexGuard<'static, ()>) {
            let guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
            static CHANNEL: OnceLock<flume::Receiver<HotkeyEvent>> = OnceLock::new();
            let rx = CHANNEL
                .get_or_init(|| {
                    let (tx, rx) = flume::unbounded();
                    let _ = EVENTS.set(tx);
                    rx
                })
                .clone();
            while rx.try_recv().is_ok() {}
            configure(dictate.as_ref(), paste.as_ref());
            RECORDING.store(false, Ordering::SeqCst);
            CAPTURING.store(false, Ordering::SeqCst);
            (Self { state: State::default(), rx }, guard)
        }

        fn key(&mut self, vk: u16, down: bool) -> bool {
            let held = self.state.down.clone();
            handle(&mut self.state, vk, down, &move |k| held.contains(&k))
        }

        fn events(&self) -> Vec<HotkeyEvent> {
            self.rx.try_iter().collect()
        }
    }

    #[test]
    fn modifier_chord_reports_press_and_release() {
        let (mut h, _g) = Harness::new(Some(Shortcut::ctrl_win()), None);
        assert!(!h.key(LCTRL, true));
        assert!(!h.key(LWIN, true));
        assert_eq!(h.events(), vec![E::DictateDown]);
        // Auto-repeat changes nothing.
        h.key(LWIN, true);
        assert!(h.events().is_empty());
        assert!(!h.key(LWIN, false));
        assert_eq!(h.events(), vec![E::DictateUp]);
        h.key(LCTRL, false);
        assert!(h.events().is_empty());
    }

    #[test]
    fn windows_shortcuts_interrupt_and_block_until_released() {
        let (mut h, _g) = Harness::new(Some(Shortcut::ctrl_win()), None);
        h.key(LCTRL, true);
        h.key(LWIN, true);
        assert!(!h.key(LEFT, true), "the arrow reaches Windows");
        assert_eq!(h.events(), vec![E::DictateDown, E::Interrupted]);
        h.key(LEFT, false);
        // Pressing the arrow again while still holding does nothing.
        h.key(LEFT, true);
        h.key(LEFT, false);
        h.key(LWIN, false);
        h.key(LCTRL, false);
        assert!(h.events().is_empty());
        // Released fully: works again.
        h.key(LCTRL, true);
        h.key(LWIN, true);
        assert_eq!(h.events(), vec![E::DictateDown]);
    }

    #[test]
    fn chord_only_fires_on_exact_modifiers() {
        let (mut h, _g) = Harness::new(Some(Shortcut::ctrl_win()), None);
        h.key(keys::VK_LSHIFT, true);
        h.key(LCTRL, true);
        h.key(LWIN, true);
        assert!(h.events().is_empty(), "Ctrl + Shift + Win is another shortcut");
        // A letter held first: typing, not dictation.
        let (mut h, _g2) = (Harness { state: State::default(), rx: h.rx.clone() }, ());
        h.key(0x41, true);
        h.key(LCTRL, true);
        h.key(LWIN, true);
        assert!(h.events().is_empty());
    }

    #[test]
    fn key_chord_swallows_its_key() {
        let s = Shortcut { ctrl: true, shift: false, alt: false, win: false, key: Some(SPACE), key_label: None };
        let (mut h, _g) = Harness::new(Some(s), None);
        assert!(!h.key(LCTRL, true));
        assert!(h.key(SPACE, true), "kept from the app");
        assert!(h.key(SPACE, true), "repeats too");
        assert!(h.key(SPACE, false), "and the release");
        assert!(!h.key(LCTRL, false));
        assert_eq!(h.events(), vec![E::DictateDown, E::DictateUp]);
        // Space alone is just typing.
        assert!(!h.key(SPACE, true));
        assert!(!h.key(SPACE, false));
        assert!(h.events().is_empty());
    }

    #[test]
    fn escape_cancels_only_while_recording() {
        let (mut h, _g) = Harness::new(Some(Shortcut::ctrl_win()), None);
        assert!(!h.key(VK_ESCAPE, true));
        h.key(VK_ESCAPE, false);
        RECORDING.store(true, Ordering::SeqCst);
        assert!(h.key(VK_ESCAPE, true));
        assert!(h.key(VK_ESCAPE, false));
        assert_eq!(h.events(), vec![E::Cancel]);
        RECORDING.store(false, Ordering::SeqCst);
    }

    #[test]
    fn paste_last_fires_once_per_press() {
        let (mut h, _g) = Harness::new(Some(Shortcut::ctrl_win()), Some(Shortcut::alt_shift_v()));
        h.key(keys::VK_LMENU, true);
        h.key(keys::VK_LSHIFT, true);
        assert!(h.key(keys::VK_V, true));
        assert!(h.key(keys::VK_V, true));
        assert!(h.key(keys::VK_V, false));
        assert_eq!(h.events(), vec![E::PasteLast]);
    }

    #[test]
    fn capture_reports_the_whole_combination() {
        let (mut h, _g) = Harness::new(Some(Shortcut::ctrl_win()), None);
        CAPTURING.store(true, Ordering::SeqCst);
        assert!(h.key(LCTRL, true));
        assert!(h.key(keys::VK_LMENU, true));
        assert!(h.key(SPACE, true));
        h.key(SPACE, false);
        h.key(keys::VK_LMENU, false);
        assert!(h.events().is_empty(), "reported once everything is up");
        h.key(LCTRL, false);
        match h.events().as_slice() {
            [E::Captured(s)] => {
                assert!(s.ctrl && s.alt && !s.shift && !s.win);
                assert_eq!(s.key, Some(SPACE));
            }
            other => panic!("{other:?}"),
        }
        assert!(!CAPTURING.load(Ordering::SeqCst));
        // Nothing triggers dictation while capturing.
        CAPTURING.store(true, Ordering::SeqCst);
        h.key(VK_ESCAPE, true);
        assert_eq!(h.events(), vec![E::CaptureCancelled]);
    }
}
