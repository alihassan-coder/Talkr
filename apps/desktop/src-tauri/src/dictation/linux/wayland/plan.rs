//! The Wayland backend's decisions, kept apart from the portals so they can be tested: what
//! dictation can do on this desktop, and how a text gets into the field.

use crate::dictation::backend::Capabilities;
use crate::dictation::settings::InsertMethod;

/// What this desktop offers. Probed once (see `portal::availability`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Availability {
    /// The GlobalShortcuts portal: the shortcut's press and release.
    pub shortcuts: bool,
    /// The RemoteDesktop portal, with a keyboard: typing into other apps.
    pub keyboard: bool,
    /// The Clipboard portal, used on the RemoteDesktop session (GNOME).
    pub portal_clipboard: bool,
    /// The wlr or ext data-control protocol: the clipboard without a focused window (wlroots
    /// compositors, KDE).
    pub data_control: bool,
    /// Data-control can also set the primary selection.
    pub primary: bool,
}

/// What the settings page shows. `shortcut` describes the shortcut the desktop bound, if known;
/// `hyprland` is the hyprland.conf line that binds it, on Hyprland (which binds nothing itself).
/// `no_releases`: the desktop turned out not to report the shortcut's release, so it toggles.
pub fn capabilities(av: &Availability, shortcut: Option<&str>, hyprland: Option<&str>, no_releases: bool) -> Capabilities {
    Capabilities {
        os: "linux",
        // Without the portal, `talkr --dictate` bound to a key in the system settings works.
        supported: true,
        hold_to_talk: av.shortcuts && !no_releases,
        modifier_only: false,
        records_shortcut: false,
        verifies_insertion: false,
        inserts_text: av.keyboard,
        meta_key: "Super",
        note: Some(note(av, shortcut, hyprland, no_releases)),
    }
}

fn note(av: &Availability, shortcut: Option<&str>, hyprland: Option<&str>, no_releases: bool) -> String {
    let hold = if no_releases {
        "this desktop does not report when it is released, so press it once to start and again to finish"
    } else {
        "hold it to dictate"
    };
    let shortcut = match (av.shortcuts, shortcut, hyprland) {
        (true, _, Some(line)) => format!(
            "Wayland session on Hyprland: bind the shortcut in hyprland.conf, e.g. `{}` (`hyprctl globalshortcuts` \
             lists Talkr's); {}.",
            line, hold
        ),
        (true, Some(s), None) => format!(
            "Wayland session: the shortcut is {}, bound by your desktop (change it in its keyboard settings); {}.",
            s, hold
        ),
        (true, None, None) => format!(
            "Wayland session: your desktop binds the shortcut (it may ask you to confirm it; change it in its \
             keyboard settings); {}.",
            hold
        ),
        (false, _, _) => "Wayland session without a global-shortcuts portal: bind the command `talkr --dictate` to a key in \
                       your keyboard settings; it starts and stops dictation."
            .to_string(),
    };
    let insert = if av.keyboard {
        "Text is typed or pasted through the remote-desktop portal into the window that has focus when it is ready; \
         Escape cannot cancel here (use the pill)."
    } else if av.data_control || av.portal_clipboard {
        "This desktop does not let apps type into other apps, so the text is copied for you to paste."
    } else {
        "This desktop does not let apps type into other apps or set the clipboard, so the text waits in Talkr's \
         History."
    };
    let mut note = format!("{} {}", shortcut, insert);
    if av.keyboard && (av.data_control || av.portal_clipboard) {
        // On Wayland the app that set the clipboard serves it; nothing hands it back to the app
        // the user copied from.
        note.push_str(
            " After pasting, Talkr puts your clipboard back and keeps it available itself: if Talkr quits before you \
             copy something else, that clipboard is gone (unless a clipboard manager kept it).",
        );
    }
    note
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// Put the text on the clipboard and press Shift + Insert.
    Paste,
    /// Press a key for each character.
    Type,
}

impl Method {
    pub fn name(self) -> &'static str {
        match self {
            Method::Paste => "paste",
            Method::Type => "type",
        }
    }
}

