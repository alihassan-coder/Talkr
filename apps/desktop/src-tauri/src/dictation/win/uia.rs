//! Reading the focused field through UI Automation, the API screen readers use: whether it takes
//! text, whether it is a password box, and the text just before the cursor. That decides where
//! text may go, fits its spacing, and checks afterwards that it really arrived.
//!
//! Calls go to the focused app's process and an app that is not responding can hold them for
//! seconds, so they run on a worker thread and every request has a deadline. A worker stuck past
//! its deadline is abandoned and the next request starts a fresh one.

use std::sync::Mutex;
use std::time::Duration;
use windows::core::Interface;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, CUIAutomation8, IUIAutomation, IUIAutomation2, IUIAutomationTextPattern, IUIAutomationValuePattern,
    TextPatternRangeEndpoint_End, TextPatternRangeEndpoint_Start, TextUnit_Character, UIA_ButtonControlTypeId,
    UIA_CheckBoxControlTypeId, UIA_DocumentControlTypeId, UIA_EditControlTypeId, UIA_HyperlinkControlTypeId,
    UIA_ImageControlTypeId, UIA_ListItemControlTypeId, UIA_MenuBarControlTypeId, UIA_MenuItemControlTypeId,
    UIA_RadioButtonControlTypeId, UIA_ScrollBarControlTypeId, UIA_SliderControlTypeId, UIA_SplitButtonControlTypeId,
    UIA_TabItemControlTypeId, UIA_TextPatternId, UIA_TitleBarControlTypeId, UIA_ToolBarControlTypeId,
    UIA_TreeItemControlTypeId, UIA_ValuePatternId,
};

/// Characters read before the cursor: enough for spacing decisions and to recognise inserted text.
pub const CONTEXT_CHARS: usize = 96;
/// A field's whole value is read only up to this size (for checking insertion where there is no
/// cursor information).
const MAX_VALUE_CHARS: usize = 20_000;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FieldInfo {
    pub pid: u32,
    pub control_type: i32,
    pub password: bool,
    /// `Some(false)`: certainly not a place for text (a button, a file list, a read-only page).
    /// `None`: cannot tell, which is common and fine; text goes in as usual.
    pub editable: Option<bool>,
    /// Text just before the cursor, when the field exposes it (`Some("")` at the start).
    pub before_caret: Option<String>,
    /// The field's whole value, when it exposes one and it is small.
    pub value: Option<String>,
}

enum Request {
    Inspect(flume::Sender<Option<FieldInfo>>),
}

static WORKER: Mutex<Option<flume::Sender<Request>>> = Mutex::new(None);

fn worker() -> Option<flume::Sender<Request>> {
    let mut slot = WORKER.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(tx) = slot.as_ref() {
        return Some(tx.clone());
    }
    let (tx, rx) = flume::unbounded::<Request>();
    let spawned = std::thread::Builder::new().name("talkr-uia".into()).spawn(move || {
        // SAFETY: COM is initialised for this thread and torn down when it ends; all interface
        // pointers are dropped before CoUninitialize.
        unsafe {
            let initialised = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
            if let Some(automation) = create_automation() {
                for request in rx.iter() {
                    match request {
                        Request::Inspect(reply) => {
                            let info = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| inspect_focused(&automation)))
                                .unwrap_or(None);
                            let _ = reply.send(info);
                        }
                    }
                }
            } else {
                log::warn!("UI Automation is not available; dictation will insert without checking fields");
                for request in rx.iter() {
                    let Request::Inspect(reply) = request;
                    let _ = reply.send(None);
                }
            }
            if initialised {
                CoUninitialize();
            }
        }
    });
    if spawned.is_err() {
        return None;
    }
    *slot = Some(tx.clone());
    Some(tx)
}

unsafe fn create_automation() -> Option<IUIAutomation> {
    // SAFETY: COM is initialised on this thread (see `worker`).
    unsafe {
        let automation: IUIAutomation = CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)
            .or_else(|_| CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER))
            .ok()?;
        // UI Automation waits up to 20 s for an unresponsive app by default.
        if let Ok(a2) = automation.cast::<IUIAutomation2>() {
            let _ = a2.SetConnectionTimeout(1_000);
            let _ = a2.SetTransactionTimeout(1_500);
        }
        Some(automation)
    }
}

