//! Getting text into the field the user chose, and making sure it got there. The same steps as
//! on Windows (`win/insert.rs`):
//!
//! 1. Check: the app is still the one the user started in (or bring it back), and its focused
//!    element takes text.
//! 2. Insert by pasting (⌘V with the pasteboard borrowed and given back), or by typing into
//!    remote desktops and apps the user marked.
//! 3. Verify by reading the field back, and only when it is certain nothing went in, try typing.
//!    Text is never inserted twice.
//! 4. Recover: when it cannot be inserted, it is left on the clipboard and the user is told.

use std::time::{Duration, Instant};
use super::ax;
use super::ffi;
use super::field::{self, FieldInfo, Verdict};
use super::keys;
use super::pasteboard;
use super::target::{self, CopyReason, Method, Target};
use super::workspace;
use crate::dictation::settings::{DictationSettings, FocusPolicy};

/// Everything decided before inserting.
pub struct Ready {
    pub target: Target,
    /// The field, read just before inserting (`None` when it cannot be read).
    pub field: Option<FieldInfo>,
    pub method: Method,
    /// Line breaks would run commands.
    pub terminal: bool,
    /// The method came from the user (a setting or an app rule), not from Talkr's choice.
    pub forced: bool,
}

pub enum Prepared {
    Ready(Box<Ready>),
    Copy(CopyReason),
}

pub enum Outcome {
    Inserted { method: Method, verdict: Verdict, learned_typing: bool },
    Copied(CopyReason),
}

/// Make sure the right app has focus and decide how to insert. Waits for the user to let go of
/// the shortcut's keys first: pasting with ⌃ and ⌘ still down would be ⌃⌘V.
pub fn prepare(original: &Target, settings: &DictationSettings) -> Prepared {
    // Without Accessibility, macOS drops the keys Talkr posts without a word.
    if !ffi::trusted() {
        return Prepared::Copy(CopyReason::NoPermission);
    }
    if !keys::wait_for_modifiers_released(Duration::from_millis(2_000)) {
        log::warn!("modifier keys still held after 2 s; inserting anyway");
    }

    let mut target = original.clone();
    let front = workspace::frontmost_pid();
    if front != Some(original.pid) {
        // A click on the pill makes Talkr the active app: always go back from there.
        let ours = front == Some(std::process::id() as i32);
        match settings.focus_policy {
            FocusPolicy::Original | FocusPolicy::Current if ours => {
                if !workspace::activate(original) {
                    return Prepared::Copy(CopyReason::FocusChanged);
                }
            }
            FocusPolicy::Original => {
                if !workspace::is_running(original) {
                    return Prepared::Copy(CopyReason::TargetClosed);
                }
                if !workspace::activate(original) {
                    return Prepared::Copy(CopyReason::FocusChanged);
                }
            }
            FocusPolicy::Current => match workspace::snapshot() {
                Some(now) => target = now,
                None => return Prepared::Copy(CopyReason::NoTextField),
            },
            FocusPolicy::Copy => return Prepared::Copy(CopyReason::FocusChanged),
        }
    }

    let terminal = target.is_terminal();
    let field = ax::inspect().filter(|f| f.pid == 0 || f.pid == target.pid);
    if let Some(f) = &field {
        log::debug!("dictation field: {} {} (takes text: {:?})", f.role, f.subrole, f.editable);
    }
    match check_field(&target, field.as_ref(), terminal) {
        Some(reason) => Prepared::Copy(reason),
        None => match target::choose(&target, settings) {
            Ok((method, forced)) => Prepared::Ready(Box::new(Ready { target, field, method, terminal, forced })),
            Err(reason) => Prepared::Copy(reason),
        },
    }
}

/// Why the focused element cannot take text, if it cannot.
fn check_field(target: &Target, field: Option<&FieldInfo>, terminal: bool) -> Option<CopyReason> {
    // Terminals expose their screen as read-only text, yet take typing: skip the check there.
    if terminal {
        return None;
    }
    let editable = field.and_then(|f| f.editable);
    // The desktop and the Dock: only a field that clearly takes text (renaming a file).
    if target.is_shell() && editable != Some(true) {
        return Some(CopyReason::NoTextField);
    }
    (editable == Some(false)).then_some(CopyReason::NoTextField)
}

/// Insert `text` (already fitted to the field).
pub fn deliver(ready: &Ready, text: &str, settings: &DictationSettings) -> Outcome {
    let method = ready.method;
    match run(method, ready, text, settings) {
        Some(Verdict::Unchanged) if method == Method::Paste && can_retry(ready) => {
            // A slow app can take the paste after the check gave up: look once more before
            // typing, or the text could arrive twice.
            std::thread::sleep(Duration::from_millis(350));
            let late = ax::inspect();
            let verdict = field::verdict(ready.field.as_ref(), late.as_ref(), text);
            if verdict != Verdict::Unchanged {
                return Outcome::Inserted { method, verdict, learned_typing: false };
            }
            // Certain the paste did not land: type it instead, and remember that for this app.
            log::info!("paste did not reach {}; typing instead", ready.target.app_id().unwrap_or_default());
            let typing = ready.target.typing();
            match run(typing, ready, text, settings) {
                Some(Verdict::Unchanged) | None => Outcome::Copied(CopyReason::NotDelivered),
                Some(v) => Outcome::Inserted { method: typing, verdict: v, learned_typing: v == Verdict::Arrived },
            }
        }
        Some(Verdict::Unchanged) | None => Outcome::Copied(CopyReason::NotDelivered),
        Some(v) => Outcome::Inserted { method, verdict: v, learned_typing: false },
    }
}

