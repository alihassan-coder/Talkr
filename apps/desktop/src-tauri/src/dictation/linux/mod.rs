//! Dictation on Linux. X11 and Wayland work differently enough to need two backends:
//!
//! - **X11** (`x11`): Talkr reads the keyboard itself (XInput2), types with XTest, and asks the
//!   window manager which window is active (EWMH).
//! - **Wayland** (`wayland`): apps may not read the keyboard or type into other apps. Talkr asks
//!   the desktop through its portals instead: the GlobalShortcuts portal for the shortcut (press
//!   and release) and the RemoteDesktop portal to type, each allowed once by the user.
//!
//! The session decides which one runs. `TALKR_DICTATION_BACKEND=x11|wayland` overrides it (an
//! X11 app on XWayland only sees keys typed into other X11 apps, so Wayland is the default
//! whenever the session is Wayland).

mod wayland;
mod x11;

use crate::dictation::backend::{Backend, Capabilities, Cue, Delivery, Permission};
use crate::dictation::settings::{DictationSettings, OverlayPosition, Shortcut};
use crate::dictation::HotkeyEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Session {
    X11,
    Wayland,
}

/// Which backend runs, from the environment (see the module docs).
pub fn session() -> Session {
    detect(
        std::env::var("TALKR_DICTATION_BACKEND").ok().as_deref(),
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
        std::env::var_os("WAYLAND_DISPLAY").is_some(),
    )
}

fn detect(forced: Option<&str>, session_type: Option<&str>, wayland_display: bool) -> Session {
    match forced.map(str::to_ascii_lowercase).as_deref() {
        Some("x11") => return Session::X11,
        Some("wayland") => return Session::Wayland,
        _ => {}
    }
    if session_type.is_some_and(|t| t.eq_ignore_ascii_case("wayland")) || (session_type.is_none() && wayland_display) {
        Session::Wayland
    } else {
        Session::X11
    }
}

/// What had focus when the shortcut was pressed, on either kind of session.
#[derive(Debug, Clone)]
pub enum Target {
    X11(x11::Target),
    Wayland(wayland::Target),
}

pub struct Os;

impl Backend for Os {
    type Target = Target;

    fn capabilities() -> Capabilities {
        match session() {
            Session::X11 => x11::Os::capabilities(),
            Session::Wayland => wayland::Os::capabilities(),
        }
    }

    fn permission() -> Permission {
        match session() {
            Session::X11 => x11::Os::permission(),
            Session::Wayland => wayland::Os::permission(),
        }
    }

    fn request_permission() {
        match session() {
            Session::X11 => x11::Os::request_permission(),
            Session::Wayland => wayland::Os::request_permission(),
        }
    }

    fn start_hotkeys(events: flume::Sender<HotkeyEvent>) -> Result<(), String> {
        match session() {
            Session::X11 => x11::Os::start_hotkeys(events),
            Session::Wayland => wayland::Os::start_hotkeys(events),
        }
    }

    fn stop_hotkeys() {
        match session() {
            Session::X11 => x11::Os::stop_hotkeys(),
            Session::Wayland => wayland::Os::stop_hotkeys(),
        }
    }

    fn hotkeys_running() -> bool {
        match session() {
            Session::X11 => x11::Os::hotkeys_running(),
            Session::Wayland => wayland::Os::hotkeys_running(),
        }
    }

    fn configure_hotkeys(dictate: Option<&Shortcut>, paste_last: Option<&Shortcut>) {
        match session() {
            Session::X11 => x11::Os::configure_hotkeys(dictate, paste_last),
            Session::Wayland => wayland::Os::configure_hotkeys(dictate, paste_last),
        }
    }

    fn set_recording(recording: bool) {
        match session() {
            Session::X11 => x11::Os::set_recording(recording),
            Session::Wayland => wayland::Os::set_recording(recording),
        }
    }

