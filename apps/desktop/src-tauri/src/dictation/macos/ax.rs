//! Reading the focused field through the Accessibility API, the one VoiceOver uses: its role, the
//! text before the cursor, its value. Every call goes to the focused app's process, so each has a
//! short messaging timeout: a hung app costs at most a moment, never a frozen dictation.

use std::ffi::c_void;
use std::sync::{Mutex, OnceLock};
use super::ffi::{self, Cf, CFRange, CGPoint, CGSize, AX_SUCCESS};
use super::field::{self, FieldInfo, CONTEXT_CHARS, MAX_VALUE_CHARS};

/// Longest wait for an app to answer one question.
const TIMEOUT_SECONDS: f32 = 0.5;

/// The system-wide element, with the timeout that applies to every element.
fn system() -> Option<&'static SystemWide> {
    static SYSTEM: OnceLock<Option<SystemWide>> = OnceLock::new();
    SYSTEM
        .get_or_init(|| {
            // SAFETY: Create returns an owned element.
            let element = unsafe { Cf::from_owned(ffi::AXUIElementCreateSystemWide()) }?;
            // SAFETY: a valid element. On the system-wide element the timeout is the global one.
            unsafe { ffi::AXUIElementSetMessagingTimeout(element.as_ptr(), TIMEOUT_SECONDS) };
            Some(SystemWide(element))
        })
        .as_ref()
}

struct SystemWide(Cf);

// SAFETY: the system-wide AXUIElement is immutable and the Accessibility API may be called from
// any thread.
unsafe impl Sync for SystemWide {}

/// A copy of `attribute` of `element`, if it has one.
fn attribute(element: &Cf, attribute: &str) -> Option<Cf> {
    let name = ffi::cf_string(attribute)?;
    let mut value: ffi::CFTypeRef = std::ptr::null();
    // SAFETY: valid element and name; on success we own the returned value.
    let error = unsafe { ffi::AXUIElementCopyAttributeValue(element.as_ptr(), name.as_ptr(), &mut value) };
    if error != AX_SUCCESS {
        return None;
    }
    // SAFETY: Copy returns an owned reference.
    unsafe { Cf::from_owned(value) }
}

fn settable(element: &Cf, attribute: &str) -> Option<bool> {
    let name = ffi::cf_string(attribute)?;
    let mut settable = 0u8;
    // SAFETY: valid element and name; writes one Boolean.
    let error = unsafe { ffi::AXUIElementIsAttributeSettable(element.as_ptr(), name.as_ptr(), &mut settable) };
    (error == AX_SUCCESS).then_some(settable != 0)
}

fn string_for_range(element: &Cf, range: CFRange) -> Option<String> {
    let name = ffi::cf_string("AXStringForRange")?;
    // SAFETY: AXValueCreate copies the CFRange it is given; the result is owned.
    let parameter = unsafe { Cf::from_owned(ffi::AXValueCreate(ffi::AX_VALUE_CFRANGE, (&raw const range).cast::<c_void>())) }?;
    let mut value: ffi::CFTypeRef = std::ptr::null();
    // SAFETY: valid element, name and parameter; on success we own the returned value.
    let error = unsafe {
        ffi::AXUIElementCopyParameterizedAttributeValue(element.as_ptr(), name.as_ptr(), parameter.as_ptr(), &mut value)
    };
    if error != AX_SUCCESS {
        return None;
    }
    // SAFETY: Copy returns an owned reference.
    unsafe { Cf::from_owned(value) }?.to_string_lossy()
}

/// The focused element, system-wide.
fn focused() -> Option<Cf> {
    attribute(&system()?.0, "AXFocusedUIElement")
}

/// The process of the app with keyboard focus, as Accessibility sees it. Unlike NSWorkspace's
/// frontmost app (updated through notifications on the main thread's run loop) it is read fresh
/// on every call, and it follows panels that take keys without activating their app (Spotlight).
pub fn focused_app_pid() -> Option<i32> {
    if !ffi::trusted() {
        return None;
    }
    let app = attribute(&system()?.0, "AXFocusedApplication")?;
    let mut pid = 0i32;
    // SAFETY: valid element; writes one pid.
    let error = unsafe { ffi::AXUIElementGetPid(app.as_ptr(), &mut pid) };
    (error == AX_SUCCESS && pid > 0).then_some(pid)
}