/// Inspect the focused element, waiting at most `timeout`. `None` when it could not be read.
pub fn inspect(timeout: Duration) -> Option<FieldInfo> {
    let tx = worker()?;
    let (reply_tx, reply_rx) = flume::bounded(1);
    tx.send(Request::Inspect(reply_tx)).ok()?;
    match reply_rx.recv_timeout(timeout) {
        Ok(info) => info,
        Err(_) => {
            // Stuck in a frozen app: abandon this worker. Its queue closes when the last sender
            // goes, and it exits once the call it is in returns.
            log::warn!("reading the focused field took over {:?}; skipping it", timeout);
            *WORKER.lock().unwrap_or_else(|e| e.into_inner()) = None;
            None
        }
    }
}

const NOT_TEXT: &[i32] = &[
    UIA_ButtonControlTypeId.0,
    UIA_CheckBoxControlTypeId.0,
    UIA_RadioButtonControlTypeId.0,
    UIA_HyperlinkControlTypeId.0,
    UIA_ImageControlTypeId.0,
    UIA_MenuItemControlTypeId.0,
    UIA_MenuBarControlTypeId.0,
    UIA_TabItemControlTypeId.0,
    UIA_TreeItemControlTypeId.0,
    UIA_ListItemControlTypeId.0,
    UIA_ScrollBarControlTypeId.0,
    UIA_SliderControlTypeId.0,
    UIA_TitleBarControlTypeId.0,
    UIA_ToolBarControlTypeId.0,
    UIA_SplitButtonControlTypeId.0,
];

/// Decide from the element's type and patterns whether text can go in.
pub fn editable(control_type: i32, read_only: Option<bool>, has_text: bool) -> Option<bool> {
    if read_only == Some(false) {
        return Some(true);
    }
    if NOT_TEXT.contains(&control_type) {
        return Some(false);
    }
    if control_type == UIA_EditControlTypeId.0 {
        return Some(read_only != Some(true));
    }
    if control_type == UIA_DocumentControlTypeId.0 && read_only == Some(true) {
        // A web page or PDF with no field focused.
        return Some(false);
    }
    if has_text && read_only.is_none() && control_type == UIA_DocumentControlTypeId.0 {
        return None;
    }
    None
}

unsafe fn inspect_focused(automation: &IUIAutomation) -> Option<FieldInfo> {
    // SAFETY: COM calls on interfaces owned by this thread.
    unsafe {
        let element = automation.GetFocusedElement().ok()?;
        let pid = element.CurrentProcessId().unwrap_or(0) as u32;
        let control_type = element.CurrentControlType().map(|c| c.0).unwrap_or(0);
        let password = element.CurrentIsPassword().map(|b| b.as_bool()).unwrap_or(false);
        let value_pattern: Option<IUIAutomationValuePattern> = element.GetCurrentPatternAs(UIA_ValuePatternId).ok();
        let read_only = value_pattern.as_ref().and_then(|v| v.CurrentIsReadOnly().ok()).map(|b| b.as_bool());
        let text_pattern: Option<IUIAutomationTextPattern> = element.GetCurrentPatternAs(UIA_TextPatternId).ok();

        let mut info = FieldInfo {
            pid,
            control_type,
            password,
            editable: editable(control_type, read_only, text_pattern.is_some()),
            before_caret: None,
            value: None,
        };
        if password {
            // Never read what is in a password box.
            return Some(info);
        }
        if let Some(tp) = &text_pattern {
            info.before_caret = text_before_caret(tp);
        }
        if let Some(vp) = &value_pattern {
            if let Ok(value) = vp.CurrentValue() {
                let value = value.to_string();
                if value.chars().count() <= MAX_VALUE_CHARS {
                    info.value = Some(value);
                }
            }
        }
        Some(info)
    }
}

unsafe fn text_before_caret(tp: &IUIAutomationTextPattern) -> Option<String> {
    // SAFETY: COM calls on interfaces owned by this thread.
    unsafe {
        let selection = tp.GetSelection().ok()?;
        if selection.Length().ok()? < 1 {
            return None;
        }
        let caret = selection.GetElement(0).ok()?;
        let range = caret.Clone().ok()?;
        // Collapse to the start of the selection (the caret when nothing is selected), then
        // reach back.
        range.MoveEndpointByRange(TextPatternRangeEndpoint_End, &caret, TextPatternRangeEndpoint_Start).ok()?;
        range.MoveEndpointByUnit(TextPatternRangeEndpoint_Start, TextUnit_Character, -(CONTEXT_CHARS as i32)).ok()?;
        let text = range.GetText(CONTEXT_CHARS as i32 * 2).ok()?.to_string();
        // Some providers return the whole document when the range is degenerate; keep the tail.
        let count = text.chars().count();
        Some(if count > CONTEXT_CHARS { text.chars().skip(count - CONTEXT_CHARS).collect() } else { text })
    }
}

