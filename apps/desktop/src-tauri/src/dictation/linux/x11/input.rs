//! Synthetic key presses through the XTEST extension: the paste keystroke, and typing for apps
//! that do not take pastes. The shortcut listener ignores keys sent while [`injecting`] is true.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt as _, KEY_PRESS_EVENT, KEY_RELEASE_EVENT};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::wrapper::ConnectionExt as _;
use super::chord::{ALT, CTRL, SHIFT, WIN};
use super::keymap::{Keymap, LOCK_MASK};
use super::keys::{self, Keysym, NO_SYMBOL};
use super::xconn::{Display, Fail};

/// Until when (ms since [`epoch`]) key events are Talkr's own. Set a little past the last one,
/// since the listener reads them from its own connection a moment later.
static INJECTING_UNTIL: AtomicU64 = AtomicU64::new(0);

fn epoch() -> Instant {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    *EPOCH.get_or_init(Instant::now)
}

fn now_ms() -> u64 {
    epoch().elapsed().as_millis() as u64
}

pub fn injecting() -> bool {
    now_ms() < INJECTING_UNTIL.load(Ordering::SeqCst)
}

/// Marks a stretch of synthetic input; ends 250 ms after it is dropped.
struct Injecting;

impl Injecting {
    fn start() -> Self {
        INJECTING_UNTIL.store(u64::MAX, Ordering::SeqCst);
        Injecting
    }
}

impl Drop for Injecting {
    fn drop(&mut self) {
        INJECTING_UNTIL.store(now_ms() + 250, Ordering::SeqCst);
    }
}

fn fake(d: &Display, keycode: u8, down: bool) -> Result<(), String> {
    let kind = if down { KEY_PRESS_EVENT } else { KEY_RELEASE_EVENT };
    d.conn.xtest_fake_input(kind, keycode, x11rb::CURRENT_TIME, x11rb::NONE, 0, 0, 0).x()?;
    Ok(())
}

/// Hand a key the user is holding back to the server as a fresh press, after Talkr's grab has
/// been let go. Replaying the grabbed event instead would skip every passive grab on the root
/// window, the window manager's too, so Ctrl + Super + Left would reach the app rather than
/// switch workspaces. Sent on `d` right after the ungrab, so the server takes them in order.
///
/// The server ignores a press of a key that is already down, so it takes three events: a press
/// (ignored, but it marks the key down on the XTEST device), a release (the key is up: an
/// unmatched release for the focused window, which apps ignore), and the press that counts,
/// matched against every grab with the modifiers the user holds. That press stays down on the
/// XTEST device until [`release_when_up`] lets it go.
pub fn resend_held(d: &Display, keycode: u8) -> Result<(), String> {
    let _injecting = Injecting::start();
    fake(d, keycode, true)?;
    fake(d, keycode, false)?;
    fake(d, keycode, true)?;
    d.conn.flush().x()
}

/// Once the user lets go of a key [`resend_held`] pressed, release it on the XTEST device too,
/// so no key is left down there. (The server already counts the key as up by then: this release
/// changes nothing anyone sees.)
pub fn release_when_up(keycode: u8) {
    std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(30 * 60);
        loop {
            std::thread::sleep(Duration::from_millis(25));
            let down = super::xconn::with(|d| {
                let keys = d.conn.query_keymap().x()?.reply().x()?.keys;
                Ok(keys[keycode as usize / 8] & (1 << (keycode % 8)) != 0)
            });
            if down != Some(true) || Instant::now() >= deadline {
                break;
            }
        }
        super::xconn::with(|d| {
            let _injecting = Injecting::start();
            fake(d, keycode, false)?;
            d.conn.sync().x()
        });
    });
}

/// The modifiers (Ctrl, Shift, Alt, Super) held right now. Reads the pointer's modifier state
/// only, not which keys are down.
pub fn held_modifiers(d: &Display, keymap: &Keymap) -> Result<u8, String> {
    let mask = d.conn.query_pointer(d.root).x()?.reply().x()?.mask;
    Ok(keymap.mods_of(u16::from(mask)))
}

