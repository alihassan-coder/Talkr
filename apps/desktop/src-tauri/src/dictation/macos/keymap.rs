//! Mac keys in Talkr's portable terms. Settings store Windows virtual-key codes on every system
//! (a shortcut saved on one Mac reads the same on any other), so the listener translates macOS
//! virtual key codes (`kVK_*`, positions on an ANSI keyboard) to Windows codes and back.
//!
//! Keys Windows has no code for (JIS keys, odd function keys) still work: they get a code in a
//! private range above 0xFF, which no Windows key uses.

/// Windows codes for the keys the listener treats specially.
pub const VK_ESCAPE: u16 = 0x1B;
pub const VK_LSHIFT: u16 = 0xA0;
pub const VK_RSHIFT: u16 = 0xA1;
pub const VK_LCONTROL: u16 = 0xA2;
pub const VK_RCONTROL: u16 = 0xA3;
pub const VK_LMENU: u16 = 0xA4;
pub const VK_RMENU: u16 = 0xA5;
pub const VK_LWIN: u16 = 0x5B;
pub const VK_RWIN: u16 = 0x5C;

/// macOS key codes Talkr posts itself.
pub const MAC_V: u16 = 0x09;
pub const MAC_RETURN: u16 = 0x24;
pub const MAC_COMMAND: u16 = 0x37;

/// Codes for Mac keys with no Windows counterpart: `PRIVATE + mac code`.
const PRIVATE: u16 = 0x100;
/// The keypad's Enter. Windows gives it Return's code (telling them apart by a flag), but here
/// it needs its own: a shortcut on one must neither take the other nor be judged by its state.
pub const VK_KEYPAD_ENTER: u16 = PRIVATE + 0x4C;

/// (mac key code, Windows virtual-key code). Each Windows code belongs to one Mac key.
const KEYS: &[(u16, u16)] = &[
    // Letters
    (0x00, 0x41), // A
    (0x0B, 0x42), // B
    (0x08, 0x43), // C
    (0x02, 0x44), // D
    (0x0E, 0x45), // E
    (0x03, 0x46), // F
    (0x05, 0x47), // G
    (0x04, 0x48), // H
    (0x22, 0x49), // I
    (0x26, 0x4A), // J
    (0x28, 0x4B), // K
    (0x25, 0x4C), // L
    (0x2E, 0x4D), // M
    (0x2D, 0x4E), // N
    (0x1F, 0x4F), // O
    (0x23, 0x50), // P
    (0x0C, 0x51), // Q
    (0x0F, 0x52), // R
    (0x01, 0x53), // S
    (0x11, 0x54), // T
    (0x20, 0x55), // U
    (0x09, 0x56), // V
    (0x0D, 0x57), // W
    (0x07, 0x58), // X
    (0x10, 0x59), // Y
    (0x06, 0x5A), // Z
    // Digits
    (0x1D, 0x30),
    (0x12, 0x31),
    (0x13, 0x32),
    (0x14, 0x33),
    (0x15, 0x34),
    (0x17, 0x35),
    (0x16, 0x36),
    (0x1A, 0x37),
    (0x1C, 0x38),
    (0x19, 0x39),
    // Punctuation (US positions)
    (0x29, 0xBA), // ;
    (0x18, 0xBB), // =
    (0x2B, 0xBC), // ,
    (0x1B, 0xBD), // -
    (0x2F, 0xBE), // .
    (0x2C, 0xBF), // /
    (0x32, 0xC0), // `
    (0x21, 0xDB), // [
    (0x2A, 0xDC), // \
    (0x1E, 0xDD), // ]
    (0x27, 0xDE), // '
    (0x0A, 0xE2), // § (ISO keyboards)
    // Editing and navigation
    (0x24, 0x0D), // Return (the keypad's Enter is VK_KEYPAD_ENTER)
    (0x30, 0x09), // Tab
    (0x31, 0x20), // Space
    (0x33, 0x08), // Delete (backspace)
    (0x35, 0x1B), // Escape
    (0x75, 0x2E), // Forward Delete
    (0x72, 0x2D), // Help: Insert on PC keyboards
    (0x73, 0x24), // Home
    (0x77, 0x23), // End
    (0x74, 0x21), // Page Up
    (0x79, 0x22), // Page Down
    (0x7B, 0x25), // Left
    (0x7E, 0x26), // Up
    (0x7C, 0x27), // Right
    (0x7D, 0x28), // Down
    // Modifiers
    (0x38, VK_LSHIFT),
    (0x3C, VK_RSHIFT),
    (0x3B, VK_LCONTROL),
    (0x3E, VK_RCONTROL),
    (0x3A, VK_LMENU),
    (0x3D, VK_RMENU),
    (0x37, VK_LWIN),
    (0x36, VK_RWIN),
    (0x39, 0x14), // Caps Lock
    // Function keys
    (0x7A, 0x70),
    (0x78, 0x71),
    (0x63, 0x72),
    (0x76, 0x73),
    (0x60, 0x74),
    (0x61, 0x75),
    (0x62, 0x76),
    (0x64, 0x77),
    (0x65, 0x78),
    (0x6D, 0x79),
    (0x67, 0x7A),
    (0x6F, 0x7B),
    (0x69, 0x7C), // F13
    (0x6B, 0x7D),
    (0x71, 0x7E),
    (0x6A, 0x7F),
    (0x40, 0x80), // F17
    (0x4F, 0x81),
    (0x50, 0x82),
    (0x5A, 0x83), // F20
    // Keypad
    (0x52, 0x60),
    (0x53, 0x61),
    (0x54, 0x62),
    (0x55, 0x63),
    (0x56, 0x64),
    (0x57, 0x65),
    (0x58, 0x66),
    (0x59, 0x67),
    (0x5B, 0x68),
    (0x5C, 0x69),
    (0x43, 0x6A), // *
    (0x45, 0x6B), // +
    (0x4E, 0x6D), // -
    (0x41, 0x6E), // .
    (0x4B, 0x6F), // /
    (0x47, 0x0C), // Clear
    (0x51, 0x92), // =
    // Media
    (0x4A, 0xAD), // Mute
    (0x49, 0xAE), // Volume Down
    (0x48, 0xAF), // Volume Up
];

