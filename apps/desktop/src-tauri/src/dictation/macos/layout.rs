//! The keyboard layout read backwards: which key, with or without Shift, types each character.
//!
//! Remote desktops and virtual machines (`target::TYPE_APPS`) pass on key codes, not the
//! characters a key event carries, so text typed there the usual way (every event on one key,
//! carrying its own characters) arrives as a row of identical key presses. There, each character
//! is typed as the real key that makes it on the Mac's current layout, which is right as long as
//! the remote system uses the same layout. Characters no single key makes (accents built with a
//! dead key, emoji) cannot be typed that way: the text is then copied instead.

use std::collections::HashMap;
use std::ffi::c_ulong;
use std::time::Duration;
use super::ffi::{self, Cf};
use super::keymap::MAC_RETURN;
use super::target::LineBreak;

/// One key press: a Mac key code, with Shift or without.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stroke {
    pub code: u16,
    pub shift: bool,
}

/// The keypad: its keys depend on Num Lock on the other side, so they are never used for text.
const KEYPAD: &[u16] = &[
    0x41, 0x43, 0x45, 0x47, 0x4B, 0x4C, 0x4E, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5B, 0x5C, 0x5F,
];

/// For each character the layout makes with one key, that key.
#[derive(Debug, Default)]
pub struct KeyTable(HashMap<char, Stroke>);

impl KeyTable {
    /// Build the table from `translate`: what a key types, alone or with Shift (`None` for a dead
    /// key or nothing). Option is never used: on the other side it is Alt, which opens menus.
    /// Where two keys type the same character, the one without Shift and the lower code wins.
    pub fn build(translate: impl Fn(u16, bool) -> Option<String>) -> KeyTable {
        let mut table = HashMap::new();
        for shift in [false, true] {
            for code in (0u16..0x80).filter(|c| !KEYPAD.contains(c)) {
                let Some(typed) = translate(code, shift) else { continue };
                let mut chars = typed.chars();
                let (Some(c), None) = (chars.next(), chars.next()) else { continue };
                // Control characters come from Return, Tab, Escape and the like; the private
                // range from function and arrow keys.
                if c.is_control() || ('\u{E000}'..='\u{F8FF}').contains(&c) {
                    continue;
                }
                table.entry(c).or_insert(Stroke { code, shift });
            }
        }
        KeyTable(table)
    }

    /// The key presses that type `text`, line breaks as Return (or Shift + Return); `None` when a
    /// character has no key, so nothing is typed at all rather than part of it.
    pub fn plan(&self, text: &str, line_break: LineBreak) -> Option<Vec<Stroke>> {
        let mut strokes = Vec::with_capacity(text.len());
        for c in text.chars() {
            match c {
                '\r' => {}
                '\n' => strokes.push(Stroke { code: MAC_RETURN, shift: line_break == LineBreak::ShiftReturn }),
                c => strokes.push(*self.0.get(&c)?),
            }
        }
        Some(strokes)
    }
}

/// The table for the current keyboard layout. macOS only lets the main thread ask for the
/// layout (elsewhere the process is stopped), so it is read there, through `owner`'s event loop.
/// `None` when that cannot be done in time or the layout has no Unicode table.
pub fn current(owner: Option<&tauri::WebviewWindow>) -> Option<KeyTable> {
    if objc2::MainThreadMarker::new().is_some() {
        return read_current();
    }
    let (tx, rx) = flume::bounded(1);
    owner?.run_on_main_thread(move || {
        let _ = tx.send(read_current());
    })
    .ok()?;
    rx.recv_timeout(Duration::from_secs(1)).ok().flatten()
}

