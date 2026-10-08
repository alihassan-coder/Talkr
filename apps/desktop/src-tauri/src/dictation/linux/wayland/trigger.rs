//! The shortcut as the GlobalShortcuts portal wants it: a trigger in the XDG "shortcuts" format,
//! modifiers then one XKB key name ("CTRL+LOGO+space").
//!
//! Settings keep Windows virtual-key codes on every system, so the key is translated here. The
//! format has no shortcuts of modifiers alone, so those (Ctrl + Win, the default) get Space.

use crate::dictation::settings::Shortcut;

/// Added to a shortcut of modifiers alone: the portal binds keys, not modifiers.
const STAND_IN_KEY: (&str, &str) = ("space", "Space");

/// A shortcut with no modifiers and no key (never valid in settings) still needs a trigger.
const FALLBACK: &str = "CTRL+ALT+space";

/// What Talkr asks the desktop to bind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trigger {
    /// For the portal: "CTRL+LOGO+space".
    pub spec: String,
    /// For people: "Ctrl + Super + Space".
    pub label: String,
    /// Space stands in for a key the shortcut does not have (or one with no XKB name).
    pub stand_in: bool,
}

/// The trigger for `shortcut`.
pub fn trigger(shortcut: &Shortcut) -> Trigger {
    let mut spec: Vec<String> = Vec::new();
    let mut label: Vec<String> = Vec::new();
    for (on, portal, person) in [
        (shortcut.ctrl, "CTRL", "Ctrl"),
        (shortcut.alt, "ALT", "Alt"),
        (shortcut.shift, "SHIFT", "Shift"),
        (shortcut.win, "LOGO", "Super"),
    ] {
        if on {
            spec.push(portal.into());
            label.push(person.into());
        }
    }
    let key = shortcut.key.and_then(key_name);
    let stand_in = key.is_none();
    if spec.is_empty() && stand_in {
        return Trigger { spec: FALLBACK.into(), label: "Ctrl + Alt + Space".into(), stand_in: true };
    }
    match key {
        Some((name, fallback)) => {
            spec.push(name);
            // The label recorded from the keyboard layout reads better than the US one.
            label.push(shortcut.key_label.clone().filter(|l| !l.trim().is_empty()).unwrap_or(fallback));
        }
        None => {
            spec.push(STAND_IN_KEY.0.into());
            label.push(STAND_IN_KEY.1.into());
        }
    }
    Trigger { spec: spec.join("+"), label: label.join(" + "), stand_in }
}

/// The XKB key name of a Windows virtual-key code, and how to show it.
pub fn key_name(vk: u16) -> Option<(String, String)> {
    let one = |c: u8| Some(((c as char).to_string(), (c as char).to_ascii_uppercase().to_string()));
    match vk {
        0x41..=0x5A => one(vk as u8 + (b'a' - b'A')),
        0x30..=0x39 => one(vk as u8),
        0x60..=0x69 => {
            let digit = vk - 0x60;
            Some((format!("KP_{}", digit), format!("Num {}", digit)))
        }
        0x70..=0x87 => {
            let n = vk - 0x70 + 1;
            Some((format!("F{}", n), format!("F{}", n)))
        }
        _ => NAMED.iter().find(|(code, _, _)| *code == vk).map(|(_, name, label)| ((*name).into(), (*label).into())),
    }
}

