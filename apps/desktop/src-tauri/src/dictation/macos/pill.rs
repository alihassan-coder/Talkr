//! The pill's window on macOS: shown without activating Talkr (the app being dictated into keeps
//! focus), on every Space and above full-screen apps, on the display the user is working on.
//! AppKit windows belong to the main thread, so both calls run there.

use objc2::rc::Retained;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSEvent, NSScreen, NSStatusWindowLevel, NSWindow, NSWindowCollectionBehavior};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use super::placement::{self, Frame};
use crate::dictation::settings::OverlayPosition;

fn ns_window(window: &tauri::WebviewWindow) -> Option<Retained<NSWindow>> {
    let ptr = window.ns_window().ok()?;
    // SAFETY: Tauri returns the window's live NSWindow, used here on the main thread and retained
    // for as long as it is held.
    unsafe { Retained::retain(ptr.cast::<NSWindow>()) }
}

fn frame(rect: NSRect) -> Frame {
    (rect.origin.x, rect.origin.y, rect.size.width, rect.size.height)
}

/// Show the pill, `logical` points large, centred on the display that holds `anchor` (the middle
/// of the target's window, in Accessibility coordinates) or else the mouse. Main thread only.
pub fn show(window: &tauri::WebviewWindow, anchor: Option<(f64, f64)>, logical: (f64, f64), position: OverlayPosition) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let Some(ns) = ns_window(window) else { return };
    let screens = NSScreen::screens(mtm).to_vec();
    if let Some(main) = screens.first() {
        let point = match anchor {
            Some(p) => placement::from_top_left(p, main.frame().size.height),
            None => {
                let mouse = NSEvent::mouseLocation();
                (mouse.x, mouse.y)
            }
        };
        let screen = screens.iter().find(|s| placement::contains(frame(s.frame()), point)).unwrap_or(main);
        let (x, y) = placement::pill_origin(frame(screen.visibleFrame()), logical, position);
        ns.setFrame_display(NSRect::new(NSPoint::new(x, y), NSSize::new(logical.0, logical.1)), true);
    }
    ns.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );
    // Above floating windows and full-screen apps, like the menu bar.
    ns.setLevel(NSStatusWindowLevel);
    // On screen without making Talkr active or the pill key.
    ns.orderFrontRegardless();
}

/// Take the pill off screen. Main thread only.
pub fn hide(window: &tauri::WebviewWindow) {
    if MainThreadMarker::new().is_none() {
        return;
    }
    if let Some(ns) = ns_window(window) {
        ns.orderOut(None);
    }
}