/// A second attempt is only safe where the field is known to take text and could be read.
fn can_retry(ready: &Ready) -> bool {
    !ready.forced && ready.field.as_ref().is_some_and(|f| f.editable == Some(true))
}

/// Insert once with `method`. `None` when it could not even be attempted.
fn run(method: Method, ready: &Ready, text: &str, settings: &DictationSettings) -> Option<Verdict> {
    let text = if ready.terminal { crate::dictation::text::single_line(text) } else { text.to_string() };
    match method {
        Method::Paste => {
            let saved = settings.restore_clipboard.then(pasteboard::save);
            let Some(count) = pasteboard::put_text(&text, settings.restore_clipboard) else {
                // Cleared but not filled: give the user's content back.
                if let Some(saved) = saved {
                    pasteboard::restore(saved, pasteboard::change_count());
                }
                return None;
            };
            let sent = keys::paste();
            if !sent {
                log::warn!("macOS did not accept the paste keystroke");
            }
            let started = Instant::now();
            let verdict = watch(ready.field.as_ref(), &text);
            // The app reads the pasteboard after it handles the keystroke; restoring too early
            // would paste the old content.
            let slow = ready.target.is_slow_paste() || ready.field.is_none();
            let settle = Duration::from_millis(if slow { 450 } else { 220 });
            if let Some(rest) = settle.checked_sub(started.elapsed()) {
                std::thread::sleep(rest);
            }
            if let Some(saved) = saved {
                pasteboard::restore(saved, count);
            }
            Some(if sent { verdict } else { Verdict::Unchanged })
        }
        Method::Type { line_break } => {
            if !keys::type_text(&text, line_break) {
                log::warn!("macOS did not accept all typed keys");
                return Some(Verdict::Unknown);
            }
            Some(watch(ready.field.as_ref(), &text))
        }
    }
}

/// Re-read the field until the text shows up, for up to about 0.7 s. Returns `Unchanged` only
/// if it never changed at all.
fn watch(before: Option<&FieldInfo>, text: &str) -> Verdict {
    let Some(before) = before else { return Verdict::Unknown };
    if before.before_caret.is_none() && before.value.is_none() {
        return Verdict::Unknown;
    }
    let mut last = Verdict::Unknown;
    for wait in [50u64, 80, 120, 180, 260] {
        std::thread::sleep(Duration::from_millis(wait));
        let after = ax::inspect();
        last = field::verdict(Some(before), after.as_ref(), text);
        if last == Verdict::Arrived {
            return last;
        }
        if after.is_none() {
            return Verdict::Unknown;
        }
    }
    last
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(bundle_id: &str) -> Target {
        Target { pid: 7, bundle_id: bundle_id.into(), name: None }
    }

    fn field(role: &str, editable: Option<bool>) -> FieldInfo {
        FieldInfo { role: role.into(), editable, ..FieldInfo::default() }
    }

    #[test]
    fn fields_that_take_no_text_are_refused() {
        let notes = app("com.apple.Notes");
        assert_eq!(check_field(&notes, Some(&field("AXTextArea", Some(true))), false), None);
        assert_eq!(check_field(&notes, Some(&field("AXGroup", None)), false), None, "unknown: let it through");
        assert_eq!(check_field(&notes, None, false), None, "unreadable: let it through");
        assert_eq!(check_field(&notes, Some(&field("AXButton", Some(false))), false), Some(CopyReason::NoTextField));
    }

    #[test]
    fn terminals_skip_the_check() {
        let terminal = app("com.apple.Terminal");
        assert_eq!(check_field(&terminal, Some(&field("AXTextArea", Some(false))), true), None);
    }

    #[test]
    fn the_desktop_needs_a_real_field() {
        let finder = app("com.apple.finder");
        assert_eq!(check_field(&finder, Some(&field("AXList", Some(false))), false), Some(CopyReason::NoTextField));
        assert_eq!(check_field(&finder, None, false), Some(CopyReason::NoTextField));
        assert_eq!(check_field(&finder, Some(&field("AXTextField", Some(true))), false), None, "renaming a file");
    }

    #[test]
    fn retries_only_when_safe() {
        let ready = |forced: bool, editable: Option<bool>| Ready {
            target: app("com.apple.TextEdit"),
            field: Some(field("AXTextArea", editable)),
            method: Method::Paste,
            terminal: false,
            forced,
        };
        assert!(can_retry(&ready(false, Some(true))));
        assert!(!can_retry(&ready(true, Some(true))), "the user chose pasting");
        assert!(!can_retry(&ready(false, None)), "the field could not be judged");
    }
}
