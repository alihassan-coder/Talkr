//! The macOS side of dictation: a Core Graphics event tap for the shortcut, the Accessibility
//! API to read the focused field, and insertion by pasting (with the pasteboard given back) or by
//! typing. All of it needs the Accessibility permission, which the settings page asks for.

mod ax;
mod ffi;
mod field;
mod insert;
mod keymap;
mod keys;
mod layout;
mod machine;
mod pasteboard;
mod pill;
mod placement;
mod tap;
mod target;
mod workspace;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
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
        static THIS_SESSION: AtomicBool = AtomicBool::new(false);
        let asked_before = note_prompt(&THIS_SESSION, prompt_marker().as_deref());
        if ffi::prompt_for_trust() {
            return;
        }
        // macOS shows its prompt only the first time an app ever asks (and remembers that across
        // launches): from then on only System Settings can allow Talkr, so open it. Not on the
        // first request, which the prompt answers (it has its own button to open the pane).
        if asked_before {
            if let Err(e) = std::process::Command::new("/usr/bin/open").arg(SETTINGS_URL).spawn() {
                log::warn!("could not open the Accessibility settings: {}", e);
            }
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
        // A key shortcut's key is kept from apps by the listener: ask the listener about it.
        let listener = tap::is_running().then(tap::dictate_held);
        Some(keys::shortcut_held(shortcut, listener))
    }

    fn snapshot() -> Option<Self::Target> {
        workspace::snapshot()
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
        owner: Option<&tauri::WebviewWindow>,
    ) -> Delivery {
        // Text meant for a password box is copied marked concealed and transient, so clipboard
        // managers leave it out of their history.
        let copy = |reason: target::CopyReason, password: bool| Delivery {
            password,
            ..Delivery::copied(reason.message(), pasteboard::put_text(clean, password).is_some())
        };
        let Some(t) = target else { return copy(target::CopyReason::NoTextField, false) };
        let ready = match insert::prepare(t, settings) {
            Prepared::Copy(reason) => return copy(reason, false),
            Prepared::Ready(ready) => ready,
        };
        let password = ready.field.as_ref().is_some_and(|f| f.password);
        let before = ready.field.as_ref().and_then(|f| f.before_caret.as_deref());
        let fitted = match before {
            Some(before) if settings.smart_spacing => text::fit_to_context(clean, before),
            _ => clean.to_string(),
        };
        match insert::deliver(&ready, &fitted, settings, owner) {
            Outcome::Inserted { method, verdict, learned_typing } => Delivery {
                inserted: true,
                verified: verdict == field::Verdict::Arrived,
                method: Some(method.name()),
                password,
                learned_typing_for: if learned_typing { ready.target.app_id() } else { None },
                ..Delivery::default()
            },
            Outcome::Copied(reason) => copy(reason, password),
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

/// Where Talkr notes that it has asked macOS for Accessibility before: next to its other cached
/// state, in the data folder (`TALKR_HOME`, else the home folder).
fn prompt_marker() -> Option<PathBuf> {
    let base = match std::env::var_os("TALKR_HOME").filter(|v| !v.is_empty()) {
        Some(v) => {
            let path = PathBuf::from(v);
            if path.is_absolute() {
                path
            } else {
                std::env::current_dir().ok()?.join(path)
            }
        }
        None => dirs::home_dir()?,
    };
    Some(base.join(".talkr").join("cache").join("accessibility-prompted"))
}

/// Note a request for the Accessibility prompt; returns whether one was made before, in this
/// session (`session`) or an earlier one (`marker` exists).
fn note_prompt(session: &AtomicBool, marker: Option<&Path>) -> bool {
    let mut before = session.swap(true, Ordering::SeqCst);
    if let Some(marker) = marker {
        before |= marker.exists();
        let written = marker.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(marker, b""));
        if let Err(e) = written {
            log::debug!("could not note the Accessibility prompt: {}", e);
        }
    }
    before
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The first request shows macOS's prompt only; a later one, in this session or after a
    /// restart, also opens System Settings, where macOS sends the user from then on.
    #[test]
    fn settings_open_only_once_the_prompt_was_shown() {
        let dir = std::env::temp_dir().join(format!("talkr-prompt-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let marker = dir.join("cache").join("accessibility-prompted");
        let session = AtomicBool::new(false);
        assert!(!note_prompt(&session, Some(&marker)), "first request: the prompt only");
        assert!(marker.exists());
        assert!(note_prompt(&session, Some(&marker)), "asked again in this session");
        assert!(note_prompt(&AtomicBool::new(false), Some(&marker)), "asked again after a restart");
        assert!(!note_prompt(&AtomicBool::new(false), None));
        let _ = std::fs::remove_dir_all(&dir);
    }

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
                // A dialog that appeared meanwhile is closed again; then the field is brought
                // to the front (by activation, and through Accessibility where that is refused).
                if let Some(d) = front.as_ref().filter(|t| dialog(t)) {
                    if let Some(done) = ax::dismiss_dialog(d.pid) {
                        note(done);
                    }
                }
                workspace::activate(&Target { pid, ..Target::default() });
            }
            assert!(
                workspace::is_running(&Target { pid, ..Target::default() }),
                "the test field app ended before it got focus"
            );
            if last_note.elapsed() >= Duration::from_secs(5) {
                last_note = Instant::now();
                note(format!("waiting: front {front:?}, focused {focused:?}"));
            }
            if started.elapsed() >= Duration::from_secs(45) {
                // Shared CI runners sometimes keep another app (Finder, a system dialog) in
                // front of a window a background process opens, whatever it asks: that is the
                // runner's desktop, not dictation, so the test is skipped rather than failed.
                note(format!("skipped: the test field never became frontmost (front {front:?}, focused {focused:?})"));
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
