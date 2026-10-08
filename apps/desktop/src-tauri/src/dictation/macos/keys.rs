//! Synthetic keyboard input, and what the keyboard is doing right now. Every event Talkr posts
//! carries [`MARKER`] in its user-data field, so Talkr's own listener never reacts to it.

use std::time::{Duration, Instant};
use super::ffi::{self, Cf};
use super::keymap::{self, FLAGS_MODIFIERS, FLAG_COMMAND, FLAG_CONTROL, FLAG_OPTION, FLAG_SHIFT, MAC_COMMAND, MAC_RETURN, MAC_V};
use super::layout::Stroke;
use super::target::LineBreak;
use crate::dictation::settings::Shortcut;

/// `kCGEventSourceUserData` of Talkr's own key events: "TKR1".
pub const MARKER: i64 = 0x544B_5231;

/// A private event source: the events carry exactly the modifiers Talkr sets, whatever keys
/// the user still holds, and do not change the keyboard state other apps read.
fn source() -> Option<Cf> {
    // SAFETY: Create returns an owned source (NULL on failure).
    unsafe { Cf::from_owned(ffi::CGEventSourceCreate(ffi::STATE_PRIVATE)) }
}

/// Post one key event. `text`: the characters it types, overriding the keyboard layout.
fn post(source: &Cf, code: u16, down: bool, flags: u64, text: Option<&[u16]>) -> bool {
    // SAFETY: Create returns an owned event; it is configured and posted while alive, and the
    // text pointer and length describe a live slice.
    unsafe {
        let Some(event) = Cf::from_owned(ffi::CGEventCreateKeyboardEvent(source.as_ptr(), code, down)) else {
            return false;
        };
        let raw: ffi::CGEventRef = event.as_ptr().cast_mut();
        ffi::CGEventSetFlags(raw, flags);
        ffi::CGEventSetIntegerValueField(raw, ffi::FIELD_SOURCE_USER_DATA, MARKER);
        if let Some(units) = text {
            ffi::CGEventKeyboardSetUnicodeString(raw, units.len() as _, units.as_ptr());
        }
        ffi::CGEventPost(ffi::HID_EVENT_TAP, raw);
    }
    true
}

/// Press ⌘V. The V carries the character "v" too, so layouts that move the key (Dvorak) still
/// paste.
pub fn paste() -> bool {
    let Some(source) = source() else { return false };
    let v = [u16::from(b'v')];
    let command = post(&source, MAC_COMMAND, true, FLAG_COMMAND, None);
    let pressed = post(&source, MAC_V, true, FLAG_COMMAND, Some(&v));
    let released = post(&source, MAC_V, false, FLAG_COMMAND, Some(&v));
    // Always let go of ⌘, even when the V failed.
    let command_up = post(&source, MAC_COMMAND, false, 0, None);
    command && pressed && released && command_up
}

const MAC_SHIFT: u16 = 0x38;

/// How much of a text was typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Typed {
    All,
    Nothing,
    /// Some keys went in, then macOS refused one: the field holds part of the text.
    Partly,
}

impl Typed {
    fn stopped(after_any: bool) -> Typed {
        if after_any {
            Typed::Partly
        } else {
            Typed::Nothing
        }
    }
}

/// Press and release one key; whether both events were posted.
fn tap_key(source: &Cf, code: u16, flags: u64, text: Option<&[u16]>) -> bool {
    post(source, code, true, flags, text) && post(source, code, false, flags, text)
}

/// Type `text` as key events carrying its characters, a few at a time so slow apps keep up.
pub fn type_text(text: &str, line_break: LineBreak) -> Typed {
    let Some(source) = source() else { return Typed::Nothing };
    let pause = || std::thread::sleep(Duration::from_millis(4));
    let mut any = false;
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            let flags = if line_break == LineBreak::ShiftReturn { FLAG_SHIFT } else { 0 };
            if !tap_key(&source, MAC_RETURN, flags, None) {
                return Typed::stopped(any);
            }
            any = true;
            pause();
        }
        let units: Vec<u16> = line.trim_end_matches('\r').encode_utf16().collect();
        for chunk in keymap::typing_chunks(&units, 16) {
            // Key code 0 with the text attached: apps take the text, not the key.
            if !tap_key(&source, 0, 0, Some(chunk)) {
                return Typed::stopped(any);
            }
            any = true;
            pause();
        }
    }
    Typed::All
}

