//! What Talkr knows about the focused field, read through the Accessibility API (`ax`): whether
//! it takes text, whether it is a password box, and the text before the cursor. The decisions
//! made from it (may text go in, did it arrive) are pure functions, tested here.

/// Characters read before the cursor: enough for spacing decisions and to recognise inserted text.
pub const CONTEXT_CHARS: usize = 96;
/// A field's whole value is read only up to this size (to check insertion where the cursor
/// position is not exposed).
pub const MAX_VALUE_CHARS: usize = 20_000;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FieldInfo {
    /// The process that owns the field (0 when unknown).
    pub pid: i32,
    /// The Accessibility role ("AXTextArea") and subrole ("AXSecureTextField").
    pub role: String,
    pub subrole: String,
    pub password: bool,
    /// `Some(false)`: certainly not a place for text (a button, a list, a label).
    /// `None`: cannot tell, which is common (web views, Electron apps) and fine.
    pub editable: Option<bool>,
    /// Text just before the cursor, when the field exposes it (`Some("")` at the start).
    pub before_caret: Option<String>,
    /// The field's whole value, when it exposes one and it is small.
    pub value: Option<String>,
}

/// Roles that hold text the user types.
const TEXT_ROLES: &[&str] = &["AXTextField", "AXTextArea", "AXComboBox", "AXSearchField"];

/// Roles that never take typed text.
const NOT_TEXT_ROLES: &[&str] = &[
    "AXButton",
    "AXCheckBox",
    "AXRadioButton",
    "AXLink",
    "AXImage",
    "AXMenu",
    "AXMenuItem",
    "AXMenuBar",
    "AXMenuBarItem",
    "AXMenuButton",
    "AXPopUpButton",
    "AXTabGroup",
    "AXRadioGroup",
    "AXRow",
    "AXCell",
    "AXOutline",
    "AXTable",
    "AXList",
    "AXBrowser",
    "AXScrollBar",
    "AXSlider",
    "AXIncrementor",
    "AXDisclosureTriangle",
    "AXToolbar",
    "AXColorWell",
    "AXStaticText",
    "AXDockItem",
];

/// Whether the focused element takes text. `value_settable`: its AXValue can be written;
/// `has_selection`: it exposes a text cursor (AXSelectedTextRange).
pub fn editable(role: &str, value_settable: Option<bool>, has_selection: bool) -> Option<bool> {
    if TEXT_ROLES.contains(&role) {
        // A text field whose value cannot be set and has no cursor is read-only (a disabled
        // field); otherwise it takes text, though some web fields report neither: let them be.
        return match (value_settable, has_selection) {
            (Some(true), _) | (_, true) => Some(true),
            (Some(false), false) => Some(false),
            (None, false) => None,
        };
    }
    if NOT_TEXT_ROLES.contains(&role) {
        // A writable value wins over the role (custom controls that edit in place).
        return if value_settable == Some(true) && has_selection { Some(true) } else { Some(false) };
    }
    None
}

/// Whether the field is a password box: its text must not be read or kept in history.
pub fn is_password(role: &str, subrole: &str) -> bool {
    subrole == "AXSecureTextField" || role == "AXSecureTextField"
}

/// The text before position `caret` (in UTF-16 units, as the Accessibility API counts) of a
/// field's value, at most [`CONTEXT_CHARS`] characters.
pub fn before_caret(value: &[u16], caret: usize) -> String {
    let end = caret.min(value.len());
    // UTF-16 units: twice the characters is enough even if every one is a surrogate pair.
    let start = end.saturating_sub(CONTEXT_CHARS * 2);
    let text = String::from_utf16_lossy(&value[start..end]);
    // A cut (or a caret reported mid-character) may have split a surrogate pair.
    let text = text.trim_matches('\u{FFFD}');
    tail(text, CONTEXT_CHARS)
}

