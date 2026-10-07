//! The Windows side of dictation: the shortcut hook, the focused field, inserting text.

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
use crate::dictation::settings::OverlayPosition;

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
