//! The app with keyboard focus (through Accessibility, or NSWorkspace): who gets the text, and
//! bringing them back to the front when focus moved during a dictation.

use std::time::{Duration, Instant};
use objc2::rc::autoreleasepool;
use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication, NSWorkspace};
use super::ax;
use super::target::Target;

fn target(app: &NSRunningApplication) -> Target {
    Target {
        pid: app.processIdentifier(),
        bundle_id: app.bundleIdentifier().map(|id| id.to_string()).unwrap_or_default(),
        name: app.localizedName().map(|name| name.to_string()),
    }
}

/// The app with keyboard focus right now: from Accessibility when allowed (always current),
/// else NSWorkspace's frontmost app.
pub fn snapshot() -> Option<Target> {
    autoreleasepool(|_| {
        if let Some(pid) = ax::focused_app_pid() {
            return Some(match NSRunningApplication::runningApplicationWithProcessIdentifier(pid) {
                Some(app) => target(&app),
                None => Target { pid, ..Target::default() },
            });
        }
        NSWorkspace::sharedWorkspace().frontmostApplication().map(|app| target(&app))
    })
}

/// The process id of the app with keyboard focus.
pub fn frontmost_pid() -> Option<i32> {
    ax::focused_app_pid()
        .or_else(|| autoreleasepool(|_| NSWorkspace::sharedWorkspace().frontmostApplication().map(|app| app.processIdentifier())))
}

/// Whether the app is still running.
pub fn is_running(target: &Target) -> bool {
    autoreleasepool(|_| {
        NSRunningApplication::runningApplicationWithProcessIdentifier(target.pid).is_some_and(|app| !app.isTerminated())
    })
}

/// Bring `target` back to the front. Its own window and field come back with it, as when the user
/// switches apps. Returns whether it is frontmost.
pub fn activate(target: &Target) -> bool {
    if frontmost_pid() == Some(target.pid) {
        return true;
    }
    let asked = autoreleasepool(|_| {
        let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(target.pid) else { return false };
        if app.isTerminated() {
            return false;
        }
        // All windows, ignoring other apps (written as bits: the flag is deprecated in macOS 14,
        // where it does nothing, but older systems need it).
        app.activateWithOptions(NSApplicationActivationOptions(0b11))
    });
    if !asked {
        return false;
    }
    let started = Instant::now();
    let mut raised = false;
    while started.elapsed() < Duration::from_millis(600) {
        if frontmost_pid() == Some(target.pid) {
            // Give the app a moment to put the cursor back in its field.
            std::thread::sleep(Duration::from_millis(60));
            return true;
        }
        if !raised && started.elapsed() >= Duration::from_millis(150) {
            // macOS 14 can refuse activation requested by an app that is not active itself.
            raised = ax::raise(target.pid);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_frontmost_app_can_be_read() {
        // A CI runner may have no frontmost app; either way it must not crash.
        if let Some(t) = snapshot() {
            assert!(t.pid > 0, "{t:?}");
            assert!(frontmost_pid().is_some());
            assert!(is_running(&t));
        }
        assert!(!is_running(&Target { pid: i32::MAX, ..Target::default() }));
        assert!(!activate(&Target { pid: i32::MAX, ..Target::default() }), "no such app");
    }
}
