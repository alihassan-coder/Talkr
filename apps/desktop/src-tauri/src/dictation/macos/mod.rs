//! The macOS side of dictation: a Core Graphics event tap for the shortcut, the Accessibility
//! API to read the focused field, and insertion by pasting (with the pasteboard given back) or by
//! typing. All of it needs the Accessibility permission, which the settings page asks for.

mod ax;
mod ffi;
mod field;
mod insert;
mod keymap;
mod keys;
mod machine;
mod pasteboard;
mod pill;
mod placement;
mod tap;
mod target;
mod workspace;

use std::sync::Mutex;
use crate::dictation::backend::{Backend, Capabilities, Delivery, Permission};
use crate::dictation::settings::{DictationSettings, OverlayPosition, Shortcut};
use crate::dictation::{text, HotkeyEvent};
use insert::{Outcome, Prepared};

pub use target::Target;

pub struct Os;

const SETTINGS_URL: &str = "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";

impl Backend for Os {
    type Target = Target;

    fn capabilities() -> Capabilities {
        Capabilities {
            os: "macos",
            supported: true,
            hold_to_talk: true,
            modifier_only: true,
            records_shortcut: true,
            verifies_insertion: ffi::trusted(),
            inserts_text: true,
            meta_key: "⌘",
            note: ffi::secure_input().then(|| {
                "Secure keyboard entry is on (a password field, or Terminal's Secure Keyboard Entry): macOS hides \
                 the keyboard from Talkr until it is off."
                    .into()
            }),
        }
    }

    fn permission() -> Permission {
        if ffi::trusted() {
            Permission::Granted
        } else {
            Permission::Missing {
                title: "Accessibility".into(),
                detail: "Talkr needs it to hear the shortcut in other apps and to type the text where your cursor is. \
                         Allow Talkr in System Settings › Privacy & Security › Accessibility."
                    .into(),
                can_request: true,
            }
        }
    }

    fn request_permission() {
        if ffi::prompt_for_trust() {
            return;
        }
        // macOS shows its prompt only the first time an app ever asks (and remembers that across
        // launches); the settings pane always opens.
        if let Err(e) = std::process::Command::new("/usr/bin/open").arg(SETTINGS_URL).spawn() {
            log::warn!("could not open the Accessibility settings: {}", e);
        }
    }

    fn start_hotkeys(events: flume::Sender<HotkeyEvent>) -> Result<(), String> {
        tap::start(events)
    }

    fn stop_hotkeys() {
        tap::stop();
    }

    fn hotkeys_running() -> bool {
        tap::is_running()
    }

    fn configure_hotkeys(dictate: Option<&Shortcut>, paste_last: Option<&Shortcut>) {
        tap::configure(dictate, paste_last);
    }

    fn set_recording(recording: bool) {
        tap::set_recording(recording);
    }

    fn set_capturing(capturing: bool) {
        tap::set_capturing(capturing);
    }

    fn shortcut_held(shortcut: &Shortcut) -> Option<bool> {
        Some(keys::shortcut_held(shortcut))
    }

    fn snapshot() -> Option<Self::Target> {
        let target = workspace::snapshot()?;
        expose_once(target.pid);
        Some(target)
    }

    fn app_label(t: &Self::Target) -> Option<String> {
        t.label()
    }

    fn app_id(t: &Self::Target) -> Option<String> {
        t.app_id()
    }

    fn deliver(
        clean: &str,
        target: Option<&Self::Target>,
        settings: &DictationSettings,
        _owner: Option<&tauri::WebviewWindow>,
    ) -> Delivery {
        let copy = |reason: target::CopyReason| Delivery::copied(reason.message(), pasteboard::put_text(clean, false).is_some());
        let Some(t) = target else { return copy(target::CopyReason::NoTextField) };
        let ready = match insert::prepare(t, settings) {
            Prepared::Copy(reason) => return copy(reason),
            Prepared::Ready(ready) => ready,
        };
        let password = ready.field.as_ref().is_some_and(|f| f.password);
        let before = ready.field.as_ref().and_then(|f| f.before_caret.as_deref());
        let fitted = match before {
            Some(before) if settings.smart_spacing => text::fit_to_context(clean, before),
            _ => clean.to_string(),
        };
        match insert::deliver(&ready, &fitted, settings) {
            Outcome::Inserted { method, verdict, learned_typing } => Delivery {
                inserted: true,
                verified: verdict == field::Verdict::Arrived,
                method: Some(method.name()),
                password,
                learned_typing_for: if learned_typing { ready.target.app_id() } else { None },
                ..Delivery::default()
            },
            Outcome::Copied(reason) => Delivery { password, ..copy(reason) },
        }
    }

