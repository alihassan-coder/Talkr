//! The Windows side of dictation: a low-level keyboard hook for the shortcut, UI Automation to
//! read the focused field, and insertion straight into edit controls, by pasting or by typing.

pub mod clipboard;
pub mod hook;
pub mod insert;
pub mod keys;
pub mod sound;
pub mod target;
pub mod uia;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    SetWindowPos, ShowWindow, HWND_TOPMOST, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE, SW_SHOWNOACTIVATE,
};
use crate::dictation::backend::{Backend, Capabilities, Cue, Delivery};
use crate::dictation::settings::{DictationSettings, OverlayPosition, Shortcut};
use crate::dictation::{text, HotkeyEvent};

pub struct Os;

fn hwnd_of(window: &tauri::WebviewWindow) -> Option<isize> {
    window.hwnd().ok().map(|h| h.0 as isize)
}

impl Backend for Os {
    type Target = target::Target;

    fn capabilities() -> Capabilities {
        Capabilities {
            os: "windows",
            supported: true,
            hold_to_talk: true,
            modifier_only: true,
            records_shortcut: true,
            verifies_insertion: true,
            inserts_text: true,
            meta_key: "Win",
            note: None,
        }
    }

    fn start_hotkeys(events: flume::Sender<HotkeyEvent>) -> Result<(), String> {
        hook::start(events)
    }

    fn stop_hotkeys() {
        hook::stop();
    }

    fn hotkeys_running() -> bool {
        hook::is_running()
    }

    fn configure_hotkeys(dictate: Option<&Shortcut>, paste_last: Option<&Shortcut>) {
        hook::configure(dictate, paste_last);
    }

    fn set_recording(recording: bool) {
        hook::set_recording(recording);
    }

    fn set_capturing(capturing: bool) {
        hook::set_capturing(capturing);
    }

    fn shortcut_held(shortcut: &Shortcut) -> Option<bool> {
        Some(keys::shortcut_held(shortcut))
    }

    fn on_shortcut_pressed(shortcut: &Shortcut) {
        // Mark Win/Alt as used right away, on every press (stopping hands-free too): when they
        // are let go, Windows must not open Start or a menu bar.
        if shortcut.win || shortcut.alt {
            keys::neutralize_modifier_release();
        }
    }

    fn snapshot() -> Option<Self::Target> {
        target::snapshot()
    }

    fn app_label(t: &Self::Target) -> Option<String> {
        target::app_label(&t.exe)
    }

    fn app_id(t: &Self::Target) -> Option<String> {
        (!t.exe.is_empty()).then(|| t.exe.clone())
    }

    fn deliver(
        clean: &str,
        target: Option<&Self::Target>,
        settings: &DictationSettings,
        owner: Option<&tauri::WebviewWindow>,
    ) -> Delivery {
        use insert::{CopyReason, Outcome, Prepared};
        let owner = owner.and_then(hwnd_of);
        let copy = |reason: CopyReason| {
            let copied = clipboard::put_text(clean, owner.map(target::hwnd), false).is_some();
            Delivery::copied(reason.message(), copied)
        };
        let Some(t) = target else { return copy(CopyReason::NoTextField) };
        match insert::prepare(t, settings, owner.unwrap_or(0)) {
            Prepared::Copy(reason) => copy(reason),
            Prepared::Ready(ready) => {
                let password = ready.field.as_ref().is_some_and(|f| f.password);
                let before = ready.field.as_ref().and_then(|f| f.before_caret.as_deref());
                let fitted = match before {
                    Some(before) if settings.smart_spacing => text::fit_to_context(clean, before),
                    _ => clean.to_string(),
                };
                match insert::deliver(&ready, &fitted, settings, owner) {
                    Outcome::Inserted { method, verdict, learned_typing } => Delivery {
                        inserted: true,
                        verified: verdict == uia::Verdict::Arrived,
                        method: Some(method.name()),
                        password,
                        learned_typing_for: learned_typing.then(|| ready.target.exe.clone()).filter(|e| !e.is_empty()),
                        ..Delivery::default()
                    },
                    Outcome::Copied(reason) => Delivery { password, ..copy(reason) },
                }
            }
        }
    }

    fn copy(text: &str, owner: Option<&tauri::WebviewWindow>) -> bool {
        clipboard::put_text(text, owner.and_then(hwnd_of).map(target::hwnd), false).is_some()
    }

    fn show_overlay(window: &tauri::WebviewWindow, anchor: Option<&Self::Target>, logical: (f64, f64), position: OverlayPosition) {
        if let Some(overlay) = hwnd_of(window) {
            show_overlay(overlay, anchor.map(|t| t.hwnd).unwrap_or(0), logical, position);
        }
    }

    fn hide_overlay(window: &tauri::WebviewWindow) {
        if let Some(overlay) = hwnd_of(window) {
            hide_overlay(overlay);
        }
    }

    fn play(cue: Cue) {
        sound::play(cue);
    }
}

/// Show the pill window over everything without taking focus from the app the user is typing
/// in, centred on the monitor of `anchor` (the target window), above the taskbar or at the top.
pub fn show_overlay(overlay: isize, anchor: isize, logical: (f64, f64), position: OverlayPosition) {
    let window = target::hwnd(overlay);
    let Some((work, scale)) = target::work_area(if anchor != 0 { anchor } else { overlay }) else {
        // SAFETY: plain window call.
        let _ = unsafe { ShowWindow(window, SW_SHOWNOACTIVATE) };
        return;
    };
    let w = (logical.0 * scale).round() as i32;
    let h = (logical.1 * scale).round() as i32;
    let x = work.left + ((work.right - work.left) - w) / 2;
    let margin = (14.0 * scale).round() as i32;
    let y = match position {
        OverlayPosition::Bottom => work.bottom - h - margin,
        OverlayPosition::Top => work.top + margin,
    };
    // SAFETY: plain window calls. Twice: moving to a monitor with another scale makes Windows
    // resize the window to suit it, and the second call puts the intended size back.
    unsafe {
        for _ in 0..2 {
            let _ = SetWindowPos(window, Some(HWND_TOPMOST), x, y, w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
        }
        let _ = ShowWindow(window, SW_SHOWNOACTIVATE);
    }
}

pub fn hide_overlay(overlay: isize) {
    // SAFETY: plain window call.
    let _ = unsafe { ShowWindow(HWND(overlay as *mut core::ffi::c_void), SW_HIDE) };
}
