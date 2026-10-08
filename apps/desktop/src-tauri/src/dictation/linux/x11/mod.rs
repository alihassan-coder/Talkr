//! Dictation on an X11 session, in pure Rust over the X protocol (x11rb).
//!
//! - **Shortcut** (`listener`, `chord`): passive key grabs for exactly the configured shortcuts,
//!   the standard way global hotkeys work on X11. Talkr never sees other typing: the server hands
//!   it keys only while one of its own shortcuts is held (and gives back any other key pressed
//!   then), Escape only while a dictation records, and the whole keyboard only while the user
//!   records a new shortcut in the settings.
//! - **Target** (`target`): the active window from the window manager (EWMH), named by WM_CLASS.
//! - **Insert** (`insert`, `clipboard`, `input`): the text goes on the clipboard and the app's
//!   paste keystroke is sent with XTEST, then the user's clipboard is put back; apps that do not
//!   take pastes get it typed. X11 cannot read a field back, so insertion is not verified.

mod chord;
mod clipboard;
mod input;
mod insert;
mod keymap;
mod keys;
mod listener;
mod target;
mod xconn;
#[cfg(test)]
mod xvfb;

use std::sync::Mutex;
use std::time::{Duration, Instant};
use x11rb::protocol::xproto::ConnectionExt as _;
use crate::dictation::backend::{Backend, Capabilities, Delivery};
use crate::dictation::settings::{DictationSettings, OverlayPosition, Shortcut};
use crate::dictation::HotkeyEvent;
use keymap::Keymap;
use xconn::{Display, Fail};

pub use target::Target;

pub struct Os;

/// The keyboard map for the backstop check below, read again every few seconds (the layout can
/// change while Talkr runs).
fn keymap(d: &Display) -> Result<Keymap, String> {
    static CACHE: Mutex<Option<(Instant, Keymap)>> = Mutex::new(None);
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, map)) = cache.as_ref() {
        if at.elapsed() < Duration::from_secs(3) {
            return Ok(map.clone());
        }
    }
    let map = Keymap::load(&d.conn)?;
    *cache = Some((Instant::now(), map.clone()));
    Ok(map)
}

impl Backend for Os {
    type Target = Target;

    fn capabilities() -> Capabilities {
        Capabilities {
            os: "linux",
            supported: true,
            hold_to_talk: true,
            modifier_only: true,
            records_shortcut: true,
            verifies_insertion: false,
            inserts_text: true,
            meta_key: "Super",
            note: None,
        }
    }

    fn start_hotkeys(events: flume::Sender<HotkeyEvent>) -> Result<(), String> {
        listener::start(events)
    }

    fn stop_hotkeys() {
        listener::stop();
    }

    fn hotkeys_running() -> bool {
        listener::is_running()
    }

    fn configure_hotkeys(dictate: Option<&Shortcut>, paste_last: Option<&Shortcut>) {
        listener::configure(dictate, paste_last);
    }

    fn hotkeys_problem() -> Option<String> {
        listener::problem()
    }

    fn set_recording(recording: bool) {
        listener::set_recording(recording);
    }

    fn set_capturing(capturing: bool) {
        listener::set_capturing(capturing);
    }

    /// From the modifier state, and for a key shortcut that key's bit in the keyboard state:
    /// only the shortcut's own keys are looked at.
    fn shortcut_held(shortcut: &Shortcut) -> Option<bool> {
        xconn::with(|d| {
            let map = keymap(d)?;
            let wanted = chord::Chord::of(shortcut);
            if input::held_modifiers(d, &map)? & wanted.mods != wanted.mods {
                return Ok(false);
            }
            let Some(vk) = wanted.key else { return Ok(true) };
            let state = d.conn.query_keymap().x()?.reply().x()?.keys;
            Ok(map.keycodes_for_vk(vk).iter().any(|&kc| state[kc as usize / 8] & (1 << (kc % 8)) != 0))
        })
    }

    fn snapshot() -> Option<Self::Target> {
        xconn::with(target::snapshot).flatten()
    }

    fn app_label(t: &Self::Target) -> Option<String> {
        target::app_label(if t.class.is_empty() { &t.instance } else { &t.class })
    }

    fn app_id(t: &Self::Target) -> Option<String> {
        t.app_id()
    }

    fn deliver(
        text: &str,
        target: Option<&Self::Target>,
        settings: &DictationSettings,
        _owner: Option<&tauri::WebviewWindow>,
    ) -> Delivery {
        insert::deliver(text, target, settings)
    }

    fn copy(text: &str, _owner: Option<&tauri::WebviewWindow>) -> bool {
        clipboard::put(clipboard::Selection::Clipboard, text)
    }

    fn show_overlay(window: &tauri::WebviewWindow, anchor: Option<&Self::Target>, logical: (f64, f64), position: OverlayPosition) {
        crate::dictation::overlay::show_portable(window, logical, position);
        // The pill asks not to be focused (it is built with focus off); should a window manager
        // focus it anyway, hand focus back to the app being dictated into.
        let Some(anchor) = anchor.filter(|a| !a.is_ours()).cloned() else { return };
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            xconn::with(|d| {
                if target::snapshot(d)?.is_some_and(|now| now.is_ours()) {
                    log::info!("the window manager focused the dictation pill; giving focus back");
                    target::activate(d, anchor.window)?;
                }
                Ok(())
            });
        });
    }
}