/// Wait until the user lets go of the shortcut, so the paste is not read as Ctrl + Super + V.
pub fn wait_for_modifiers_released(d: &Display, keymap: &Keymap, limit: Duration) -> bool {
    let deadline = Instant::now() + limit;
    loop {
        match held_modifiers(d, keymap) {
            Ok(0) | Err(_) => return true,
            Ok(_) if Instant::now() >= deadline => return false,
            Ok(_) => std::thread::sleep(Duration::from_millis(15)),
        }
    }
}

/// Borrowed keycodes, given back their (empty) mapping when dropped.
struct Borrowed<'a> {
    d: &'a Display,
    per: usize,
    keycodes: Vec<u8>,
}

impl Borrowed<'_> {
    fn map(&self, keycode: u8, ks: Keysym) -> Result<(), String> {
        // The same symbol on every level and group, so no modifier or layout changes it.
        let syms = vec![ks; self.per];
        self.d.conn.change_keyboard_mapping(1, keycode, self.per as u8, &syms).x()?;
        Ok(())
    }
}

impl Drop for Borrowed<'_> {
    fn drop(&mut self) {
        for &kc in &self.keycodes {
            let _ = self.d.conn.change_keyboard_mapping(1, kc, self.per as u8, &vec![NO_SYMBOL; self.per]);
        }
        let _ = self.d.conn.sync();
    }
}

/// A key for `ks`: one that has it on its first level, or a spare key mapped to it.
fn key_for<'a>(d: &'a Display, keymap: &Keymap, ks: Keysym, borrowed: &mut Option<Borrowed<'a>>) -> Result<u8, String> {
    if let Some(kc) = keymap.keycode_for(ks) {
        return Ok(kc);
    }
    let spare = *keymap.spare_keycodes().last().ok_or("no free key to type with")?;
    let b = Borrowed { d, per: keymap.keysyms_per_keycode(), keycodes: vec![spare] };
    b.map(spare, ks)?;
    d.conn.sync().x()?;
    // Apps read the new mapping before the key that uses it; give them a moment.
    std::thread::sleep(Duration::from_millis(30));
    *borrowed = Some(b);
    Ok(spare)
}

fn modifier_keys(keymap: &Keymap, mods: u8) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    for (bit, ks) in [(CTRL, keys::XK_CONTROL_L), (SHIFT, keys::XK_SHIFT_L), (ALT, keys::XK_ALT_L), (WIN, keys::XK_SUPER_L)] {
        if mods & bit != 0 {
            out.push(keymap.keycode_for(ks).ok_or("a modifier key is missing from the keyboard map")?);
        }
    }
    Ok(out)
}