/// Inspect the field with keyboard focus. `None` when nothing could be read (Accessibility not
/// allowed, no focused element, an app that does not answer).
pub fn inspect() -> Option<FieldInfo> {
    if !ffi::trusted() {
        return None;
    }
    let element = focused()?;
    let mut pid = 0i32;
    // SAFETY: valid element; writes one pid.
    unsafe { ffi::AXUIElementGetPid(element.as_ptr(), &mut pid) };
    let role = attribute(&element, "AXRole").and_then(|r| r.to_string_lossy()).unwrap_or_default();
    let subrole = attribute(&element, "AXSubrole").and_then(|r| r.to_string_lossy()).unwrap_or_default();
    let password = field::is_password(&role, &subrole);
    let selection = attribute(&element, "AXSelectedTextRange").and_then(|r| r.to_ax_value::<CFRange>(ffi::AX_VALUE_CFRANGE));
    let value_settable = settable(&element, "AXValue");
    let mut info = FieldInfo {
        pid,
        editable: field::editable(&role, value_settable, selection.is_some()),
        role,
        subrole,
        password,
        before_caret: None,
        value: None,
    };
    if password {
        // Never read what is in a password box.
        return Some(info);
    }
    let length = attribute(&element, "AXNumberOfCharacters").and_then(|n| n.to_i64());
    let small = length.is_some_and(|n| (0..=MAX_VALUE_CHARS as i64).contains(&n));
    // Read the whole value only when it is known to be small (a long document is slow to copy).
    let value = if small { attribute(&element, "AXValue").and_then(|v| v.to_utf16()) } else { None };
    if let Some(caret) = selection.map(|s| s.location.max(0) as usize) {
        info.before_caret = match &value {
            Some(units) => Some(field::before_caret(units, caret)),
            None => {
                let start = caret.saturating_sub(CONTEXT_CHARS * 2);
                let range = CFRange { location: start as isize, length: (caret - start) as isize };
                string_for_range(&element, range).map(|text| field::tail(text.trim_matches('\u{FFFD}'), CONTEXT_CHARS))
            }
        };
    }
    info.value = value.map(|units| String::from_utf16_lossy(&units));
    Some(info)
}

/// The middle of the focused window of app `pid`, in screen coordinates with the origin at the
/// top left of the main display (as the Accessibility API gives them).
pub fn focused_window_center(pid: i32) -> Option<(f64, f64)> {
    if !ffi::trusted() || pid <= 0 {
        return None;
    }
    // SAFETY: Create returns an owned element.
    let app = unsafe { Cf::from_owned(ffi::AXUIElementCreateApplication(pid)) }?;
    // SAFETY: valid element. Placing the pill must not wait on a busy app.
    unsafe { ffi::AXUIElementSetMessagingTimeout(app.as_ptr(), 0.2) };
    let window = attribute(&app, "AXFocusedWindow")?;
    let origin = attribute(&window, "AXPosition")?.to_ax_value::<CGPoint>(ffi::AX_VALUE_CGPOINT)?;
    let size = attribute(&window, "AXSize")?.to_ax_value::<CGSize>(ffi::AX_VALUE_CGSIZE)?;
    Some((origin.x + size.width / 2.0, origin.y + size.height / 2.0))
}

/// Ask app `pid` to build its accessibility tree; returns whether it took the request. Electron
/// and Chromium apps (Slack, VS Code, Discord, Chrome) build it only when an assistive app asks,
/// through `AXManualAccessibility`; without it their fields cannot be read, so smart spacing and
/// verification would be off there. Other apps answer that the attribute is unsupported, which
/// changes nothing.
pub fn expose(pid: i32) -> bool {
    if !ffi::trusted() || pid <= 0 {
        return false;
    }
    // SAFETY: Create returns an owned element.
    let Some(app) = (unsafe { Cf::from_owned(ffi::AXUIElementCreateApplication(pid)) }) else { return false };
    let Some(name) = ffi::cf_string("AXManualAccessibility") else { return false };
    // SAFETY: valid element and name; kCFBooleanTrue is a constant.
    unsafe {
        ffi::AXUIElementSetMessagingTimeout(app.as_ptr(), 0.3);
        ffi::AXUIElementSetAttributeValue(app.as_ptr(), name.as_ptr(), ffi::kCFBooleanTrue) == AX_SUCCESS
    }
}