/// Type real keys, one per character (see `layout`), for apps that pass key codes on to another
/// system. Shift is pressed as a key of its own around the keys that need it: the other side
/// tracks Shift's key, not the flags on each event. Slower than `type_text`, as remote links
/// drop keys that come too fast.
pub fn type_keys(strokes: &[Stroke]) -> Typed {
    let Some(source) = source() else { return Typed::Nothing };
    let mut shift = false;
    let mut any = false;
    let mut result = Typed::All;
    for stroke in strokes {
        if stroke.shift != shift {
            let flags = if stroke.shift { FLAG_SHIFT } else { 0 };
            if !post(&source, MAC_SHIFT, stroke.shift, flags, None) {
                result = Typed::stopped(any);
                break;
            }
            shift = stroke.shift;
        }
        if !tap_key(&source, stroke.code, if shift { FLAG_SHIFT } else { 0 }, None) {
            result = Typed::stopped(any);
            break;
        }
        any = true;
        std::thread::sleep(Duration::from_millis(8));
    }
    if shift {
        post(&source, MAC_SHIFT, false, 0, None);
    }
    result
}

/// The modifier flags held right now, by anyone (the keyboard, or software posting keys).
fn held_flags() -> u64 {
    // SAFETY: plain query.
    unsafe { ffi::CGEventSourceFlagsState(ffi::STATE_COMBINED_SESSION) }
}

/// Whether the Mac key `code` is held right now, by the keyboard (the hardware state, which an
/// event tap cannot change) or by software posting keys. A key Talkr's tap keeps from the
/// session does not show in the session's own state (seen on CI with posted keys), so the
/// hardware state is asked too.
pub fn key_held(code: u16) -> bool {
    // SAFETY: plain queries.
    unsafe {
        ffi::CGEventSourceKeyState(ffi::STATE_HID_SYSTEM, code) || ffi::CGEventSourceKeyState(ffi::STATE_COMBINED_SESSION, code)
    }
}

/// Whether every key of `shortcut` is held right now. `listener`: whether the listener saw its
/// key go down and not up, when the listener runs. It keeps that key from apps, and a key kept
/// from the session may not show in the session's key state, so its word goes first; the
/// modifiers, which always reach apps, are read from the system.
pub fn shortcut_held(shortcut: &Shortcut, listener: Option<bool>) -> bool {
    let flags = held_flags();
    let held = |on: bool, flag: u64| !on || flags & flag != 0;
    held(shortcut.ctrl, FLAG_CONTROL)
        && held(shortcut.shift, FLAG_SHIFT)
        && held(shortcut.alt, FLAG_OPTION)
        && held(shortcut.win, FLAG_COMMAND)
        && shortcut.key.is_none_or(|vk| match listener {
            Some(held) => held,
            None => keymap::mac_from_vk(vk).is_none_or(key_held),
        })
}

/// Wait until the user has let go of ⌃, ⇧, ⌥ and ⌘, so a paste is not read as ⌃⌘V. Returns
/// whether they were released within `limit`.
pub fn wait_for_modifiers_released(limit: Duration) -> bool {
    let deadline = Instant::now() + limit;
    loop {
        if held_flags() & FLAGS_MODIFIERS == 0 {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nobody is at a CI runner's keyboard: nothing is held.
    #[test]
    fn keyboard_state_on_an_idle_machine() {
        if std::env::var_os("CI").is_none() {
            return;
        }
        let _turn = super::super::pasteboard::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        assert!(!shortcut_held(&Shortcut::ctrl_win(), None));
        assert!(!shortcut_held(&Shortcut::alt_shift_v(), None));
        assert!(!shortcut_held(&Shortcut::alt_shift_v(), Some(true)), "its modifiers are up");
        assert!(wait_for_modifiers_released(Duration::from_millis(100)));
        assert!(!key_held(MAC_V));
    }

    /// A key shortcut's key is judged by the listener while it runs: the key it keeps from the
    /// apps may not show in the system's key state. Without modifiers, only the key counts.
    #[test]
    fn the_listener_answers_for_the_key_it_keeps() {
        let f19 = Shortcut { ctrl: false, shift: false, alt: false, win: false, key: Some(0x82), key_label: None };
        assert!(shortcut_held(&f19, Some(true)));
        assert!(!shortcut_held(&f19, Some(false)));
    }
}