/// Press `mods`, tap the key for `ks`, release the modifiers.
pub fn chord(d: &Display, keymap: &Keymap, mods: u8, ks: Keysym) -> Result<(), String> {
    let _injecting = Injecting::start();
    let modifiers = modifier_keys(keymap, mods)?;
    let mut borrowed = None;
    let key = key_for(d, keymap, ks, &mut borrowed)?;
    for &m in &modifiers {
        fake(d, m, true)?;
    }
    fake(d, key, true)?;
    fake(d, key, false)?;
    for &m in modifiers.iter().rev() {
        fake(d, m, false)?;
    }
    d.conn.sync().x()?;
    // The app must have read the key before its mapping goes back.
    if borrowed.is_some() {
        std::thread::sleep(Duration::from_millis(60));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineBreak {
    Enter,
    /// Where Enter sends a message (chat apps).
    ShiftEnter,
}

/// One step of typing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stroke {
    /// A character, by keysym, typed with a borrowed key.
    Char(Keysym),
    /// A key on the keyboard as it is, with or without Shift.
    Key { ks: Keysym, shift: bool },
}

fn plan(text: &str, line_break: LineBreak) -> Vec<Stroke> {
    text.chars()
        .filter(|&c| c != '\r')
        .map(|c| match c {
            '\n' => Stroke::Key { ks: keys::XK_RETURN, shift: line_break == LineBreak::ShiftEnter },
            '\t' => Stroke::Key { ks: keys::XK_TAB, shift: false },
            c => Stroke::Char(keys::char_to_keysym(c)),
        })
        .collect()
}

/// Type `text`. Each character goes through a spare key remapped to it, so it comes out right
/// whatever the layout; a batch of keys is remapped at once, then typed, then the next batch.
pub fn type_text(d: &Display, keymap: &Keymap, text: &str, line_break: LineBreak) -> Result<(), String> {
    let _injecting = Injecting::start();
    let mut spare = keymap.spare_keycodes();
    spare.truncate(12);
    if spare.is_empty() {
        return Err("no free key to type with".into());
    }
    let shift = keymap.keycode_for(keys::XK_SHIFT_L).ok_or("Shift is missing from the keyboard map")?;
    // Caps Lock would turn borrowed letters upper case: switch it off while typing.
    let caps = held_caps_lock(d)?.then(|| keymap.keycode_for(keys::XK_CAPS_LOCK)).flatten();
    if let Some(caps) = caps {
        fake(d, caps, true)?;
        fake(d, caps, false)?;
    }
    let result = (|| {
        let borrowed = Borrowed { d, per: keymap.keysyms_per_keycode(), keycodes: spare.clone() };
        let strokes = plan(text, line_break);
        let mut rest = strokes.as_slice();
        while !rest.is_empty() {
            // Up to one spare key per distinct character in this batch.
            let mut assigned: Vec<(Keysym, u8)> = Vec::new();
            let mut take = 0;
            for stroke in rest {
                if let Stroke::Char(ks) = *stroke {
                    if !assigned.iter().any(|(k, _)| *k == ks) {
                        if assigned.len() == spare.len() {
                            break;
                        }
                        assigned.push((ks, spare[assigned.len()]));
                    }
                }
                take += 1;
            }
            for &(ks, kc) in &assigned {
                borrowed.map(kc, ks)?;
            }
            d.conn.sync().x()?;
            std::thread::sleep(Duration::from_millis(30));
            for stroke in &rest[..take] {
                match *stroke {
                    Stroke::Char(ks) => {
                        let kc = assigned.iter().find(|(k, _)| *k == ks).map(|(_, kc)| *kc).ok_or("unmapped character")?;
                        fake(d, kc, true)?;
                        fake(d, kc, false)?;
                    }
                    Stroke::Key { ks, shift: with_shift } => {
                        let kc = keymap.keycode_for(ks).ok_or("a key is missing from the keyboard map")?;
                        if with_shift {
                            fake(d, shift, true)?;
                        }
                        fake(d, kc, true)?;
                        fake(d, kc, false)?;
                        if with_shift {
                            fake(d, shift, false)?;
                        }
                    }
                }
                d.conn.flush().x()?;
                std::thread::sleep(Duration::from_millis(3));
            }
            d.conn.sync().x()?;
            // Let apps read this batch before its keys change meaning.
            std::thread::sleep(Duration::from_millis(50));
            rest = &rest[take..];
        }
        Ok(())
    })();
    if let Some(caps) = caps {
        let _ = fake(d, caps, true);
        let _ = fake(d, caps, false);
        let _ = d.conn.sync();
    }
    result
}

fn held_caps_lock(d: &Display) -> Result<bool, String> {
    let mask = d.conn.query_pointer(d.root).x()?.reply().x()?.mask;
    Ok(u16::from(mask) & LOCK_MASK != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_plans_line_breaks_and_characters() {
        let strokes = plan("a\r\nb\tc", LineBreak::ShiftEnter);
        assert_eq!(
            strokes,
            vec![
                Stroke::Char(0x61),
                Stroke::Key { ks: keys::XK_RETURN, shift: true },
                Stroke::Char(0x62),
                Stroke::Key { ks: keys::XK_TAB, shift: false },
                Stroke::Char(0x63),
            ]
        );
        assert_eq!(plan("\n", LineBreak::Enter), vec![Stroke::Key { ks: keys::XK_RETURN, shift: false }]);
    }
}
