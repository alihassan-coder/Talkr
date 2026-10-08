//! Synthetic keyboard input, and what the keyboard is doing right now. Every event Talkr posts
//! carries [`MARKER`] in its user-data field, so Talkr's own listener never reacts to it.

use std::time::{Duration, Instant};
use super::ffi::{self, Cf};
use super::keymap::{self, FLAGS_MODIFIERS, FLAG_COMMAND, FLAG_CONTROL, FLAG_OPTION, FLAG_SHIFT, MAC_COMMAND, MAC_RETURN, MAC_V};
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

/// Type `text` as key events carrying its characters, a few at a time so slow apps keep up.
pub fn type_text(text: &str, line_break: LineBreak) -> bool {
    let Some(source) = source() else { return false };
    let pause = || std::thread::sleep(Duration::from_millis(4));
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            let flags = if line_break == LineBreak::ShiftReturn { FLAG_SHIFT } else { 0 };
            if !(post(&source, MAC_RETURN, true, flags, None) && post(&source, MAC_RETURN, false, flags, None)) {
                return false;
            }
            pause();
        }
        let units: Vec<u16> = line.trim_end_matches('\r').encode_utf16().collect();
        for chunk in keymap::typing_chunks(&units, 16) {
            // Key code 0 with the text attached: apps take the text, not the key.
            if !(post(&source, 0, true, 0, Some(chunk)) && post(&source, 0, false, 0, Some(chunk))) {
                return false;
            }
            pause();
        }
    }
    true
}

/// The modifier flags held right now, by anyone (the keyboard, or software posting keys).
fn held_flags() -> u64 {
    // SAFETY: plain query.
    unsafe { ffi::CGEventSourceFlagsState(ffi::STATE_COMBINED_SESSION) }
}

/// Whether the Mac key `code` is held right now.
pub fn key_held(code: u16) -> bool {
    // SAFETY: plain query.
    unsafe { ffi::CGEventSourceKeyState(ffi::STATE_COMBINED_SESSION, code) }
}

/// Whether every key of `shortcut` is held right now.
pub fn shortcut_held(shortcut: &Shortcut) -> bool {
    let flags = held_flags();
    let held = |on: bool, flag: u64| !on || flags & flag != 0;
    held(shortcut.ctrl, FLAG_CONTROL)
        && held(shortcut.shift, FLAG_SHIFT)
        && held(shortcut.alt, FLAG_OPTION)
        && held(shortcut.win, FLAG_COMMAND)
        && shortcut.key.is_none_or(|vk| keymap::mac_from_vk(vk).is_none_or(key_held))
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
        assert!(!shortcut_held(&Shortcut::ctrl_win()));
        assert!(!shortcut_held(&Shortcut::alt_shift_v()));
        assert!(wait_for_modifiers_released(Duration::from_millis(100)));
        assert!(!key_held(MAC_V));
    }
}