/// The Windows code for a Mac key.
pub fn vk_from_mac(code: u16) -> u16 {
    KEYS.iter().find(|(mac, _)| *mac == code).map(|&(_, vk)| vk).unwrap_or(PRIVATE + code)
}

/// The Mac key for a Windows code, if this keyboard has one. The side-neutral modifier codes
/// (Shift, Ctrl, Alt) mean the left key.
pub fn mac_from_vk(vk: u16) -> Option<u16> {
    let vk = match vk {
        0x10 => VK_LSHIFT,
        0x11 => VK_LCONTROL,
        0x12 => VK_LMENU,
        other => other,
    };
    if (PRIVATE..PRIVATE + 0x80).contains(&vk) {
        return Some(vk - PRIVATE);
    }
    KEYS.iter().find(|(_, v)| *v == vk).map(|&(mac, _)| mac)
}

// CGEventFlags: one bit per modifier, and the device-dependent bits (NX_DEVICE*KEYMASK) that
// tell the left key from the right.
pub const FLAG_SHIFT: u64 = 0x0002_0000;
pub const FLAG_CONTROL: u64 = 0x0004_0000;
pub const FLAG_OPTION: u64 = 0x0008_0000;
pub const FLAG_COMMAND: u64 = 0x0010_0000;
pub const FLAGS_MODIFIERS: u64 = FLAG_SHIFT | FLAG_CONTROL | FLAG_OPTION | FLAG_COMMAND;
const DEVICE_LCONTROL: u64 = 0x0000_0001;
const DEVICE_LSHIFT: u64 = 0x0000_0002;
const DEVICE_RSHIFT: u64 = 0x0000_0004;
const DEVICE_LCOMMAND: u64 = 0x0000_0008;
const DEVICE_RCOMMAND: u64 = 0x0000_0010;
const DEVICE_LOPTION: u64 = 0x0000_0020;
const DEVICE_ROPTION: u64 = 0x0000_0040;
const DEVICE_RCONTROL: u64 = 0x0000_2000;

/// For a modifier's Windows code: its own device bit, the device bits of both its keys, and its
/// device-independent flag.
fn modifier_flags(vk: u16) -> Option<(u64, u64, u64)> {
    let control = DEVICE_LCONTROL | DEVICE_RCONTROL;
    let shift = DEVICE_LSHIFT | DEVICE_RSHIFT;
    let option = DEVICE_LOPTION | DEVICE_ROPTION;
    let command = DEVICE_LCOMMAND | DEVICE_RCOMMAND;
    Some(match vk {
        VK_LCONTROL => (DEVICE_LCONTROL, control, FLAG_CONTROL),
        VK_RCONTROL => (DEVICE_RCONTROL, control, FLAG_CONTROL),
        VK_LSHIFT => (DEVICE_LSHIFT, shift, FLAG_SHIFT),
        VK_RSHIFT => (DEVICE_RSHIFT, shift, FLAG_SHIFT),
        VK_LMENU => (DEVICE_LOPTION, option, FLAG_OPTION),
        VK_RMENU => (DEVICE_ROPTION, option, FLAG_OPTION),
        VK_LWIN => (DEVICE_LCOMMAND, command, FLAG_COMMAND),
        VK_RWIN => (DEVICE_RCOMMAND, command, FLAG_COMMAND),
        _ => return None,
    })
}

