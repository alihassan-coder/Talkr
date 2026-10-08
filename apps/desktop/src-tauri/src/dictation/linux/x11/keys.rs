//! X keysyms and the Windows virtual-key codes the settings store on every OS, and the names the
//! settings page shows for keys.

pub type Keysym = u32;

pub const NO_SYMBOL: Keysym = 0;
pub const XK_BACKSPACE: Keysym = 0xff08;
pub const XK_TAB: Keysym = 0xff09;
pub const XK_RETURN: Keysym = 0xff0d;
pub const XK_ESCAPE: Keysym = 0xff1b;
pub const XK_INSERT: Keysym = 0xff63;
pub const XK_NUM_LOCK: Keysym = 0xff7f;
pub const XK_SCROLL_LOCK: Keysym = 0xff14;
pub const XK_CAPS_LOCK: Keysym = 0xffe5;
pub const XK_SHIFT_L: Keysym = 0xffe1;
pub const XK_SHIFT_R: Keysym = 0xffe2;
pub const XK_CONTROL_L: Keysym = 0xffe3;
pub const XK_CONTROL_R: Keysym = 0xffe4;
pub const XK_META_L: Keysym = 0xffe7;
pub const XK_META_R: Keysym = 0xffe8;
pub const XK_ALT_L: Keysym = 0xffe9;
pub const XK_ALT_R: Keysym = 0xffea;
pub const XK_SUPER_L: Keysym = 0xffeb;
pub const XK_SUPER_R: Keysym = 0xffec;
pub const XK_V: Keysym = 0x76;

pub const VK_ESCAPE: u16 = 0x1B;
pub const VK_LWIN: u16 = 0x5B;
pub const VK_RWIN: u16 = 0x5C;
pub const VK_LSHIFT: u16 = 0xA0;
pub const VK_RSHIFT: u16 = 0xA1;
pub const VK_LCONTROL: u16 = 0xA2;
pub const VK_RCONTROL: u16 = 0xA3;
pub const VK_LMENU: u16 = 0xA4;
pub const VK_RMENU: u16 = 0xA5;
/// Stands for a key with no virtual-key code (media keys and the like): never part of a
/// shortcut, but it still interrupts one.
pub const VK_UNKNOWN: u16 = 0xFF;

/// Keys whose code does not follow from their keysym's value, with their name. The first entry
/// for a code is the keysym used to find it on the keyboard.
const NAMED: &[(u16, Keysym, &str)] = &[
    (0x08, XK_BACKSPACE, "Backspace"),
    (0x09, XK_TAB, "Tab"),
    (0x09, 0xfe20, "Tab"), // ISO_Left_Tab, Shift + Tab
    (0x0D, XK_RETURN, "Enter"),
    (0x0D, 0xff8d, "Enter"), // KP_Enter
    (0x13, 0xff13, "Pause"),
    (0x14, XK_CAPS_LOCK, "Caps Lock"),
    (VK_ESCAPE, XK_ESCAPE, "Escape"),
    (0x20, 0x20, "Space"),
    (0x21, 0xff55, "Page Up"),
    (0x22, 0xff56, "Page Down"),
    (0x23, 0xff57, "End"),
    (0x24, 0xff50, "Home"),
    (0x25, 0xff51, "Left"),
    (0x26, 0xff52, "Up"),
    (0x27, 0xff53, "Right"),
    (0x28, 0xff54, "Down"),
    (0x2C, 0xff61, "Print Screen"),
    (0x2D, XK_INSERT, "Insert"),
    (0x2E, 0xffff, "Delete"),
    (VK_LWIN, XK_SUPER_L, "Super"),
    (VK_RWIN, XK_SUPER_R, "Super"),
    (0x5D, 0xff67, "Menu"),
    (0x6A, 0xffaa, "Num *"),
    (0x6B, 0xffab, "Num +"),
    (0x6C, 0xffac, "Num ,"),
    (0x6D, 0xffad, "Num -"),
    (0x6E, 0xffae, "Num ."),
    (0x6F, 0xffaf, "Num /"),
    (0x90, XK_NUM_LOCK, "Num Lock"),
    (0x91, XK_SCROLL_LOCK, "Scroll Lock"),
    (VK_LSHIFT, XK_SHIFT_L, "Shift"),
    (VK_RSHIFT, XK_SHIFT_R, "Shift"),
    (VK_LCONTROL, XK_CONTROL_L, "Ctrl"),
    (VK_RCONTROL, XK_CONTROL_R, "Ctrl"),
    (VK_LMENU, XK_ALT_L, "Alt"),
    (VK_LMENU, XK_META_L, "Alt"),
    (VK_RMENU, XK_ALT_R, "Alt"),
    (VK_RMENU, XK_META_R, "Alt"),
    // The US layout's punctuation keys ("OEM" codes on Windows).
    (0xBA, 0x3b, ";"),
    (0xBB, 0x3d, "="),
    (0xBC, 0x2c, ","),
    (0xBD, 0x2d, "-"),
    (0xBE, 0x2e, "."),
    (0xBF, 0x2f, "/"),
    (0xC0, 0x60, "`"),
    (0xDB, 0x5b, "["),
    (0xDC, 0x5c, "\\"),
    (0xDD, 0x5d, "]"),
    (0xDE, 0x27, "'"),
];

