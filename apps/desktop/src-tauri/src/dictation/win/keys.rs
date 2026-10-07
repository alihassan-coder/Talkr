//! Synthetic keyboard input. Every event Talkr sends carries [`MARKER`] so its own keyboard hook
//! can tell them from the user's typing and never reacts to them.

use std::time::{Duration, Instant};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyNameTextW, MapVirtualKeyW, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
    KEYBD_EVENT_FLAGS, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MAPVK_VK_TO_VSC_EX, VIRTUAL_KEY,
};

/// `dwExtraInfo` of Talkr's own key events: "TKR1".
pub const MARKER: usize = 0x544B_5231;

pub const VK_SHIFT: u16 = 0x10;
pub const VK_CONTROL: u16 = 0x11;
pub const VK_MENU: u16 = 0x12;
pub const VK_RETURN: u16 = 0x0D;
pub const VK_INSERT: u16 = 0x2D;
pub const VK_ESCAPE: u16 = 0x1B;
pub const VK_LWIN: u16 = 0x5B;
pub const VK_RWIN: u16 = 0x5C;
pub const VK_LSHIFT: u16 = 0xA0;
pub const VK_RSHIFT: u16 = 0xA1;
pub const VK_LCONTROL: u16 = 0xA2;
pub const VK_RCONTROL: u16 = 0xA3;
pub const VK_LMENU: u16 = 0xA4;
pub const VK_RMENU: u16 = 0xA5;
pub const VK_V: u16 = 0x56;
/// Unassigned: pressing it tells Windows the Win (or Alt) key was used in a combination, so
/// letting go of it does not open Start (or the menu bar). PowerToys does the same.
pub const VK_DUMMY: u16 = 0xE8;

/// Keys whose scan code needs the extended flag to mean the right key.
fn is_extended(vk: u16) -> bool {
    matches!(
        vk,
        0x21..=0x28 // Page Up/Down, End, Home, arrows
            | 0x2D | 0x2E // Insert, Delete
            | VK_RCONTROL | VK_RMENU | VK_LWIN | VK_RWIN
            | 0x6F // Numpad divide
            | 0x90 // Num Lock
    )
}

fn key_input(vk: u16, up: bool) -> INPUT {
    // SAFETY: MapVirtualKeyW only reads the layout tables.
    let scan = unsafe { MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC_EX) } as u16;
    let mut flags = KEYBD_EVENT_FLAGS(0);
    if up {
        flags |= KEYEVENTF_KEYUP;
    }
    if is_extended(vk) || scan & 0xE000 == 0xE000 {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: VIRTUAL_KEY(vk), wScan: scan & 0xFF, dwFlags: flags, time: 0, dwExtraInfo: MARKER },
        },
    }
}

fn unicode_input(unit: u16, up: bool) -> INPUT {
    let mut flags = KEYEVENTF_UNICODE;
    if up {
        flags |= KEYEVENTF_KEYUP;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VIRTUAL_KEY(0), wScan: unit, dwFlags: flags, time: 0, dwExtraInfo: MARKER } },
    }
}

fn send(inputs: &[INPUT]) -> bool {
    if inputs.is_empty() {
        return true;
    }
    // SAFETY: the slice holds fully initialised INPUT structs of the size passed.
    let sent = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
    sent as usize == inputs.len()
}

/// Press `modifiers`, tap `key`, release the modifiers (in reverse order).
pub fn chord(modifiers: &[u16], key: u16) -> bool {
    let mut inputs: Vec<INPUT> = modifiers.iter().map(|&m| key_input(m, false)).collect();
    inputs.push(key_input(key, false));
    inputs.push(key_input(key, true));
    inputs.extend(modifiers.iter().rev().map(|&m| key_input(m, true)));
    send(&inputs)
}

pub fn tap(key: u16) -> bool {
    send(&[key_input(key, false), key_input(key, true)])
}

/// Mark a held Win or Alt key as used (see [`VK_DUMMY`]).
pub fn neutralize_modifier_release() {
    tap(VK_DUMMY);
}