/// What an event's flags say about the modifier `vk`: down, up, or `None` when they cannot tell
/// (a key of its kind is down, but the event carries no device bits to say which: some
/// synthetic events, some remote keyboards). The outer `None` is for keys that are no modifier.
fn modifier_flags_say(vk: u16, flags: u64) -> Option<Option<bool>> {
    let (own, both, generic) = modifier_flags(vk)?;
    Some(if flags & generic == 0 {
        Some(false)
    } else if flags & both != 0 {
        Some(flags & own != 0)
    } else {
        None
    })
}

/// Whether the modifier `vk` is down according to an event's flags; `None` for other keys.
/// Without device bits the flags only say whether either key of its kind is down.
pub fn modifier_down(vk: u16, flags: u64) -> Option<bool> {
    modifier_flags_say(vk, flags).map(|said| said.unwrap_or(true))
}

/// A FlagsChanged event: which modifier changed (as a Windows code) and whether it is now down.
/// `None` for keys the listener ignores: Caps Lock (a toggle, not held) and Fn. `was_down` is
/// the listener's own view of the key: a FlagsChanged event means it changed, so where the
/// flags cannot tell (no device bits, and the other key of its kind may hold the flag) it went
/// the other way from where the listener last saw it.
pub fn modifier_change(code: u16, flags: u64, was_down: bool) -> Option<(u16, bool)> {
    let vk = vk_from_mac(code);
    modifier_flags_say(vk, flags).map(|said| (vk, said.unwrap_or(!was_down)))
}

/// The label a key typed, from the character the layout produced for it, when that is a
/// plain visible character ("Z" on a German keyboard where the US one has "Y"). `None` when
/// Option was held (it types special characters), when Shift turned the key into another symbol
/// (⇧1 types "!", but the key is "1"), or when the key typed nothing printable.
pub fn label_from_typed(typed: &str, flags: u64) -> Option<String> {
    if flags & FLAG_OPTION != 0 {
        return None;
    }
    let mut chars = typed.chars();
    let c = chars.next()?;
    if chars.next().is_some() || c.is_control() || c.is_whitespace() || ('\u{E000}'..='\u{F8FF}').contains(&c) {
        return None;
    }
    if flags & FLAG_SHIFT != 0 && !c.is_alphabetic() {
        return None;
    }
    Some(c.to_uppercase().collect())
}

/// How a key is shown when the layout did not say ("Space", "F5", "A" on a US keyboard).
pub fn key_label(vk: u16) -> String {
    let fixed = match vk {
        0x20 => "Space",
        0x08 => "Delete",
        0x2E => "Forward Delete",
        0x09 => "Tab",
        0x0D => "Return",
        VK_KEYPAD_ENTER => "Num Enter",
        0x1B => "Esc",
        0x2D => "Insert",
        0x24 => "Home",
        0x23 => "End",
        0x21 => "Page Up",
        0x22 => "Page Down",
        0x25 => "Left",
        0x26 => "Up",
        0x27 => "Right",
        0x28 => "Down",
        0x14 => "Caps Lock",
        0x0C => "Clear",
        0xBA => ";",
        0xBB => "=",
        0xBC => ",",
        0xBD => "-",
        0xBE => ".",
        0xBF => "/",
        0xC0 => "`",
        0xDB => "[",
        0xDC => "\\",
        0xDD => "]",
        0xDE => "'",
        0xE2 => "§",
        0x6A => "Num *",
        0x6B => "Num +",
        0x6D => "Num -",
        0x6E => "Num .",
        0x6F => "Num /",
        0x92 => "Num =",
        0xAD => "Mute",
        0xAE => "Volume Down",
        0xAF => "Volume Up",
        _ => "",
    };
    if !fixed.is_empty() {
        return fixed.into();
    }
    match vk {
        0x30..=0x39 | 0x41..=0x5A => char::from(vk as u8).to_string(),
        0x60..=0x69 => format!("Num {}", vk - 0x60),
        0x70..=0x87 => format!("F{}", vk - 0x6F),
        _ => match mac_from_vk(vk) {
            Some(code) if vk >= PRIVATE => format!("Key {}", code),
            _ => format!("Key {:#04x}", vk),
        },
    }
}

