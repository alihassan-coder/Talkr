//! The contract between dictation's shared engine (the state machine, transcription, text
//! clean-up, the pill) and each operating system. Every OS module provides one `Os` type that
//! implements [`Backend`]; the compiler holds Windows, macOS and Linux to the same interface.
//!
//! What each OS must do:
//! - **Shortcut**: report the dictation shortcut's press and release anywhere in the system, and
//!   Escape while recording; record a new shortcut for the settings page.
//! - **Target**: remember which app (and field) had focus when the shortcut was pressed.
//! - **Insert**: put text into that field, keep the user's clipboard, and say honestly whether it
//!   arrived or was only copied.
//! - **Pill and sounds**: show the overlay without taking focus; play the cues.
//! - **Permissions**: say what the system still has to allow (Accessibility on macOS, the input
//!   portal on Wayland) and open the place to allow it.

use serde::Serialize;
use super::settings::{DictationSettings, OverlayPosition, Shortcut};
use super::HotkeyEvent;

/// What dictation can do on this system, for the settings page.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    /// "windows", "macos" or "linux".
    pub os: &'static str,
    /// Dictation works here at all.
    pub supported: bool,
    /// The shortcut's release is reported, so hold-to-talk works (otherwise: press to start,
    /// press again to stop).
    pub hold_to_talk: bool,
    /// Shortcuts of modifiers alone (Ctrl + Win) can be used.
    pub modifier_only: bool,
    /// Talkr records the shortcut itself. When false the system chooses it (Wayland's
    /// global-shortcuts portal) and the page explains where to change it.
    pub records_shortcut: bool,
    /// Insertion is checked by reading the field back.
    pub verifies_insertion: bool,
    /// Text can be typed into other apps. When false it is copied for the user to paste.
    pub inserts_text: bool,
    /// What the Win key is called here: "Win", "⌘" or "Super".
    pub meta_key: &'static str,
    /// A short note about how dictation works on this system, if anything is unusual
    /// ("Wayland session: …").
    pub note: Option<String>,
}

/// Something the system must allow before dictation works fully.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "camelCase")]
// Windows needs no permission; macOS and Linux build the other variants.
#[cfg_attr(windows, allow(dead_code))]
pub enum Permission {
    /// Nothing to allow.
    NotNeeded,
    Granted,
    /// Missing: `title` names it ("Accessibility"), `detail` says why and how.
    /// `can_request`: Talkr can open the system prompt or settings page for it.
    #[serde(rename_all = "camelCase")]
    Missing { title: String, detail: String, can_request: bool },
}

/// The sounds dictation plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cue {
    Start,
    Stop,
    Problem,
}

/// What happened to a dictation's text.
#[derive(Debug, Default, Clone)]
pub struct Delivery {
    pub inserted: bool,
    /// Read back from the field after inserting.
    pub verified: bool,
    /// "direct", "paste" or "type".
    pub method: Option<&'static str>,
    /// The field was a password box: keep the text out of history.
    pub password: bool,
    /// Why the text went to the clipboard instead of the field.
    pub copied: Option<String>,
    /// It could not even be copied (another app held the clipboard).
    pub clipboard_failed: bool,
    /// Pasting did not work in this app but typing did: remember it (an executable or app id).
    pub learned_typing_for: Option<String>,
}

impl Delivery {
    /// The text was not inserted; `reason` says why, and whether it reached the clipboard.
    pub fn copied(reason: impl Into<String>, clipboard_ok: bool) -> Self {
        Self { copied: Some(reason.into()), clipboard_failed: !clipboard_ok, ..Self::default() }
    }

    /// The pill's title when the text was not inserted.
    pub fn not_inserted_title(&self) -> String {
        if self.clipboard_failed {
            "Not inserted — press your paste-again shortcut".into()
        } else {
            format!("Copied — press {} to paste", super::PASTE_KEYS)
        }
    }

    pub fn summary(&self) -> String {
        match (&self.copied, self.method) {
            (Some(reason), _) => format!("copied ({})", reason),
            (None, Some(method)) => format!("inserted by {}{}", method, if self.verified { ", verified" } else { "" }),
            (None, None) => "inserted".into(),
        }
    }
}

/// One operating system's side of dictation. All methods are called from Talkr's own threads
/// (never the UI thread), may block briefly, and must never panic.
pub trait Backend {
    /// The app (and field) that had focus when the shortcut was pressed.
    type Target: Clone + Send + std::fmt::Debug + 'static;