/// Collapse whitespace so line-ending and spacing differences do not matter when comparing.
fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether text arrived, judged from the field before and after inserting it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The inserted text is now before the cursor (or in the value).
    Arrived,
    /// The field is exactly as it was: nothing went in.
    Unchanged,
    /// It changed, but not recognisably (autocorrect, formatting), or it cannot be read.
    Unknown,
}

pub fn verdict(before: Option<&FieldInfo>, after: Option<&FieldInfo>, inserted: &str) -> Verdict {
    let (Some(before), Some(after)) = (before, after) else { return Verdict::Unknown };
    let needle = squash(inserted);
    if needle.is_empty() {
        return Verdict::Unknown;
    }
    // The end of what was inserted: enough to recognise, short enough to fit the context window.
    let tail: String = {
        let count = needle.chars().count();
        needle.chars().skip(count.saturating_sub(40)).collect()
    };
    if let (Some(b), Some(a)) = (&before.before_caret, &after.before_caret) {
        if squash(a).ends_with(&tail) && squash(b) != squash(a) {
            return Verdict::Arrived;
        }
        // An empty field that stays empty proves nothing: some editors (Google Docs) take input
        // through a hidden box they clear at once.
        let had_content = !b.is_empty() || before.value.as_deref().is_some_and(|v| !v.is_empty());
        if a == b && before.value == after.value && had_content {
            return Verdict::Unchanged;
        }
        return Verdict::Unknown;
    }
    if let (Some(b), Some(a)) = (&before.value, &after.value) {
        let (b, a) = (squash(b), squash(a));
        if a.matches(&tail).count() > b.matches(&tail).count() {
            return Verdict::Arrived;
        }
        if a == b && !b.is_empty() {
            return Verdict::Unchanged;
        }
    }
    Verdict::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(before: Option<&str>, value: Option<&str>) -> FieldInfo {
        FieldInfo {
            before_caret: before.map(String::from),
            value: value.map(String::from),
            editable: Some(true),
            ..Default::default()
        }
    }

    #[test]
    fn editable_decisions() {
        assert_eq!(editable(UIA_EditControlTypeId.0, None, true), Some(true));
        assert_eq!(editable(UIA_EditControlTypeId.0, Some(true), true), Some(false));
        assert_eq!(editable(UIA_ButtonControlTypeId.0, None, false), Some(false));
        assert_eq!(editable(UIA_ListItemControlTypeId.0, None, false), Some(false));
        // A writable value wins over the type (Excel cells, custom editors).
        assert_eq!(editable(UIA_ListItemControlTypeId.0, Some(false), false), Some(true));
        assert_eq!(editable(UIA_DocumentControlTypeId.0, Some(true), true), Some(false));
        // Unknown custom controls: let it through.
        assert_eq!(editable(50025, None, false), None);
    }

    #[test]
    fn verdicts_from_the_caret() {
        let before = field(Some("Hello"), None);
        assert_eq!(verdict(Some(&before), Some(&field(Some("Hello world"), None)), " world"), Verdict::Arrived);
        assert_eq!(verdict(Some(&before), Some(&field(Some("Hello"), None)), " world"), Verdict::Unchanged);
        // Autocorrected: changed, but not into our text.
        assert_eq!(verdict(Some(&before), Some(&field(Some("Hello wrld"), None)), " world"), Verdict::Unknown);
        assert_eq!(verdict(None, Some(&before), "x"), Verdict::Unknown);
    }

    #[test]
    fn verdicts_from_the_value() {
        let before = field(None, Some("note: "));
        assert_eq!(verdict(Some(&before), Some(&field(None, Some("note: buy milk"))), "buy milk"), Verdict::Arrived);
        assert_eq!(verdict(Some(&before), Some(&field(None, Some("note: "))), "buy milk"), Verdict::Unchanged);
        // Already there once: a second copy must appear.
        let before = field(None, Some("buy milk"));
        assert_eq!(verdict(Some(&before), Some(&field(None, Some("buy milk"))), "buy milk"), Verdict::Unchanged);
    }

    #[test]
    fn empty_fields_that_stay_empty_prove_nothing() {
        let empty = field(Some(""), Some(""));
        assert_eq!(verdict(Some(&empty), Some(&empty), "hello"), Verdict::Unknown);
        let empty = field(None, Some(""));
        assert_eq!(verdict(Some(&empty), Some(&empty), "hello"), Verdict::Unknown);
    }

    #[test]
    fn long_text_is_matched_by_its_end() {
        let text = "word ".repeat(60);
        let before = field(Some(""), None);
        let after_text: String = text.chars().skip(text.len() - CONTEXT_CHARS).collect();
        assert_eq!(verdict(Some(&before), Some(&field(Some(&after_text), None)), &text), Verdict::Arrived);
    }
}