/// Keys other than letters, digits, the number pad's digits and F-keys. The punctuation keys are
/// named for a US layout, which is what their virtual-key codes mean.
const NAMED: &[(u16, &str, &str)] = &[
    (0x08, "BackSpace", "Backspace"),
    (0x09, "Tab", "Tab"),
    (0x0D, "Return", "Enter"),
    (0x13, "Pause", "Pause"),
    (0x20, "space", "Space"),
    (0x21, "Prior", "Page Up"),
    (0x22, "Next", "Page Down"),
    (0x23, "End", "End"),
    (0x24, "Home", "Home"),
    (0x25, "Left", "Left"),
    (0x26, "Up", "Up"),
    (0x27, "Right", "Right"),
    (0x28, "Down", "Down"),
    (0x2D, "Insert", "Insert"),
    (0x2E, "Delete", "Delete"),
    (0x6A, "KP_Multiply", "Num *"),
    (0x6B, "KP_Add", "Num +"),
    (0x6D, "KP_Subtract", "Num -"),
    (0x6E, "KP_Decimal", "Num ."),
    (0x6F, "KP_Divide", "Num /"),
    (0x91, "Scroll_Lock", "Scroll Lock"),
    (0xBA, "semicolon", ";"),
    (0xBB, "equal", "="),
    (0xBC, "comma", ","),
    (0xBD, "minus", "-"),
    (0xBE, "period", "."),
    (0xBF, "slash", "/"),
    (0xC0, "grave", "`"),
    (0xDB, "bracketleft", "["),
    (0xDC, "backslash", "\\"),
    (0xDD, "bracketright", "]"),
    (0xDE, "apostrophe", "'"),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn shortcut(ctrl: bool, alt: bool, shift: bool, win: bool, key: Option<u16>) -> Shortcut {
        Shortcut { ctrl, shift, alt, win, key, key_label: None }
    }

    #[test]
    fn modifier_only_shortcuts_get_space() {
        let t = trigger(&Shortcut::ctrl_win());
        assert_eq!(t.spec, "CTRL+LOGO+space");
        assert_eq!(t.label, "Ctrl + Super + Space");
        assert!(t.stand_in);
        let t = trigger(&shortcut(false, true, true, false, None));
        assert_eq!(t.spec, "ALT+SHIFT+space");
        assert!(t.stand_in);
    }

    #[test]
    fn key_chords_keep_their_key() {
        let t = trigger(&Shortcut::alt_shift_v());
        assert_eq!(t.spec, "ALT+SHIFT+v");
        assert_eq!(t.label, "Alt + Shift + V");
        assert!(!t.stand_in);
        let t = trigger(&shortcut(true, false, false, true, Some(0x20)));
        assert_eq!(t.spec, "CTRL+LOGO+space");
        assert!(!t.stand_in);
        assert_eq!(trigger(&shortcut(true, true, true, true, Some(0x37))).spec, "CTRL+ALT+SHIFT+LOGO+7");
    }

    #[test]
    fn standalone_keys_need_no_modifier() {
        let t = trigger(&shortcut(false, false, false, false, Some(0x78)));
        assert_eq!(t.spec, "F9");
        assert_eq!(t.label, "F9");
        assert_eq!(trigger(&shortcut(false, false, false, false, Some(0x87))).spec, "F24");
        assert_eq!(trigger(&shortcut(false, false, false, false, Some(0x13))).spec, "Pause");
        assert_eq!(trigger(&shortcut(false, false, false, false, Some(0x2D))).spec, "Insert");
    }

    #[test]
    fn the_recorded_label_is_shown() {
        // Recorded on a German layout: the key next to 0 is ß there.
        let mut s = shortcut(true, false, false, false, Some(0xDB));
        s.key_label = Some("ß".into());
        let t = trigger(&s);
        assert_eq!(t.spec, "CTRL+bracketleft");
        assert_eq!(t.label, "Ctrl + ß");
        s.key_label = Some("  ".into());
        assert_eq!(trigger(&s).label, "Ctrl + [");
    }

    #[test]
    fn keys_without_a_name_fall_back_to_space() {
        // Volume Up has no place in a portal trigger.
        let t = trigger(&shortcut(true, false, false, true, Some(0xAF)));
        assert_eq!(t.spec, "CTRL+LOGO+space");
        assert!(t.stand_in);
        let t = trigger(&shortcut(false, false, false, false, Some(0xAF)));
        assert_eq!(t.spec, FALLBACK);
        assert!(t.stand_in);
        assert_eq!(trigger(&shortcut(false, false, false, false, None)).spec, FALLBACK);
    }

    #[test]
    fn every_key_class_has_a_name() {
        assert_eq!(key_name(0x41), Some(("a".into(), "A".into())));
        assert_eq!(key_name(0x5A), Some(("z".into(), "Z".into())));
        assert_eq!(key_name(0x30), Some(("0".into(), "0".into())));
        assert_eq!(key_name(0x60), Some(("KP_0".into(), "Num 0".into())));
        assert_eq!(key_name(0x69), Some(("KP_9".into(), "Num 9".into())));
        assert_eq!(key_name(0x70), Some(("F1".into(), "F1".into())));
        assert_eq!(key_name(0xDE), Some(("apostrophe".into(), "'".into())));
        assert_eq!(key_name(0x0D), Some(("Return".into(), "Enter".into())));
        assert_eq!(key_name(0xFF), None);
        for (vk, name, _) in NAMED {
            assert_eq!(key_name(*vk).map(|(n, _)| n).as_deref(), Some(*name));
        }
    }
}