    fn capabilities() -> Capabilities;

    /// What the system still has to allow, if anything.
    fn permission() -> Permission {
        Permission::NotNeeded
    }

    /// Ask for the missing permission: show the system prompt or open its settings page.
    fn request_permission() {}

    // ---- the shortcut ----

    /// Start listening for shortcuts, sending what happens to `events`. Idempotent; an error is
    /// shown on the settings page.
    fn start_hotkeys(events: flume::Sender<HotkeyEvent>) -> Result<(), String>;

    /// Stop listening, and wait until it has stopped.
    fn stop_hotkeys();

    fn hotkeys_running() -> bool;

    /// The shortcuts to listen for; `None` turns one off.
    fn configure_hotkeys(dictate: Option<&Shortcut>, paste_last: Option<&Shortcut>);

    /// A dictation is recording: Escape cancels it (and should not reach the app).
    fn set_recording(recording: bool);

    /// The settings page is recording a new shortcut: report the next combination as
    /// `HotkeyEvent::Captured` (or `CaptureCancelled` on Escape) instead of acting on keys.
    fn set_capturing(capturing: bool);

    /// Whether the shortcut's keys are physically held right now, as a backstop for a release
    /// that was never reported. `None` when the system cannot tell.
    fn shortcut_held(_shortcut: &Shortcut) -> Option<bool> {
        None
    }

    /// The shortcut was just pressed (called on every press, before anything else).
    fn on_shortcut_pressed(_shortcut: &Shortcut) {}

    // ---- the target ----

    /// The app and field with keyboard focus right now.
    fn snapshot() -> Option<Self::Target>;

    /// A friendly app name for the pill ("Slack"), if known.
    fn app_label(target: &Self::Target) -> Option<String>;

    /// An identifier for logs and per-app rules (an executable name on Windows, a bundle id on
    /// macOS, a WM_CLASS or app id on Linux), lowercase.
    fn app_id(target: &Self::Target) -> Option<String>;

    // ---- inserting ----

    /// Put `text` into `target`'s field (or wherever `settings.focus_policy` says), fitting its
    /// spacing to the text before the cursor when `settings.smart_spacing` is on (see
    /// `text::fit_to_context`). `owner` is the overlay window, for platforms that need a window
    /// to own the clipboard. When the text cannot be inserted, it must be copied to the
    /// clipboard and the returned `Delivery` says why.
    fn deliver(
        text: &str,
        target: Option<&Self::Target>,
        settings: &DictationSettings,
        owner: Option<&tauri::WebviewWindow>,
    ) -> Delivery;

    /// Put `text` on the clipboard (plain, not hidden from clipboard history). Returns whether it
    /// worked.
    fn copy(text: &str, owner: Option<&tauri::WebviewWindow>) -> bool;

    // ---- the pill and sounds ----

    /// Show the overlay window over everything, without taking focus, centred on the screen of
    /// `anchor` (the target) or the one with the mouse.
    fn show_overlay(window: &tauri::WebviewWindow, _anchor: Option<&Self::Target>, logical: (f64, f64), position: OverlayPosition) {
        super::overlay::show_portable(window, logical, position);
    }

    fn hide_overlay(window: &tauri::WebviewWindow) {
        let _ = window.hide();
    }

    /// Play a cue without blocking.
    fn play(cue: Cue) {
        super::cues::play(cue);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delivery_messages() {
        let d = Delivery::copied("No text field was selected", true);
        assert_eq!(d.not_inserted_title(), format!("Copied — press {} to paste", crate::dictation::PASTE_KEYS));
        assert_eq!(d.summary(), "copied (No text field was selected)");
        assert!(Delivery::copied("x", false).not_inserted_title().contains("paste-again"));
        let ok = Delivery { inserted: true, verified: true, method: Some("paste"), ..Delivery::default() };
        assert_eq!(ok.summary(), "inserted by paste, verified");
    }

    #[test]
    fn permission_serializes_for_the_page() {
        let json = serde_json::to_string(&Permission::Missing {
            title: "Accessibility".into(),
            detail: "d".into(),
            can_request: true,
        })
        .unwrap();
        assert_eq!(json, r#"{"state":"missing","title":"Accessibility","detail":"d","canRequest":true}"#);
        assert_eq!(serde_json::to_string(&Permission::Granted).unwrap(), r#"{"state":"granted"}"#);
    }
}