/// Read the current layout's table. Main thread only.
fn read_current() -> Option<KeyTable> {
    // kUCKeyActionDown; Shift as UCKeyTranslate takes modifiers ((shiftKey >> 8) & 0xFF).
    const ACTION_DOWN: u16 = 0;
    const SHIFT: u32 = 0x02;
    // SAFETY: the input source is owned and outlives every use of its layout data, which
    // TISGetInputSourceProperty returns unretained (NULL when the layout has none).
    unsafe {
        let source = Cf::from_owned(ffi::TISCopyCurrentKeyboardLayoutInputSource())?;
        let data = ffi::TISGetInputSourceProperty(source.as_ptr(), ffi::kTISPropertyUnicodeKeyLayoutData);
        if data.is_null() {
            return None;
        }
        let layout = ffi::CFDataGetBytePtr(data);
        if layout.is_null() {
            return None;
        }
        let keyboard = u32::from(ffi::LMGetKbdType());
        Some(KeyTable::build(|code, shift| {
            let mut dead_key = 0u32;
            let mut buf = [0u16; 4];
            let mut len: c_ulong = 0;
            let status = ffi::UCKeyTranslate(
                layout.cast(),
                code,
                ACTION_DOWN,
                if shift { SHIFT } else { 0 },
                keyboard,
                0,
                &mut dead_key,
                buf.len() as c_ulong,
                &mut len,
                buf.as_mut_ptr(),
            );
            // A dead key types nothing yet; on the other side it would change the next letter.
            (status == 0 && dead_key == 0 && len > 0).then(|| String::from_utf16_lossy(&buf[..(len as usize).min(buf.len())]))
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small US-like layout: letters, digits, a few symbols, Return, F1 and the keypad's 1.
    /// Every other key is dead or types nothing.
    fn us(code: u16, shift: bool) -> Option<String> {
        let (plain, shifted) = match code {
            0x00 => ("a", "A"),
            0x0B => ("b", "B"),
            0x12 => ("1", "!"),
            0x31 => (" ", " "),
            0x2B => (",", "<"),
            0x24 => ("\r", "\r"),
            0x7A => ("\u{F704}", "\u{F704}"),
            0x53 => ("1", "1"),
            _ => return None,
        };
        Some(if shift { shifted } else { plain }.to_string())
    }

    #[test]
    fn each_character_gets_the_key_that_types_it() {
        let table = KeyTable::build(us);
        let s = |code, shift| Stroke { code, shift };
        assert_eq!(table.plan("a B", LineBreak::Return), Some(vec![s(0x00, false), s(0x31, false), s(0x0B, true)]));
        assert_eq!(table.plan("1!", LineBreak::Return), Some(vec![s(0x12, false), s(0x12, true)]), "the main row, not the keypad");
        assert_eq!(table.plan("", LineBreak::Return), Some(vec![]));
    }

    #[test]
    fn line_breaks_are_return_keys() {
        let table = KeyTable::build(us);
        let ret = |shift| Stroke { code: MAC_RETURN, shift };
        assert_eq!(table.plan("a\r\nb", LineBreak::Return).unwrap()[1], ret(false));
        assert_eq!(table.plan("a\nb", LineBreak::ShiftReturn).unwrap()[1], ret(true), "chat apps");
    }

    /// Characters no single key types: nothing is typed, the text is copied instead.
    #[test]
    fn untypable_text_is_refused_whole() {
        let table = KeyTable::build(us);
        assert_eq!(table.plan("abc", LineBreak::Return), None, "no key types c");
        assert_eq!(table.plan("a é", LineBreak::Return), None);
        assert_eq!(table.plan("a 😀", LineBreak::Return), None);
        assert_eq!(table.plan("\u{F704}", LineBreak::Return), None, "function keys type no text");
        assert!(KeyTable::build(|_, _| None).plan("a", LineBreak::Return).is_none());
    }

    /// Off the main thread and without a window to reach it, the layout cannot be read: no
    /// table (and no crash).
    #[test]
    fn without_the_main_thread_there_is_no_table() {
        if objc2::MainThreadMarker::new().is_none() {
            assert!(current(None).is_none());
        }
    }
}
