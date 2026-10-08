//! Dictation on a Wayland session. Apps may not read the keyboard or type into other apps here,
//! so Talkr asks the desktop through its XDG portals, each allowed by the user once:
//!
//! - **The shortcut** (`shortcuts`): the GlobalShortcuts portal binds it and reports its press
//!   and release (GNOME 48+, KDE Plasma 6, Hyprland and others). The desktop chooses the keys;
//!   Talkr suggests the configured ones. Without the portal, `talkr --dictate` bound to a key in
//!   the system settings starts and stops dictation.
//! - **Inserting** (`insert`): the RemoteDesktop portal's keyboard types or presses
//!   Shift + Insert; the clipboard is set through data-control or the Clipboard portal
//!   (`clipboard`, `remote`). Without the portal the text is copied.
//!
//! Wayland does not say which window has focus, so text goes wherever the cursor is when it is
//! ready (the focus policy and per-app rules cannot apply), and Escape cannot cancel (the pill's
//! cancel button can).

mod clipboard;
mod insert;
mod keysym;
mod plan;
mod portal;
mod remote;
mod shortcuts;
mod trigger;

use std::time::Duration;
use crate::dictation::backend::{Backend, Capabilities, Delivery, Permission};
use crate::dictation::settings::{DictationSettings, Shortcut};
use crate::dictation::HotkeyEvent;

/// How long the settings page's "Allow" waits for the user to answer the desktop's dialog.
const APPROVAL_WAIT: Duration = Duration::from_secs(120);

pub struct Os;

/// What had focus when the shortcut was pressed: unknown on Wayland, so a placeholder (the pill
/// names no app, and focus checks are skipped).
#[derive(Debug, Clone)]
pub struct Target;

/// The shortcut as the settings page should describe it.
fn shortcut_note() -> Option<String> {
    let bound = shortcuts::report()
        .bound
        .into_iter()
        .find(|(id, description)| id == shortcuts::DICTATE && !description.trim().is_empty())
        .map(|(_, description)| description);
    if bound.is_some() {
        return bound;
    }
    let requested = shortcuts::requested()?;
    Some(if requested.stand_in {
        format!("{} (desktop shortcuts need a regular key, so Talkr asked for Space)", requested.label)
    } else {
        requested.label
    })
}

/// Hyprland binds an app's shortcuts only from its config: the line to add there.
fn hyprland_line() -> Option<String> {
    let hyprland = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some()
        || std::env::var("XDG_CURRENT_DESKTOP")
            .is_ok_and(|d| d.split(':').any(|name| name.eq_ignore_ascii_case("hyprland")));
    if !hyprland {
        return None;
    }
    let requested = shortcuts::requested()?;
    Some(trigger::hyprland_bind(&requested, portal::APP_ID, shortcuts::DICTATE))
}

impl Backend for Os {
    type Target = Target;

    fn capabilities() -> Capabilities {
        let av = portal::availability();
        let mut caps = plan::capabilities(&av, shortcut_note().as_deref().filter(|_| av.shortcuts), hyprland_line().as_deref());
        // The session binds after `start_hotkeys` returned, so its failure is only known here.
        if let Some(error) = shortcuts::report().error.filter(|_| av.shortcuts && !shortcuts::running()) {
            if let Some(note) = caps.note.as_mut() {
                note.push_str(&format!(" The shortcut is not working: {}.", error));
            }
        }
        caps
    }

    fn permission() -> Permission {
        if !portal::availability().keyboard {
            // Nothing to allow: this desktop cannot let apps type (the note says so).
            return Permission::NotNeeded;
        }
        // Brings back access allowed before, without a dialog.
        if remote::ensure(false, Duration::ZERO).is_some() {
            return Permission::Granted;
        }
        let mut detail = "Talkr types your words through the desktop's remote-desktop portal. Allow it once (and let \
                          the desktop remember it); until then dictations are copied for you to paste."
            .to_string();
        if let Some(error) = remote::last_error() {
            detail.push_str(&format!(" Last attempt: {}.", error));
        }
        Permission::Missing { title: "Keyboard access".into(), detail, can_request: true }
    }

    fn request_permission() {
        if portal::availability().keyboard {
            remote::ensure(true, APPROVAL_WAIT);
        }
    }

    fn start_hotkeys(events: flume::Sender<HotkeyEvent>) -> Result<(), String> {
        let av = portal::settled_availability();
        if av.keyboard {
            // Bring back keyboard access allowed in an earlier run, before the first dictation.
            remote::ensure(false, Duration::ZERO);
        }
        if !av.shortcuts {
            return Err("This Wayland desktop has no global-shortcuts portal, so Talkr cannot listen for a shortcut. \
                        Bind the command `talkr --dictate` to a key in your keyboard settings instead: it starts and \
                        stops dictation."
                .into());
        }
        shortcuts::start(events);
        match shortcuts::report().error {
            Some(error) if !shortcuts::running() => Err(format!("The desktop's shortcut portal failed: {}", error)),
            _ => Ok(()),
        }
    }

    fn stop_hotkeys() {
        shortcuts::stop();
        remote::close();
    }

    fn hotkeys_running() -> bool {
        shortcuts::running()
    }

    fn configure_hotkeys(dictate: Option<&Shortcut>, paste_last: Option<&Shortcut>) {
        shortcuts::configure(shortcuts::bindings(dictate, paste_last));
    }

    // Escape cannot be seen on Wayland: the pill's cancel button stops a dictation.
    fn set_recording(_recording: bool) {}

    fn set_capturing(capturing: bool) {
        shortcuts::capture(capturing);
    }

    fn snapshot() -> Option<Self::Target> {
        Some(Target)
    }

    fn app_label(_target: &Self::Target) -> Option<String> {
        None
    }

    fn app_id(_target: &Self::Target) -> Option<String> {
        None
    }

    fn deliver(
        text: &str,
        _target: Option<&Self::Target>,
        settings: &DictationSettings,
        _owner: Option<&tauri::WebviewWindow>,
    ) -> Delivery {
        insert::deliver(text, settings)
    }

    fn copy(text: &str, _owner: Option<&tauri::WebviewWindow>) -> bool {
        insert::copy(text)
    }
}