/// What inserting can rely on right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Means {
    /// The RemoteDesktop session is running.
    pub keyboard: bool,
    /// Talkr can set the clipboard.
    pub clipboard: bool,
    /// The user's clipboard was saved, or does not need to be.
    pub keeps_clipboard: bool,
    /// Talkr can set the primary selection too.
    pub primary: bool,
    /// The text is plain ASCII, which types on any layout.
    pub plain: bool,
}

/// How to insert, or `None` without keyboard access.
///
/// Pasting is preferred: typing goes key by key, and a character missing from the keyboard layout
/// may not type at all on some compositors (GNOME looks keysyms up in the layout). But Shift +
/// Insert pastes the primary selection in terminals (VTE, kitty, Alacritty, foot, Konsole): where
/// Talkr cannot set it, Auto types a text that types everywhere instead of risking a stale paste.
/// Without a way to keep the user's clipboard, Auto types instead of losing it. Per-app rules
/// cannot apply: Wayland does not say which app has focus.
pub fn choose(setting: InsertMethod, can: Means) -> Option<Method> {
    if !can.keyboard {
        return None;
    }
    Some(match setting {
        InsertMethod::Type => Method::Type,
        InsertMethod::Paste if can.clipboard => Method::Paste,
        InsertMethod::Auto if can.clipboard && can.keeps_clipboard && (can.primary || !can.plain) => Method::Paste,
        InsertMethod::Paste | InsertMethod::Auto => Method::Type,
    })
}

/// The text types on every layout: printable ASCII, line breaks and tabs.
pub fn plain(text: &str) -> bool {
    text.chars().all(|c| matches!(c, ' '..='~' | '\n' | '\r' | '\t'))
}

/// Why a text was copied instead of typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyReason {
    /// The desktop has no RemoteDesktop portal.
    NoPortal,
    /// The user has not allowed keyboard access (yet).
    NotAllowed,
    /// Typing or pasting failed before anything went in.
    Failed,
    /// Typing stopped part-way.
    Interrupted,
    /// Paste was pressed, but no app asked for the text.
    NotPasted,
}