    fn set_capturing(capturing: bool) {
        match session() {
            Session::X11 => x11::Os::set_capturing(capturing),
            Session::Wayland => wayland::Os::set_capturing(capturing),
        }
    }

    fn shortcut_held(shortcut: &Shortcut) -> Option<bool> {
        match session() {
            Session::X11 => x11::Os::shortcut_held(shortcut),
            Session::Wayland => wayland::Os::shortcut_held(shortcut),
        }
    }

    fn on_shortcut_pressed(shortcut: &Shortcut) {
        match session() {
            Session::X11 => x11::Os::on_shortcut_pressed(shortcut),
            Session::Wayland => wayland::Os::on_shortcut_pressed(shortcut),
        }
    }

    fn snapshot() -> Option<Self::Target> {
        match session() {
            Session::X11 => x11::Os::snapshot().map(Target::X11),
            Session::Wayland => wayland::Os::snapshot().map(Target::Wayland),
        }
    }

    fn app_label(target: &Self::Target) -> Option<String> {
        match target {
            Target::X11(t) => x11::Os::app_label(t),
            Target::Wayland(t) => wayland::Os::app_label(t),
        }
    }

    fn app_id(target: &Self::Target) -> Option<String> {
        match target {
            Target::X11(t) => x11::Os::app_id(t),
            Target::Wayland(t) => wayland::Os::app_id(t),
        }
    }

    fn deliver(
        text: &str,
        target: Option<&Self::Target>,
        settings: &DictationSettings,
        owner: Option<&tauri::WebviewWindow>,
    ) -> Delivery {
        match (session(), target) {
            (Session::X11, Some(Target::X11(t))) => x11::Os::deliver(text, Some(t), settings, owner),
            (Session::X11, _) => x11::Os::deliver(text, None, settings, owner),
            (Session::Wayland, Some(Target::Wayland(t))) => wayland::Os::deliver(text, Some(t), settings, owner),
            (Session::Wayland, _) => wayland::Os::deliver(text, None, settings, owner),
        }
    }

    fn copy(text: &str, owner: Option<&tauri::WebviewWindow>) -> bool {
        match session() {
            Session::X11 => x11::Os::copy(text, owner),
            Session::Wayland => wayland::Os::copy(text, owner),
        }
    }

    fn show_overlay(window: &tauri::WebviewWindow, anchor: Option<&Self::Target>, logical: (f64, f64), position: OverlayPosition) {
        match anchor {
            Some(Target::X11(t)) => x11::Os::show_overlay(window, Some(t), logical, position),
            Some(Target::Wayland(t)) => wayland::Os::show_overlay(window, Some(t), logical, position),
            None => match session() {
                Session::X11 => x11::Os::show_overlay(window, None, logical, position),
                Session::Wayland => wayland::Os::show_overlay(window, None, logical, position),
            },
        }
    }

    fn hide_overlay(window: &tauri::WebviewWindow) {
        match session() {
            Session::X11 => x11::Os::hide_overlay(window),
            Session::Wayland => wayland::Os::hide_overlay(window),
        }
    }

    fn play(cue: Cue) {
        match session() {
            Session::X11 => x11::Os::play(cue),
            Session::Wayland => wayland::Os::play(cue),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_session_picks_the_backend() {
        assert_eq!(detect(None, Some("wayland"), true), Session::Wayland);
        assert_eq!(detect(None, Some("x11"), false), Session::X11);
        // XWayland sets DISPLAY too; the session type wins.
        assert_eq!(detect(None, Some("x11"), true), Session::X11);
        assert_eq!(detect(None, None, true), Session::Wayland);
        assert_eq!(detect(None, None, false), Session::X11);
        assert_eq!(detect(Some("X11"), Some("wayland"), true), Session::X11);
        assert_eq!(detect(Some("wayland"), Some("x11"), false), Session::Wayland);
        assert_eq!(detect(Some("bogus"), Some("x11"), false), Session::X11);
    }
}