/// The last `n` characters of `text`.
pub fn tail(text: &str, n: usize) -> String {
    let count = text.chars().count();
    text.chars().skip(count.saturating_sub(n)).collect()
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
    let tail = tail(&needle, 40);
    if let (Some(b), Some(a)) = (&before.before_caret, &after.before_caret) {
        if squash(a).ends_with(&tail) && squash(b) != squash(a) {
            return Verdict::Arrived;
        }
        // An empty field that stays empty proves nothing: some editors take input through a
        // hidden box they clear at once.
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

    fn utf16(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    #[test]
    fn editable_decisions() {
        assert_eq!(editable("AXTextArea", Some(true), true), Some(true));
        assert_eq!(editable("AXTextField", None, true), Some(true));
        assert_eq!(editable("AXTextField", Some(false), false), Some(false), "a disabled field");
        assert_eq!(editable("AXTextField", None, false), None);
        assert_eq!(editable("AXButton", None, false), Some(false));
        assert_eq!(editable("AXStaticText", Some(false), true), Some(false), "selectable label");
        assert_eq!(editable("AXCell", Some(true), true), Some(true), "a cell edited in place");
        assert_eq!(editable("AXList", None, false), Some(false));
        // Web views, Electron apps and custom views: let it through.
        assert_eq!(editable("AXWebArea", None, false), None);
        assert_eq!(editable("AXGroup", None, false), None);
        assert_eq!(editable("", None, false), None);
    }

    #[test]
    fn passwords() {
        assert!(is_password("AXTextField", "AXSecureTextField"));
        assert!(!is_password("AXTextField", "AXSearchField"));
    }

    #[test]
    fn text_before_the_caret() {
        assert_eq!(before_caret(&utf16("Hello world"), 5), "Hello");
        assert_eq!(before_caret(&utf16("Hello"), 0), "");
        assert_eq!(before_caret(&utf16("Hi"), 40), "Hi", "a caret past the end is clamped");
        let long = "x".repeat(500) + "end";
        assert_eq!(before_caret(&utf16(&long), 503).chars().count(), CONTEXT_CHARS);
        assert!(before_caret(&utf16(&long), 503).ends_with("end"));
        // Emoji are two UTF-16 units each; a cut through one is dropped, not garbled.
        let emoji = "😀".repeat(200);
        let text = before_caret(&utf16(&emoji), utf16(&emoji).len());
        assert_eq!(text.chars().count(), CONTEXT_CHARS);
        assert!(text.chars().all(|c| c == '😀'));
        let odd = before_caret(&utf16(&emoji), 2 * CONTEXT_CHARS + 1);
        assert!(!odd.contains('\u{FFFD}'));
    }

    #[test]
    fn verdicts_from_the_caret() {
        let before = field(Some("Hello"), None);
        assert_eq!(verdict(Some(&before), Some(&field(Some("Hello world"), None)), " world"), Verdict::Arrived);
        assert_eq!(verdict(Some(&before), Some(&field(Some("Hello"), None)), " world"), Verdict::Unchanged);
        assert_eq!(verdict(Some(&before), Some(&field(Some("Hello wrld"), None)), " world"), Verdict::Unknown);
        assert_eq!(verdict(None, Some(&before), "x"), Verdict::Unknown);
        assert_eq!(verdict(Some(&before), None, "x"), Verdict::Unknown);
    }

    #[test]
    fn verdicts_from_the_value() {
        let before = field(None, Some("note: "));
        assert_eq!(verdict(Some(&before), Some(&field(None, Some("note: buy milk"))), "buy milk"), Verdict::Arrived);
        assert_eq!(verdict(Some(&before), Some(&field(None, Some("note: "))), "buy milk"), Verdict::Unchanged);
        let before = field(None, Some("buy milk"));
        assert_eq!(verdict(Some(&before), Some(&field(None, Some("buy milk"))), "buy milk"), Verdict::Unchanged);
        assert_eq!(
            verdict(Some(&before), Some(&field(None, Some("buy milk buy milk"))), "buy milk"),
            Verdict::Arrived,
            "a second copy appeared"
        );
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
        let after_text = tail(&text, CONTEXT_CHARS);
        assert_eq!(verdict(Some(&before), Some(&field(Some(&after_text), None)), &text), Verdict::Arrived);
    }

    #[test]
    fn line_breaks_compare_as_spaces() {
        let before = field(Some("Dear Sam,"), None);
        let after = field(Some("Dear Sam,\nThanks for the notes."), None);
        assert_eq!(verdict(Some(&before), Some(&after), "\nThanks for the notes."), Verdict::Arrived);
    }
}