/// The virtual-key code of a keysym, if it has one.
pub fn keysym_to_vk(ks: Keysym) -> Option<u16> {
    match ks {
        0x61..=0x7a => Some((ks - 0x20) as u16), // a-z
        0x41..=0x5a | 0x30..=0x39 => Some(ks as u16), // A-Z, 0-9
        0xffbe..=0xffd5 => Some(0x70 + (ks - 0xffbe) as u16), // F1-F24
        0xffb0..=0xffb9 => Some(0x60 + (ks - 0xffb0) as u16), // keypad digits
        _ => NAMED.iter().find(|(_, k, _)| *k == ks).map(|(vk, _, _)| *vk),
    }
}

/// The keysym that stands for a virtual-key code (the inverse of [`keysym_to_vk`]; the backend
/// itself goes from keys to codes only).
#[cfg(test)]
pub fn vk_to_keysym(vk: u16) -> Option<Keysym> {
    let vk32 = vk as u32;
    match vk {
        0x41..=0x5A => Some(vk32 + 0x20), // the lower-case letter is on the key's first level
        0x30..=0x39 => Some(vk32),
        0x70..=0x87 => Some(0xffbe + vk32 - 0x70),
        0x60..=0x69 => Some(0xffb0 + vk32 - 0x60),
        _ => NAMED.iter().find(|(v, _, _)| *v == vk).map(|(_, ks, _)| *ks),
    }
}

/// How the settings page shows a key ("Space", "K", "F9", "Num 5").
pub fn label(vk: u16) -> String {
    match vk {
        0x41..=0x5A | 0x30..=0x39 => char::from(vk as u8).to_string(),
        0x70..=0x87 => format!("F{}", vk - 0x6F),
        0x60..=0x69 => format!("Num {}", vk - 0x60),
        _ => NAMED
            .iter()
            .find(|(v, _, _)| *v == vk)
            .map(|(_, _, name)| name.to_string())
            .unwrap_or_else(|| format!("Key {:#04x}", vk)),
    }
}

/// The keysym that types `c`: Latin-1 keysyms equal their code point, the rest of Unicode is
/// offset by 0x0100_0000.
pub fn char_to_keysym(c: char) -> Keysym {
    let cp = c as u32;
    match cp {
        0x20..=0x7e | 0xa0..=0xff => cp,
        _ => 0x0100_0000 | cp,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_digits_and_function_keys_map_both_ways() {
        assert_eq!(keysym_to_vk(0x61), Some(0x41)); // a
        assert_eq!(keysym_to_vk(0x41), Some(0x41)); // A
        assert_eq!(keysym_to_vk(0x7a), Some(0x5A)); // z
        assert_eq!(keysym_to_vk(0x35), Some(0x35)); // 5
        assert_eq!(keysym_to_vk(0xffbe), Some(0x70)); // F1
        assert_eq!(keysym_to_vk(0xffd5), Some(0x87)); // F24
        assert_eq!(vk_to_keysym(0x56), Some(0x76)); // V -> v
        assert_eq!(vk_to_keysym(0x7B), Some(0xffc9)); // F12
        for vk in (0x30..=0x39).chain(0x41..=0x5A).chain(0x60..=0x69).chain(0x70..=0x87) {
            assert_eq!(vk_to_keysym(vk).and_then(keysym_to_vk), Some(vk), "{vk:#x}");
        }
    }

    #[test]
    fn named_keys_and_modifiers_map_both_ways() {
        for (vk, ks) in [(0x20, 0x20), (0x0D, XK_RETURN), (0x1B, XK_ESCAPE), (0x2D, XK_INSERT), (0x25, 0xff51), (0x91, XK_SCROLL_LOCK)] {
            assert_eq!(keysym_to_vk(ks), Some(vk));
            assert_eq!(vk_to_keysym(vk), Some(ks));
        }
        assert_eq!(keysym_to_vk(XK_SUPER_L), Some(VK_LWIN));
        assert_eq!(keysym_to_vk(XK_SUPER_R), Some(VK_RWIN));
        assert_eq!(keysym_to_vk(XK_CONTROL_R), Some(VK_RCONTROL));
        assert_eq!(keysym_to_vk(XK_META_L), Some(VK_LMENU));
        assert_eq!(keysym_to_vk(0xff8d), Some(0x0D), "keypad Enter is Enter");
        assert_eq!(keysym_to_vk(0x3b), Some(0xBA));
        assert_eq!(keysym_to_vk(0x1008ff14), None, "XF86AudioPlay has no code");
        assert_eq!(keysym_to_vk(0xe9), None, "é has no code of its own");
    }

    #[test]
    fn labels_read_like_on_windows() {
        assert_eq!(label(0x20), "Space");
        assert_eq!(label(0x4B), "K");
        assert_eq!(label(0x78), "F9");
        assert_eq!(label(0x21), "Page Up");
        assert_eq!(label(0x65), "Num 5");
        assert_eq!(label(0xC0), "`");
        assert_eq!(label(0xE8), "Key 0xe8");
    }

    #[test]
    fn characters_become_keysyms() {
        assert_eq!(char_to_keysym('a'), 0x61);
        assert_eq!(char_to_keysym('é'), 0xe9);
        assert_eq!(char_to_keysym('€'), 0x0100_20ac);
        assert_eq!(char_to_keysym('😀'), 0x0101_f600);
    }
}