/// How text gets typed: line breaks as Enter, or as Shift+Enter where Enter would send a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineBreak {
    Enter,
    ShiftEnter,
}

/// Type `text` as Unicode keystrokes, in small batches so slow apps keep up.
pub fn type_text(text: &str, line_break: LineBreak) -> bool {
    const BATCH: usize = 24;
    let mut pending: Vec<INPUT> = Vec::with_capacity(BATCH * 2);
    let flush = |pending: &mut Vec<INPUT>| -> bool {
        let ok = send(pending);
        pending.clear();
        std::thread::sleep(Duration::from_millis(4));
        ok
    };
    for c in text.chars() {
        if c == '\r' {
            continue;
        }
        if c == '\n' {
            if !flush(&mut pending) {
                return false;
            }
            let ok = match line_break {
                LineBreak::Enter => tap(VK_RETURN),
                LineBreak::ShiftEnter => chord(&[VK_SHIFT], VK_RETURN),
            };
            if !ok {
                return false;
            }
            continue;
        }
        let mut units = [0u16; 2];
        for &unit in c.encode_utf16(&mut units).iter() {
            pending.push(unicode_input(unit, false));
            pending.push(unicode_input(unit, true));
        }
        if pending.len() >= BATCH * 2 && !flush(&mut pending) {
            return false;
        }
    }
    flush(&mut pending)
}

pub fn is_down(vk: u16) -> bool {
    // SAFETY: plain query.
    (unsafe { GetAsyncKeyState(vk as i32) } as u16 & 0x8000) != 0
}

const MODIFIERS: [u16; 8] = [VK_LSHIFT, VK_RSHIFT, VK_LCONTROL, VK_RCONTROL, VK_LMENU, VK_RMENU, VK_LWIN, VK_RWIN];

/// Wait until the user has let go of Ctrl, Shift, Alt and Win, so a paste is not read as
/// Ctrl+Win+V. Returns whether they were released within `limit`.
pub fn wait_for_modifiers_released(limit: Duration) -> bool {
    let deadline = Instant::now() + limit;
    loop {
        if !MODIFIERS.iter().any(|&vk| is_down(vk)) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
}

/// The key's name on the current keyboard layout ("Space", "V", "F9").
pub fn key_label(vk: u16) -> String {
    let fixed = match vk {
        0x20 => Some("Space"),
        0x08 => Some("Backspace"),
        0x09 => Some("Tab"),
        0x0D => Some("Enter"),
        0x13 => Some("Pause"),
        0x2D => Some("Insert"),
        0x2E => Some("Delete"),
        0x24 => Some("Home"),
        0x23 => Some("End"),
        0x21 => Some("Page Up"),
        0x22 => Some("Page Down"),
        0x25 => Some("Left"),
        0x26 => Some("Up"),
        0x27 => Some("Right"),
        0x28 => Some("Down"),
        0x91 => Some("Scroll Lock"),
        0xC0 => Some("`"),
        _ => None,
    };
    if let Some(name) = fixed {
        return name.into();
    }
    if (0x70..=0x87).contains(&vk) {
        return format!("F{}", vk - 0x6F);
    }
    // SAFETY: reads the layout tables into a local buffer.
    unsafe {
        let scan = MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC_EX);
        let mut lparam = ((scan & 0xFF) << 16) as i32;
        if is_extended(vk) || scan & 0xE000 == 0xE000 {
            lparam |= 1 << 24;
        }
        let mut buf = [0u16; 32];
        let len = GetKeyNameTextW(lparam, &mut buf);
        if len > 0 {
            let name = String::from_utf16_lossy(&buf[..len as usize]);
            // Letters come back upper case; names like "NUM 5" read better in title case.
            if name.chars().count() > 1 {
                let mut out = String::new();
                for (i, word) in name.split(' ').enumerate() {
                    if i > 0 {
                        out.push(' ');
                    }
                    let mut cs = word.chars();
                    if let Some(f) = cs.next() {
                        out.extend(f.to_uppercase());
                        out.push_str(&cs.as_str().to_lowercase());
                    }
                }
                return out;
            }
            return name;
        }
    }
    format!("Key {:#04x}", vk)
}