/// Text split for typing: each key event carries at most `max` UTF-16 units (macOS takes up to
/// 20), and a character is never split across two events.
pub fn typing_chunks(units: &[u16], max: usize) -> Vec<&[u16]> {
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < units.len() {
        let mut end = (start + max.max(2)).min(units.len());
        if end < units.len() && (0xD800..=0xDBFF).contains(&units[end - 1]) {
            end -= 1;
        }
        chunks.push(&units[start..end]);
        start = end;
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_never_splits_a_character() {
        let text: Vec<u16> = "ab😀cd😀😀e".encode_utf16().collect();
        for max in 2..8 {
            let chunks = typing_chunks(&text, max);
            assert!(chunks.iter().all(|c| !c.is_empty() && c.len() <= max));
            let joined: Vec<u16> = chunks.concat();
            assert_eq!(joined, text);
            for c in &chunks {
                assert!(String::from_utf16(c).is_ok(), "max {max}: {c:?}");
            }
        }
        assert!(typing_chunks(&[], 16).is_empty());
        let long: Vec<u16> = "x".repeat(40).encode_utf16().collect();
        assert_eq!(typing_chunks(&long, 16).len(), 3);
    }

    #[test]
    fn letters_digits_and_function_keys_map_to_windows_codes() {
        assert_eq!(vk_from_mac(0x00), 0x41, "A");
        assert_eq!(vk_from_mac(0x06), 0x5A, "Z");
        assert_eq!(vk_from_mac(0x09), 0x56, "V");
        assert_eq!(vk_from_mac(0x1D), 0x30, "0");
        assert_eq!(vk_from_mac(0x19), 0x39, "9");
        assert_eq!(vk_from_mac(0x7A), 0x70, "F1");
        assert_eq!(vk_from_mac(0x6F), 0x7B, "F12");
        assert_eq!(vk_from_mac(0x5A), 0x83, "F20");
        assert_eq!(vk_from_mac(0x31), 0x20, "Space");
        assert_eq!(vk_from_mac(0x35), VK_ESCAPE);
        assert_eq!(vk_from_mac(0x37), VK_LWIN, "Command is the Windows key");
        assert_eq!(vk_from_mac(0x3D), VK_RMENU, "right Option is right Alt");
    }

    #[test]
    fn every_letter_and_digit_is_mapped_once() {
        for vk in (0x41..=0x5A).chain(0x30..=0x39).chain(0x70..=0x83) {
            let macs: Vec<_> = KEYS.iter().filter(|(_, v)| *v == vk).collect();
            assert_eq!(macs.len(), 1, "{vk:#x}");
        }
        let mut codes: Vec<u16> = KEYS.iter().map(|(m, _)| *m).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), KEYS.len(), "a Mac key listed twice");
    }

    #[test]
    fn mapping_round_trips() {
        for code in 0u16..0x80 {
            let vk = vk_from_mac(code);
            assert_eq!(mac_from_vk(vk), Some(code), "{code:#x} -> {vk:#x}");
        }
        assert_eq!(mac_from_vk(0x11), Some(0x3B), "Ctrl means left Control");
        assert_eq!(mac_from_vk(0x10), Some(0x38));
        assert_eq!(mac_from_vk(0x12), Some(0x3A));
        assert_eq!(mac_from_vk(0x56), Some(MAC_V));
        assert_eq!(mac_from_vk(0x5B), Some(MAC_COMMAND));
        assert_eq!(mac_from_vk(0xFF), None);
    }

    /// Return and the keypad's Enter are two keys: a shortcut on one is not the other.
    #[test]
    fn keypad_enter_is_not_return() {
        assert_eq!(vk_from_mac(MAC_RETURN), 0x0D);
        assert_eq!(vk_from_mac(0x4C), VK_KEYPAD_ENTER);
        assert_eq!(mac_from_vk(0x0D), Some(MAC_RETURN));
        assert_eq!(mac_from_vk(VK_KEYPAD_ENTER), Some(0x4C));
        assert_eq!(key_label(VK_KEYPAD_ENTER), "Num Enter");
        assert_eq!(key_label(0x0D), "Return");
    }

    #[test]
    fn unknown_mac_keys_get_private_codes() {
        // JIS Eisu has no Windows code.
        let vk = vk_from_mac(0x66);
        assert!(vk > 0xFF);
        assert_eq!(mac_from_vk(vk), Some(0x66));
        assert_eq!(key_label(vk), "Key 102");
    }

    #[test]
    fn labels() {
        assert_eq!(key_label(0x20), "Space");
        assert_eq!(key_label(0x56), "V");
        assert_eq!(key_label(0x35), "5");
        assert_eq!(key_label(0x74), "F5");
        assert_eq!(key_label(0x87), "F24");
        assert_eq!(key_label(0x63), "Num 3");
        assert_eq!(key_label(0xBC), ",");
        assert_eq!(key_label(0x2D), "Insert");
        for vk in 0..0x100u16 {
            let label = key_label(vk);
            assert!(!label.is_empty() && label.len() <= 32 && !label.chars().any(char::is_control), "{vk:#x}");
        }
    }

    #[test]
    fn labels_follow_the_layout_when_they_can() {
        assert_eq!(label_from_typed("z", 0).as_deref(), Some("Z"));
        assert_eq!(label_from_typed("ö", 0).as_deref(), Some("Ö"));
        assert_eq!(label_from_typed("Z", FLAG_SHIFT).as_deref(), Some("Z"), "Shift + a letter is the letter");
        assert_eq!(label_from_typed("z", FLAG_COMMAND).as_deref(), Some("Z"));
        assert_eq!(label_from_typed("!", FLAG_SHIFT), None, "Shift + 1 is the 1 key");
        assert_eq!(label_from_typed("é", FLAG_OPTION), None, "Option types special characters");
        assert_eq!(label_from_typed("\u{1a}", FLAG_CONTROL), None, "Control types control characters");
        assert_eq!(label_from_typed(" ", 0), None);
        assert_eq!(label_from_typed("\u{F704}", 0), None, "function keys type private characters");
        assert_eq!(label_from_typed("", 0), None);
        assert_eq!(label_from_typed("ab", 0), None);
    }

    #[test]
    fn modifiers_from_flags() {
        // Left Command down (generic flag and its device bit).
        let flags = FLAG_COMMAND | DEVICE_LCOMMAND;
        assert_eq!(modifier_change(0x37, flags, false), Some((VK_LWIN, true)));
        assert_eq!(modifier_down(VK_RWIN, flags), Some(false));
        // Both Shifts down, then the left one released: the device bits tell them apart, whatever
        // the listener thought.
        let both = FLAG_SHIFT | DEVICE_LSHIFT | DEVICE_RSHIFT;
        assert_eq!(modifier_change(0x38, both, true), Some((VK_LSHIFT, true)));
        assert_eq!(modifier_change(0x38, FLAG_SHIFT | DEVICE_RSHIFT, false), Some((VK_LSHIFT, false)));
        // Released completely.
        assert_eq!(modifier_change(0x3C, 0, true), Some((VK_RSHIFT, false)));
        assert_eq!(modifier_change(0x3E, FLAG_CONTROL | DEVICE_RCONTROL, false), Some((VK_RCONTROL, true)));
        assert_eq!(modifier_change(0x3A, 0x100, true), Some((VK_LMENU, false)));
        // Caps Lock and Fn are not modifiers here.
        assert_eq!(modifier_change(0x39, 0x0001_0000, false), None);
        assert_eq!(modifier_change(0x3F, 0x0080_0000, false), None);
        assert_eq!(modifier_down(0x41, FLAGS_MODIFIERS), None);
    }

    /// Events without device bits: the generic flag stays set while either key of a kind is
    /// down, so the listener's own view says which way the key went.
    #[test]
    fn modifiers_without_device_bits_follow_the_listener() {
        // Left Control pressed: the flag appears.
        assert_eq!(modifier_change(0x3B, FLAG_CONTROL, false), Some((VK_LCONTROL, true)));
        // Right Control pressed too: the flag stays.
        assert_eq!(modifier_change(0x3E, FLAG_CONTROL, false), Some((VK_RCONTROL, true)));
        // Left Control let go while the right one holds the flag: a release, not a press.
        assert_eq!(modifier_change(0x3B, FLAG_CONTROL, true), Some((VK_LCONTROL, false)));
        // Without a history, the flags say only that some Control key is down.
        assert_eq!(modifier_down(VK_LCONTROL, FLAG_CONTROL), Some(true));
    }
}