    fn copy(text: &str, _owner: Option<&tauri::WebviewWindow>) -> bool {
        pasteboard::put_text(text, false).is_some()
    }

    fn show_overlay(window: &tauri::WebviewWindow, anchor: Option<&Self::Target>, logical: (f64, f64), position: OverlayPosition) {
        // Read here, off the main thread: the target app answers Accessibility questions itself.
        let anchor = anchor.and_then(|t| ax::focused_window_center(t.pid));
        let handle = window.clone();
        if let Err(e) = window.run_on_main_thread(move || pill::show(&handle, anchor, logical, position)) {
            log::warn!("could not show the dictation pill: {}", e);
        }
    }

    fn hide_overlay(window: &tauri::WebviewWindow) {
        let handle = window.clone();
        let _ = window.run_on_main_thread(move || pill::hide(&handle));
    }
}

/// Ask the app to expose its fields (see `ax::expose`) the first time it is dictated into, on a
/// thread of its own: the shortcut was just pressed and must not wait on another app. The text
/// goes in seconds later, by when the app has built its tree.
fn expose_once(pid: i32) {
    static DONE: Mutex<Vec<i32>> = Mutex::new(Vec::new());
    if !ffi::trusted() {
        return;
    }
    {
        let mut done = DONE.lock().unwrap_or_else(|e| e.into_inner());
        if done.contains(&pid) {
            return;
        }
        if done.len() >= 256 {
            done.clear();
        }
        done.push(pid);
    }
    let _ = std::thread::Builder::new().name("talkr-ax".into()).spawn(move || ax::expose(pid));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_and_permission_agree() {
        let c = Os::capabilities();
        assert_eq!((c.os, c.meta_key), ("macos", "⌘"));
        assert!(c.supported && c.hold_to_talk && c.modifier_only && c.records_shortcut && c.inserts_text);
        match Os::permission() {
            Permission::Granted => assert!(ffi::trusted()),
            Permission::Missing { title, can_request, .. } => {
                assert_eq!(title, "Accessibility");
                assert!(can_request && !ffi::trusted());
            }
            Permission::NotNeeded => panic!("macOS always needs Accessibility"),
        }
    }

    /// The copy fallback, end to end through the real pasteboard (CI only: it replaces what is
    /// on the clipboard).
    #[test]
    fn without_a_target_the_text_is_copied() {
        if std::env::var_os("CI").is_none() {
            return;
        }
        let _turn = pasteboard::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let d = Os::deliver("hello from the test", None, &DictationSettings::default(), None);
        assert!(!d.inserted);
        assert_eq!(d.copied.as_deref(), Some("No text field was selected"));
        assert!(!d.clipboard_failed);
    }

    /// A text field of the test's own in another process: a small Cocoa app run by `osascript`
    /// (JavaScript for Automation), ended when dropped.
    struct TestField(std::process::Child);

    impl Drop for TestField {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    const TEST_FIELD_APP: &str = r#"
ObjC.import('Cocoa');
var app = $.NSApplication.sharedApplication;
app.setActivationPolicy(0);
var win = $.NSWindow.alloc.initWithContentRectStyleMaskBackingDefer($.NSMakeRect(300, 300, 520, 260), 1, 2, false);
var view = $.NSTextView.alloc.initWithFrame($.NSMakeRect(0, 0, 520, 260));
view.setString($('Notes:'));
view.setAutomaticSpellingCorrectionEnabled(false);
view.setAutomaticTextReplacementEnabled(false);
view.setAutomaticQuoteSubstitutionEnabled(false);
view.setAutomaticDashSubstitutionEnabled(false);
view.setContinuousSpellCheckingEnabled(false);
win.setContentView(view);
win.setTitle($('Talkr test field'));
win.makeKeyAndOrderFront(null);
win.makeFirstResponder(view);
view.setSelectedRange($.NSMakeRange(6, 0));
app.activateIgnoringOtherApps(true);
app.run;
"#;

    /// Dictation into a real text field on a real desktop, the whole way: the target is
    /// snapshotted, the field read through Accessibility, the text pasted with ⌘V and then typed,
    /// both read back, and the user's clipboard given back. Runs on CI runners, which allow
    /// Accessibility (it types into the test's own window only).
    #[test]
    fn dictation_reaches_a_real_text_field() {
        use crate::dictation::settings::InsertMethod;
        use std::time::{Duration, Instant};
        if std::env::var_os("CI").is_none() || !ffi::trusted() {
            return;
        }
        let _turn = pasteboard::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // Straight to stderr, past the harness's capture, to show in the CI log.
        let note = |text: String| {
            use std::io::Write;
            let _ = writeln!(std::io::stderr(), "real text field test: {text}");
        };
        // A system dialog left over from the runner's start (an Accessibility prompt) can hold
        // the keyboard: close it, by its cancel button or else Escape.
        let dialog = |t: &Target| t.bundle_id == "com.apple.AccessibilityUIServer";
        for _ in 0..3 {
            let Some(front) = Os::snapshot().filter(dialog) else { break };
            note(format!("a dialog has focus: {front:?}"));
            match ax::dismiss_dialog(front.pid) {
                Some(done) => note(done),
                None => {
                    note("no button found; pressing Escape".into());
                    // SAFETY: owned source and events, released when dropped.
                    unsafe {
                        let source = ffi::Cf::from_owned(ffi::CGEventSourceCreate(ffi::STATE_PRIVATE)).unwrap();
                        for down in [true, false] {
                            let event = ffi::Cf::from_owned(ffi::CGEventCreateKeyboardEvent(source.as_ptr(), 0x35, down)).unwrap();
                            ffi::CGEventPost(ffi::HID_EVENT_TAP, event.as_ptr().cast_mut());
                        }
                    }
                }
            }
            std::thread::sleep(Duration::from_secs(1));
        }
        note(format!("frontmost before the test: {:?}", Os::snapshot()));
        let script = std::env::temp_dir().join("talkr-test-field.js");
        std::fs::write(&script, TEST_FIELD_APP).unwrap();
        let child = std::process::Command::new("/usr/bin/osascript").args(["-l", "JavaScript"]).arg(&script).spawn().unwrap();
        let field = TestField(child);
        let pid = field.0.id() as i32;

        let started = Instant::now();
        let mut last_note = started;
        let target = loop {
            let front = Os::snapshot();
            let focused = ax::inspect();
            if let Some(front) = front.clone().filter(|t| t.pid == pid) {
                if focused.as_ref().is_some_and(|f| f.pid == pid && f.role == "AXTextArea") {
                    note(format!("the field has focus after {:?}", started.elapsed()));
                    break front;
                }
            } else {
                workspace::activate(&Target { pid, ..Target::default() });
            }
            if last_note.elapsed() >= Duration::from_secs(5) {
                last_note = Instant::now();
                let running = workspace::is_running(&Target { pid, ..Target::default() });
                note(format!("waiting: front {front:?}, focused {focused:?}, field app running: {running}"));
            }
            if started.elapsed() >= Duration::from_secs(30) {
                // Only a system dialog that would not close may stand in the way.
                assert!(front.as_ref().is_some_and(dialog), "the test field never got focus: front {front:?}, focused {focused:?}");
                note("skipped: a system dialog keeps the keyboard".into());
                return;
            }
            std::thread::sleep(Duration::from_millis(250));
        };
        let value = || ax::inspect().and_then(|f| f.value).unwrap_or_default();
        assert_eq!(value(), "Notes:");

        pasteboard::put_text("the user's clipboard", false).unwrap();
        let d = Os::deliver("hello from Talkr", Some(&target), &DictationSettings::default(), None);
        assert!(d.inserted && d.copied.is_none(), "{}", d.summary());
        assert_eq!(d.method, Some("paste"));
        assert!(d.verified, "{}", d.summary());
        assert!(!d.password);
        let after_paste = value();
        assert!(after_paste.starts_with("Notes: ") && after_paste.ends_with("ello from Talkr"), "{after_paste:?}");
        assert_eq!(pasteboard::text().as_deref(), Some("the user's clipboard"), "the clipboard is given back");

        let typing = DictationSettings { insert_method: InsertMethod::Type, ..DictationSettings::default() };
        let d = Os::deliver("and typed words, ünïcode 😀", Some(&target), &typing, None);
        assert!(d.inserted && d.copied.is_none(), "{}", d.summary());
        assert_eq!(d.method, Some("type"));
        assert!(d.verified, "{}", d.summary());
        let after_typing = value();
        assert!(after_typing.starts_with(&after_paste) && after_typing.ends_with(" and typed words, ünïcode 😀"), "{after_typing:?}");
        assert_eq!(pasteboard::text().as_deref(), Some("the user's clipboard"), "typing leaves the clipboard alone");

        drop(field);
        let _ = std::fs::remove_file(script);
    }
}