impl CopyReason {
    pub fn message(self) -> &'static str {
        match self {
            CopyReason::NoPortal => "This desktop does not let apps type into other apps",
            CopyReason::NotAllowed => "Allow keyboard access in Talkr › Dictation to type into other apps",
            CopyReason::Failed => "The desktop did not accept the keystrokes",
            CopyReason::Interrupted => "Typing stopped part-way; the whole text was copied",
            CopyReason::NotPasted => "The focused app did not take the paste",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EVERYTHING: Availability =
        Availability { shortcuts: true, keyboard: true, portal_clipboard: true, data_control: true, primary: true };

    const ALL: Means = Means { keyboard: true, clipboard: true, keeps_clipboard: true, primary: true, plain: true };

    #[test]
    fn dictation_is_always_supported() {
        // The `talkr --dictate` fallback works even with no portal at all.
        let none = capabilities(&Availability::default(), None, None, false);
        assert!(none.supported);
        assert!(!none.hold_to_talk && !none.inserts_text);
        assert!(none.note.as_deref().unwrap().contains("talkr --dictate"));
        assert!(none.note.as_deref().unwrap().contains("History"));
    }

    #[test]
    fn the_portals_decide_the_capabilities() {
        let all = capabilities(&EVERYTHING, Some("Ctrl + Super + Space"), None, false);
        assert!(all.supported && all.hold_to_talk && all.inserts_text);
        assert!(!all.modifier_only && !all.records_shortcut && !all.verifies_insertion);
        assert_eq!(all.os, "linux");
        assert_eq!(all.meta_key, "Super");
        let note = all.note.unwrap();
        assert!(note.contains("Ctrl + Super + Space"), "{}", note);
        assert!(note.contains("hold it"), "{}", note);
        assert!(!note.contains("--dictate"));
        // Talkr serves the restored clipboard: the page says what that means.
        assert!(note.contains("quits"), "{}", note);

        let shortcuts_only = capabilities(&Availability { shortcuts: true, ..Availability::default() }, None, None, false);
        assert!(shortcuts_only.hold_to_talk && !shortcuts_only.inserts_text);
        let note = shortcuts_only.note.unwrap();
        assert!(note.contains("confirm") && !note.contains("quits"), "{}", note);

        let typing_only = capabilities(&Availability { keyboard: true, ..Availability::default() }, None, None, false);
        assert!(!typing_only.hold_to_talk && typing_only.inserts_text);

        let copy_only = capabilities(&Availability { data_control: true, ..Availability::default() }, None, None, false);
        assert!(!copy_only.inserts_text);
        assert!(copy_only.note.unwrap().contains("copied"));
    }

    #[test]
    fn without_releases_there_is_no_hold_to_talk() {
        let caps = capabilities(&EVERYTHING, Some("Ctrl + Super + Space"), None, true);
        assert!(!caps.hold_to_talk);
        let note = caps.note.unwrap();
        assert!(note.contains("press it once to start") && !note.contains("hold it"), "{}", note);
    }

    #[test]
    fn hyprland_is_told_how_to_bind() {
        let line = "bind = CTRL SUPER, space, global, app.talkr:dictate";
        let note = capabilities(&EVERYTHING, Some("Ctrl + Super + Space"), Some(line), false).note.unwrap();
        assert!(note.contains(line) && note.contains("hyprland.conf"), "{}", note);
        // Without the portal the line would bind nothing.
        let note = capabilities(&Availability::default(), None, Some(line), false).note.unwrap();
        assert!(!note.contains(line) && note.contains("talkr --dictate"));
    }

    #[test]
    fn nothing_is_inserted_without_keyboard_access() {
        for setting in [InsertMethod::Auto, InsertMethod::Paste, InsertMethod::Type] {
            assert_eq!(choose(setting, Means { keyboard: false, ..ALL }), None);
        }
    }

    #[test]
    fn auto_pastes_when_the_clipboard_is_kept() {
        assert_eq!(choose(InsertMethod::Auto, ALL), Some(Method::Paste));
        // The user's clipboard could not be saved: type rather than lose it.
        assert_eq!(choose(InsertMethod::Auto, Means { keeps_clipboard: false, ..ALL }), Some(Method::Type));
        assert_eq!(choose(InsertMethod::Auto, Means { clipboard: false, ..ALL }), Some(Method::Type));
    }

    #[test]
    fn auto_avoids_a_stale_primary_paste() {
        // Without the primary selection, a terminal would paste whatever was selected last:
        // plain text is typed instead.
        let no_primary = Means { primary: false, ..ALL };
        assert_eq!(choose(InsertMethod::Auto, no_primary), Some(Method::Type));
        // Text that may not type on this layout is still pasted (and the paste is checked).
        assert_eq!(choose(InsertMethod::Auto, Means { plain: false, ..no_primary }), Some(Method::Paste));
        assert_eq!(choose(InsertMethod::Auto, Means { plain: false, keeps_clipboard: false, ..no_primary }), Some(Method::Type));
        // Asked for explicitly, paste it is.
        assert_eq!(choose(InsertMethod::Paste, no_primary), Some(Method::Paste));
    }

    #[test]
    fn plain_text_is_ascii() {
        assert!(plain("Hello, world!\nNext line\t(tab) ~ 42"));
        assert!(plain(""));
        assert!(!plain("café"));
        assert!(!plain("it’s"));
        assert!(!plain("a — b"));
        assert!(!plain("\u{7}"));
    }

    #[test]
    fn the_setting_wins_where_it_can() {
        assert_eq!(choose(InsertMethod::Type, ALL), Some(Method::Type));
        assert_eq!(choose(InsertMethod::Paste, Means { keeps_clipboard: false, ..ALL }), Some(Method::Paste));
        // No clipboard to paste from: typing still gets the text in.
        assert_eq!(choose(InsertMethod::Paste, Means { clipboard: false, ..ALL }), Some(Method::Type));
        assert_eq!(Method::Paste.name(), "paste");
        assert_eq!(Method::Type.name(), "type");
    }

    #[test]
    fn copy_reasons_read_well() {
        for reason in
            [CopyReason::NoPortal, CopyReason::NotAllowed, CopyReason::Failed, CopyReason::Interrupted, CopyReason::NotPasted]
        {
            let m = reason.message();
            assert!(!m.is_empty() && !m.ends_with('.'));
        }
    }
}
