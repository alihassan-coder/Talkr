//! The pill's window on macOS: shown without activating Talkr (the app being dictated into keeps
//! focus), on every Space and above full-screen apps, on the display the user is working on.
//! AppKit windows belong to the main thread, so both calls run there.
//!
//! An ordinary window cannot do that: macOS keeps a regular app's windows off another app's
//! full-screen Space, whatever their level and collection behaviour, and a click on one
//! activates its app. A non-activating panel can (it is what Spotlight and input-method windows
//! are), so the first time the pill is shown its window becomes one: its class is changed at
//! run time from Tauri's window class to a subclass of `NSPanel` (the way the tauri-nspanel
//! plugin does it), and it gets the non-activating style. The change is made only when it is
//! safe: the window must still be of the class Tauri made it with (anything else, such as a
//! class added by key-value observing, is left alone), and the panel class must have exactly the
//! same size and instance variables, so no memory is read differently. The panel refuses to
//! become key or main: keys keep going to the app being dictated into, never to the pill, while
//! its buttons still take clicks. If the change cannot be made, the pill stays a plain window
//! above other windows, as before.

use std::sync::OnceLock;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel};
use objc2::{msg_send, sel, MainThreadMarker};
use objc2_app_kit::{NSEvent, NSScreen, NSStatusWindowLevel, NSWindow, NSWindowCollectionBehavior, NSWindowStyleMask};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use super::placement::{self, Frame};
use crate::dictation::settings::OverlayPosition;

/// The window class tao (Tauri's windowing library) gives every window.
const TAO_WINDOW: &std::ffi::CStr = c"TaoWindow";
/// An instance variable tao's class adds, which the panel class repeats at the same place.
const TAO_IVAR: &std::ffi::CStr = c"focusable";

fn ns_window(window: &tauri::WebviewWindow) -> Option<Retained<NSWindow>> {
    let ptr = window.ns_window().ok()?;
    // SAFETY: Tauri returns the window's live NSWindow, used here on the main thread and retained
    // for as long as it is held.
    unsafe { Retained::retain(ptr.cast::<NSWindow>()) }
}

fn frame(rect: NSRect) -> Frame {
    (rect.origin.x, rect.origin.y, rect.size.width, rect.size.height)
}

extern "C" fn refuse(_this: &AnyObject, _cmd: Sel) -> Bool {
    Bool::NO
}

/// `TalkrPillPanel`: an `NSPanel` that never becomes key or main, laid out like tao's window
/// class (the same instance variable), registered once.
fn panel_class() -> Option<&'static AnyClass> {
    static CLASS: OnceLock<Option<&'static AnyClass>> = OnceLock::new();
    *CLASS.get_or_init(|| {
        let mut builder = ClassBuilder::new(c"TalkrPillPanel", AnyClass::get(c"NSPanel")?)?;
        // SAFETY: both methods take no arguments and return a BOOL, as the ones they override.
        unsafe {
            builder.add_method(sel!(canBecomeKeyWindow), refuse as extern "C" fn(_, _) -> _);
            builder.add_method(sel!(canBecomeMainWindow), refuse as extern "C" fn(_, _) -> _);
        }
        builder.add_ivar::<Bool>(TAO_IVAR);
        Some(builder.register())
    })
}

/// Whether an object of class `from` may become a `to`: the same size, and tao's instance
/// variable at the same offset.
fn same_layout(from: &AnyClass, to: &AnyClass) -> bool {
    let offset = |class: &AnyClass| class.instance_variable(TAO_IVAR).map(|ivar| ivar.offset());
    from.instance_size() == to.instance_size() && offset(from).is_some() && offset(from) == offset(to)
}

/// Turn the pill's window into a non-activating panel (see the module's notes). Idempotent.
fn make_panel(ns: &NSWindow) {
    let Some(panel) = panel_class() else { return };
    let object: &AnyObject = ns;
    let class = object.class();
    if !std::ptr::eq(class, panel) {
        if class.name() != TAO_WINDOW || !same_layout(class, panel) {
            static WARNED: OnceLock<()> = OnceLock::new();
            WARNED.get_or_init(|| log::warn!("the dictation pill stays a plain window (its class is {:?})", class.name()));
            return;
        }
        // SAFETY: checked above: the object is still of tao's class, and the panel class has the
        // same size and instance variables. NSPanel adds behaviour only, no storage; every method
        // the panel class overrides keeps its signature.
        let _previous = unsafe { AnyObject::set_class(object, panel) };
        log::debug!("the dictation pill is a non-activating panel");
    }
    ns.setStyleMask(ns.styleMask() | NSWindowStyleMask::NonactivatingPanel);
    // A panel hides when its app is not active, which Talkr never is while dictating.
    ns.setHidesOnDeactivate(false);
    // SAFETY: NSPanel methods, on an object that is an NSPanel now; both take a BOOL.
    unsafe {
        let _: () = msg_send![ns, setFloatingPanel: true];
        let _: () = msg_send![ns, setBecomesKeyOnlyIfNeeded: true];
    }
}

/// Show the pill, `logical` points large, centred on the display that holds `anchor` (the middle
/// of the target's window, in Accessibility coordinates) or else the mouse. Main thread only.
pub fn show(window: &tauri::WebviewWindow, anchor: Option<(f64, f64)>, logical: (f64, f64), position: OverlayPosition) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let Some(ns) = ns_window(window) else { return };
    make_panel(&ns);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The panel class registers, is an NSPanel, refuses key and main status, and has exactly
    /// the layout of a window class like tao's, so the swap is allowed; a class with another
    /// layout is refused.
    #[test]
    fn the_panel_class_matches_a_tao_like_window() {
        let panel = panel_class().expect("the panel class registers");
        assert!(panel.superclass().is_some_and(|s| s.name() == c"NSPanel"));
        assert!(panel.instance_method(sel!(canBecomeKeyWindow)).is_some());

        let window = AnyClass::get(c"NSWindow").unwrap();
        let mut tao_like = ClassBuilder::new(c"TalkrTestTaoLikeWindow", window).unwrap();
        tao_like.add_ivar::<Bool>(TAO_IVAR);
        let tao_like = tao_like.register();
        assert!(same_layout(tao_like, panel), "NSPanel adds no storage to NSWindow");

        let mut other = ClassBuilder::new(c"TalkrTestOtherWindow", window).unwrap();
        other.add_ivar::<u64>(c"something");
        let other = other.register();
        assert!(!same_layout(other, panel));
        assert!(!same_layout(window, panel), "tao's instance variable must be there");
    }
}