/// [`expose`] app `pid`, the first time its focused field could not be read; returns whether it
/// was asked just now and took the request.
///
/// Only then: an app keeps its tree for as long as it runs once asked, and in Chromium that
/// costs memory and CPU on every change to the page. Apps whose fields read fine are never
/// asked. Talkr does not switch it off again after a dictation either, since the next one would
/// pay for a rebuild (a long page takes seconds) and find the field unreadable again.
pub fn expose_once(pid: i32) -> bool {
    static DONE: Mutex<Vec<i32>> = Mutex::new(Vec::new());
    if !ffi::trusted() || pid <= 0 {
        return false;
    }
    {
        let mut done = DONE.lock().unwrap_or_else(|e| e.into_inner());
        if done.contains(&pid) {
            return false;
        }
        if done.len() >= 256 {
            done.clear();
        }
        done.push(pid);
    }
    expose(pid)
}

/// Bring app `pid` to the front through Accessibility, which works where activating it as an app
/// is refused (macOS 14 only lets the active app hand over activation).
pub fn raise(pid: i32) -> bool {
    // SAFETY: Create returns an owned element.
    let Some(app) = (unsafe { Cf::from_owned(ffi::AXUIElementCreateApplication(pid)) }) else { return false };
    let Some(name) = ffi::cf_string("AXFrontmost") else { return false };
    // SAFETY: valid element and name; kCFBooleanTrue is a constant.
    unsafe { ffi::AXUIElementSetAttributeValue(app.as_ptr(), name.as_ptr(), ffi::kCFBooleanTrue) == AX_SUCCESS }
}

/// Close the dialog app `pid` shows by pressing its cancel (or default) button; returns what it
/// pressed, for the log. For tests on CI runners, which can start with a system dialog in front.
#[cfg(test)]
pub fn dismiss_dialog(pid: i32) -> Option<String> {
    // SAFETY: Create returns an owned element.
    let app = unsafe { Cf::from_owned(ffi::AXUIElementCreateApplication(pid)) }?;
    let window = attribute(&app, "AXFocusedWindow").or_else(|| attribute(&app, "AXMainWindow"))?;
    let text = |element: &Cf, name: &str| attribute(element, name).and_then(|t| t.to_string_lossy()).unwrap_or_default();
    let title = text(&window, "AXTitle");
    let button = attribute(&window, "AXCancelButton").or_else(|| attribute(&window, "AXDefaultButton"))?;
    let label = text(&button, "AXTitle");
    let press = ffi::cf_string("AXPress")?;
    // SAFETY: valid element and action name.
    let error = unsafe { ffi::AXUIElementPerformAction(button.as_ptr(), press.as_ptr()) };
    Some(format!("dialog {title:?}: pressed {label:?} (AX error {error})"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// Whatever has focus on this machine (and whether Accessibility is allowed), reading it
    /// answers quickly and never crashes.
    #[test]
    fn reading_the_focused_field_never_hangs() {
        let started = Instant::now();
        let info = inspect();
        if !ffi::trusted() {
            assert_eq!(info, None, "nothing can be read without permission");
        }
        if let Some(target) = super::super::workspace::snapshot() {
            let _ = focused_window_center(target.pid);
            let _ = expose(target.pid);
        }
        assert_eq!(focused_window_center(0), None);
        assert!(!expose(0) && !expose_once(-1), "no such app");
        assert!(!raise(i32::MAX), "no such app");
        assert!(started.elapsed() < Duration::from_secs(5), "{:?}", started.elapsed());
    }
}
