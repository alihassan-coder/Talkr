//! The macOS side of dictation: a Core Graphics event tap for the shortcut, the Accessibility
//! API to read the focused field, and insertion by pasting (with the pasteboard given back) or by
//! typing. All of it needs the Accessibility permission, which the settings page asks for.

mod ax;
mod ffi;
mod field;
mod insert;
mod keymap;
mod keys;
mod machine;
mod pasteboard;
mod pill;
mod placement;
mod tap;
mod target;
mod workspace;

use crate::dictation::backend::{Backend, Capabilities, Delivery, Permission};
use crate::dictation::settings::{DictationSettings, OverlayPosition, Shortcut};
use crate::dictation::{text, HotkeyEvent};
use insert::{Outcome, Prepared};

pub use target::Target;

pub struct Os;

const SETTINGS_URL: &str = "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";

impl Backend for Os {
    type Target = Target;

    fn capabilities() -> Capabilities {
        Capabilities {
            os: "macos",
            supported: true,
            hold_to_talk: true,
            modifier_only: true,
            records_shortcut: true,
            verifies_insertion: ffi::trusted(),
            inserts_text: true,
            meta_key: "⌘",
            note: ffi::secure_input().then(|| {
                "Secure keyboard entry is on (a password field, or Terminal's Secure Keyboard Entry): macOS hides \
                 the keyboard from Talkr until it is off."
                    .into()
            }),
        }
    }

    fn permission() -> Permission {
        if ffi::trusted() {
            Permission::Granted
        } else {
            Permission::Missing {
                title: "Accessibility".into(),
                detail: "Talkr needs it to hear the shortcut in other apps and to type the text where your cursor is. \
                         Allow Talkr in System Settings › Privacy & Security › Accessibility."
                    .into(),
                can_request: true,
            }
        }
    }

    fn request_permission() {
        if ffi::prompt_for_trust() {
            return;
        }
        // The prompt shows once per app; the settings pane always works.
        if let Err(e) = std::process::Command::new("/usr/bin/open").arg(SETTINGS_URL).spawn() {
            log::warn!("could not open the Accessibility settings: {}", e);
        }
    }

    fn start_hotkeys(events: flume::Sender<HotkeyEvent>) -> Result<(), String> {
        tap::start(events)
    }

    fn stop_hotkeys() {
        tap::stop();
    }

    fn hotkeys_running() -> bool {
        tap::is_running()
    }

    fn configure_hotkeys(dictate: Option<&Shortcut>, paste_last: Option<&Shortcut>) {
        tap::configure(dictate, paste_last);
    }

    fn set_recording(recording: bool) {
        tap::set_recording(recording);
    }

    fn set_capturing(capturing: bool) {
        tap::set_capturing(capturing);
    }

    fn shortcut_held(shortcut: &Shortcut) -> Option<bool> {
        Some(keys::shortcut_held(shortcut))
    }

    fn snapshot() -> Option<Self::Target> {
        workspace::snapshot()
    }

    fn app_label(t: &Self::Target) -> Option<String> {
        t.label()
    }

    fn app_id(t: &Self::Target) -> Option<String> {
        t.app_id()
    }

    fn deliver(
        clean: &str,
        target: Option<&Self::Target>,
        settings: &DictationSettings,
        _owner: Option<&tauri::WebviewWindow>,
    ) -> Delivery {
        let copy = |reason: target::CopyReason| Delivery::copied(reason.message(), pasteboard::put_text(clean, false).is_some());
        let Some(t) = target else { return copy(target::CopyReason::NoTextField) };
        let ready = match insert::prepare(t, settings) {
            Prepared::Copy(reason) => return copy(reason),
            Prepared::Ready(ready) => ready,
        };
        let password = ready.field.as_ref().is_some_and(|f| f.password);
        let before = ready.field.as_ref().and_then(|f| f.before_caret.as_deref());
        let fitted = match before {
            Some(before) if settings.smart_spacing => text::fit_to_context(clean, before),
            _ => clean.to_string(),
        };
        match insert::deliver(&ready, &fitted, settings) {
            Outcome::Inserted { method, verdict, learned_typing } => Delivery {
                inserted: true,
                verified: verdict == field::Verdict::Arrived,
                method: Some(method.name()),
                password,
                learned_typing_for: if learned_typing { ready.target.app_id() } else { None },
                ..Delivery::default()
            },
            Outcome::Copied(reason) => Delivery { password, ..copy(reason) },
        }
    }

    fn copy(text: &str, _owner: Option<&tauri::WebviewWindow>) -> bool {
        pasteboard::put_text(text, false).is_some()
    }

    fn show_overlay(window: &tauri::WebviewWindow, anchor: Option<&Self::Target>, logical: (f64, f64), position: OverlayPosition) {
        // Read here, off the main thread: the target app answers Accessibility questions itself.
        let anchor = anchor.and_then(|t| ax::focused_window_center(t.pid));
        let handle = window.clone();
        if let Err(e) = window.run_on_main_thread(move || pill::show(&handle, anchor, logical, position)) {
            log::warn!("could not show the dictation pill: {}", e);
        }
    }

    fn hide_overlay(window: &tauri::WebviewWindow) {
        let handle = window.clone();
        let _ = window.run_on_main_thread(move || pill::hide(&handle));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_and_permission_agree() {
        let c = Os::capabilities();
        assert_eq!((c.os, c.meta_key), ("macos", "⌘"));
        assert!(c.supported && c.hold_to_talk && c.modifier_only && c.records_shortcut && c.inserts_text);
        match Os::permission() {
            Permission::Granted => assert!(ffi::trusted()),
            Permission::Missing { title, can_request, .. } => {
                assert_eq!(title, "Accessibility");
                assert!(can_request && !ffi::trusted());
            }
            Permission::NotNeeded => panic!("macOS always needs Accessibility"),
        }
    }

    /// The copy fallback, end to end through the real pasteboard (CI only: it replaces what is
    /// on the clipboard).
    #[test]
    fn without_a_target_the_text_is_copied() {
        if std::env::var_os("CI").is_none() {
            return;
        }
        let _turn = pasteboard::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let d = Os::deliver("hello from the test", None, &DictationSettings::default(), None);
        assert!(!d.inserted);
        assert_eq!(d.copied.as_deref(), Some("No text field was selected"));
        assert!(!d.clipboard_failed);
    }
}
