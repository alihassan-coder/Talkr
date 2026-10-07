//! Dictation's part of `config.json`: the shortcut, how text is inserted, and the user's words.
//! Kept as one nested object so the frontend saves it whole and the flat settings stay readable.

use serde::{Deserialize, Serialize};

/// Windows virtual-key codes the defaults use.
const VK_V: u16 = 0x56;

/// A global keyboard shortcut: modifiers plus at most one other key. A shortcut of modifiers
/// alone (Ctrl + Win, the default) is pressed when the last of them goes down.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Shortcut {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub win: bool,
    /// Windows virtual-key code of the non-modifier key, if any.
    pub key: Option<u16>,
    /// How to show `key` ("Space", "V"), read from the keyboard layout when it was recorded.
    pub key_label: Option<String>,
}

impl Default for Shortcut {
    fn default() -> Self {
        Self::ctrl_win()
    }
}

/// Keys that work alone as a shortcut: nothing types them by accident.
fn is_standalone_key(vk: u16) -> bool {
    // F1-F24, Pause, Scroll Lock, Insert.
    (0x70..=0x87).contains(&vk) || matches!(vk, 0x13 | 0x91 | 0x2D)
}

/// Keys a shortcut must not use: modifiers themselves, Escape (it cancels dictation), and keys
/// Windows reserves.
fn is_reserved_key(vk: u16) -> bool {
    matches!(
        vk,
        0x10 | 0x11 | 0x12 // Shift, Ctrl, Alt
            | 0xA0..=0xA5 // left/right Shift, Ctrl, Alt
            | 0x5B | 0x5C // Windows keys
            | 0x1B // Escape
            | 0x2C // Print Screen
            | 0x14 | 0x90 // Caps Lock, Num Lock
            | 0x01..=0x06 // mouse buttons
    )
}

impl Shortcut {
    pub fn ctrl_win() -> Self {
        Self { ctrl: true, shift: false, alt: false, win: true, key: None, key_label: None }
    }

    pub fn alt_shift_v() -> Self {
        Self { ctrl: false, shift: true, alt: true, win: false, key: Some(VK_V), key_label: Some("V".into()) }
    }

    pub fn modifier_count(&self) -> usize {
        [self.ctrl, self.shift, self.alt, self.win].iter().filter(|m| **m).count()
    }

    /// Same keys, whatever the labels say.
    pub fn same_keys(&self, other: &Shortcut) -> bool {
        (self.ctrl, self.shift, self.alt, self.win, self.key) == (other.ctrl, other.shift, other.alt, other.win, other.key)
    }

    /// Why this cannot be a global shortcut, if it cannot. Single keys would fire while typing,
    /// so a shortcut needs two modifiers, or a modifier and a key, or a key nobody types.
    pub fn problem(&self) -> Option<String> {
        match self.key {
            Some(vk) if is_reserved_key(vk) => Some("uses a key that cannot be part of a shortcut".into()),
            Some(vk) if self.modifier_count() == 0 && !is_standalone_key(vk) => {
                Some("needs Ctrl, Alt, Shift or Win with that key (F-keys, Pause, Scroll Lock and Insert work alone)".into())
            }
            None if self.modifier_count() < 2 => Some("needs at least two modifier keys, or a modifier and a key".into()),
            _ if self.key_label.as_deref().is_some_and(|l| l.len() > 32 || l.chars().any(char::is_control)) => {
                Some("has an invalid key label".into())
            }
            _ => None,
        }
    }

    /// "Ctrl + Win", "Alt + Shift + V".
    pub fn describe(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        for (on, name) in [(self.ctrl, "Ctrl"), (self.alt, "Alt"), (self.shift, "Shift"), (self.win, "Win")] {
            if on {
                parts.push(name.into());
            }
        }
        if let Some(vk) = self.key {
            parts.push(self.key_label.clone().unwrap_or_else(|| format!("Key {:#04x}", vk)));
        }
        parts.join(" + ")
    }
}

/// How the shortcut starts and stops dictation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActivationMode {
    /// Hold to talk; a quick tap starts hands-free dictation, and the next press ends it.
    #[default]
    Auto,
    /// Only while the shortcut is held.
    Hold,
    /// Press to start, press again to stop.
    Toggle,
}

/// How text gets into the focused field.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InsertMethod {
    /// Pick per field: write straight into classic edit boxes, paste elsewhere, type into
    /// remote desktops and other apps where pasting is unreliable.
    #[default]
    Auto,
    Paste,
    Type,
}

/// Per-app override of [`InsertMethod`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppMethod {
    Auto,
    Paste,
    Type,
    /// Never insert into this app; copy instead.
    Off,
}

/// What to do when the focused window changed between pressing and releasing the shortcut.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FocusPolicy {
    /// Bring back the window you started in and insert there.
    #[default]
    Original,
    /// Insert wherever the cursor is now.
    Current,
    /// Insert nowhere; copy the text to the clipboard.
    Copy,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OverlayPosition {
    #[default]
    Bottom,
    Top,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Replacement {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppRule {
    /// Executable name, lowercase ("slack.exe").
    pub app: String,
    pub method: AppMethod,
    /// Added by Talkr after pasting failed and typing worked, not by the user.
    #[serde(default)]
    pub learned: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DictationSettings {
    pub enabled: bool,
    pub shortcut: Shortcut,
    pub mode: ActivationMode,
    pub paste_last_enabled: bool,
    pub paste_last_shortcut: Shortcut,
    /// Speech model for dictation; `None` uses the default transcription model.
    pub model: Option<String>,
    /// Spoken language; `None` uses the transcription language setting.
    pub language: Option<String>,
    /// Keep the model loaded while dictation is on, so text appears without a load delay.
    pub keep_warm: bool,
    /// Microphone id (see `audio::record::list_inputs`); `None` is the system default. Used by
    /// every recording in Talkr.
    pub microphone: Option<String>,
    pub insert_method: InsertMethod,
    pub focus_policy: FocusPolicy,
    pub restore_clipboard: bool,
    /// Add a space or capital letter to fit the text around the cursor.
    pub smart_spacing: bool,
    pub remove_fillers: bool,
    /// "new line" and "new paragraph" become line breaks.
    pub voice_commands: bool,
    /// Names and terms whisper should spell this way.
    pub vocabulary: Vec<String>,
    pub replacements: Vec<Replacement>,
    pub app_rules: Vec<AppRule>,
    pub overlay_position: OverlayPosition,
    /// Show which app the text will go to, in the pill.
    pub show_target: bool,
    pub sounds: bool,
    pub save_history: bool,
    pub launch_at_login: bool,
    /// Closing the window keeps Talkr in the tray while dictation is on.
    pub close_to_tray: bool,
}

impl Default for DictationSettings {
    fn default() -> Self {
        Self {
            // Off until the user turns it on: it installs a keyboard hook.
            enabled: false,
            shortcut: Shortcut::ctrl_win(),
            mode: ActivationMode::Auto,
            paste_last_enabled: true,
            paste_last_shortcut: Shortcut::alt_shift_v(),
            model: None,
            language: None,
            keep_warm: true,
            microphone: None,
            insert_method: InsertMethod::Auto,
            focus_policy: FocusPolicy::Original,
            restore_clipboard: true,
            smart_spacing: true,
            remove_fillers: true,
            voice_commands: true,
            vocabulary: Vec::new(),
            replacements: Vec::new(),
            app_rules: Vec::new(),
            overlay_position: OverlayPosition::Bottom,
            show_target: true,
            sounds: true,
            save_history: true,
            launch_at_login: false,
            close_to_tray: true,
        }
    }
}

pub const MAX_VOCABULARY: usize = 200;
pub const MAX_WORD_BYTES: usize = 100;
/// whisper reads at most 224 prompt tokens; past this the oldest words are dropped anyway.
pub const MAX_VOCABULARY_PROMPT_BYTES: usize = 800;
pub const MAX_REPLACEMENTS: usize = 200;
pub const MAX_REPLACEMENT_BYTES: usize = 500;
pub const MAX_APP_RULES: usize = 300;
pub const MAX_APP_BYTES: usize = 128;
const MAX_ID_BYTES: usize = 512;

fn text_problem(value: &str, max: usize) -> Option<String> {
    if value.len() > max {
        Some(format!("is too long (at most {} bytes)", max))
    } else if value.chars().any(|c| c.is_control() && c != '\n') {
        Some("must not contain control characters".into())
    } else {
        None
    }
}

impl DictationSettings {
    /// The first reason these settings cannot be used, worded for the user.
    pub fn problem(&self) -> Option<String> {
        if let Some(p) = self.shortcut.problem() {
            return Some(format!("The dictation shortcut {}", p));
        }
        if self.paste_last_enabled {
            if let Some(p) = self.paste_last_shortcut.problem() {
                return Some(format!("The paste-again shortcut {}", p));
            }
            if self.paste_last_shortcut.same_keys(&self.shortcut) {
                return Some("The dictation and paste-again shortcuts must be different".into());
            }
        }
        for (name, value) in [("model", &self.model), ("language", &self.language), ("microphone", &self.microphone)] {
            if let Some(p) = value.as_deref().and_then(|v| text_problem(v, MAX_ID_BYTES)) {
                return Some(format!("The dictation {} {}", name, p));
            }
        }
        if self.vocabulary.len() > MAX_VOCABULARY {
            return Some(format!("Vocabulary can hold at most {} words", MAX_VOCABULARY));
        }
        if let Some(p) = self.vocabulary.iter().find_map(|w| text_problem(w, MAX_WORD_BYTES)) {
            return Some(format!("A vocabulary word {}", p));
        }
        if self.replacements.len() > MAX_REPLACEMENTS {
            return Some(format!("There can be at most {} replacements", MAX_REPLACEMENTS));
        }
        for r in &self.replacements {
            if r.from.trim().is_empty() {
                return Some("A replacement needs the words to replace".into());
            }
            if let Some(p) = text_problem(&r.from, MAX_WORD_BYTES).or_else(|| text_problem(&r.to, MAX_REPLACEMENT_BYTES)) {
                return Some(format!("A replacement {}", p));
            }
        }
        if self.app_rules.len() > MAX_APP_RULES {
            return Some(format!("There can be at most {} app rules", MAX_APP_RULES));
        }
        for rule in &self.app_rules {
            if rule.app.trim().is_empty() {
                return Some("An app rule needs an app".into());
            }
            if let Some(p) = text_problem(&rule.app, MAX_APP_BYTES) {
                return Some(format!("An app rule {}", p));
            }
        }
        None
    }

    /// Repair values a hand-edited file got wrong, field by field, so one bad entry does not
    /// reset the rest. Returns what was changed, for the log.
    pub fn sanitize(&mut self) -> Vec<String> {
        let mut notices = Vec::new();
        let defaults = Self::default();
        if self.shortcut.problem().is_some() {
            notices.push(format!("dictation shortcut is not usable; using {}", defaults.shortcut.describe()));
            self.shortcut = defaults.shortcut.clone();
        }
        if self.paste_last_shortcut.problem().is_some() || self.paste_last_shortcut.same_keys(&self.shortcut) {
            notices.push("paste-again shortcut is not usable; turning it off".into());
            self.paste_last_shortcut = defaults.paste_last_shortcut.clone();
            self.paste_last_enabled = false;
        }
        for (name, value) in [("model", &mut self.model), ("language", &mut self.language), ("microphone", &mut self.microphone)] {
            if value.as_deref().is_some_and(|v| v.trim().is_empty() || text_problem(v, MAX_ID_BYTES).is_some()) {
                notices.push(format!("dictation {} is not valid; clearing it", name));
                *value = None;
            }
        }
        let before = (self.vocabulary.len(), self.replacements.len(), self.app_rules.len());
        self.vocabulary.retain(|w| !w.trim().is_empty() && text_problem(w, MAX_WORD_BYTES).is_none());
        self.vocabulary.truncate(MAX_VOCABULARY);
        self.replacements.retain(|r| {
            !r.from.trim().is_empty()
                && text_problem(&r.from, MAX_WORD_BYTES).is_none()
                && text_problem(&r.to, MAX_REPLACEMENT_BYTES).is_none()
        });
        self.replacements.truncate(MAX_REPLACEMENTS);
        self.app_rules.retain(|r| !r.app.trim().is_empty() && text_problem(&r.app, MAX_APP_BYTES).is_none());
        self.app_rules.truncate(MAX_APP_RULES);
        if before != (self.vocabulary.len(), self.replacements.len(), self.app_rules.len()) {
            notices.push("dropped invalid dictation words, replacements or app rules".into());
        }
        notices
    }

    /// The rule for an executable, if the user (or Talkr) set one.
    pub fn rule_for(&self, exe: &str) -> Option<&AppRule> {
        self.app_rules.iter().find(|r| r.app.eq_ignore_ascii_case(exe))
    }

    /// The vocabulary as a whisper prompt: comma separated, newest words dropped past the limit.
    pub fn vocabulary_prompt(&self) -> Option<String> {
        let mut prompt = String::new();
        for word in self.vocabulary.iter().map(|w| w.trim()).filter(|w| !w.is_empty()) {
            let extra = if prompt.is_empty() { word.len() } else { word.len() + 2 };
            if prompt.len() + extra > MAX_VOCABULARY_PROMPT_BYTES {
                break;
            }
            if !prompt.is_empty() {
                prompt.push_str(", ");
            }
            prompt.push_str(word);
        }
        (!prompt.is_empty()).then_some(prompt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid_and_off() {
        let d = DictationSettings::default();
        assert!(!d.enabled);
        assert_eq!(d.problem(), None);
        assert_eq!(d.shortcut.describe(), "Ctrl + Win");
        assert_eq!(d.paste_last_shortcut.describe(), "Alt + Shift + V");
    }

    #[test]
    fn shortcut_rules() {
        let key = |ctrl, alt, vk| Shortcut { ctrl, shift: false, alt, win: false, key: Some(vk), key_label: None };
        // One modifier alone fires while typing.
        assert!(Shortcut { win: false, ..Shortcut::ctrl_win() }.problem().is_some());
        // A letter alone too; an F-key does not.
        assert!(key(false, false, 0x41).problem().is_some());
        assert!(key(false, false, 0x78).problem().is_none());
        assert!(key(true, false, 0x20).problem().is_none());
        // Escape cancels dictation; modifiers are not keys.
        assert!(key(true, false, 0x1B).problem().is_some());
        assert!(key(true, false, 0xA2).problem().is_some());
        assert!(Shortcut { ctrl: true, alt: true, ..Default::default() }.problem().is_none());
    }

    #[test]
    fn shortcuts_must_differ() {
        let d = DictationSettings { paste_last_shortcut: Shortcut::ctrl_win(), ..Default::default() };
        assert!(d.problem().unwrap().contains("different"));
        // Not when paste-again is off.
        let d = DictationSettings { paste_last_enabled: false, ..d };
        assert_eq!(d.problem(), None);
    }

    #[test]
    fn limits_are_checked() {
        let d = DictationSettings { vocabulary: vec!["x".into(); MAX_VOCABULARY + 1], ..Default::default() };
        assert!(d.problem().is_some());
        let d = DictationSettings { replacements: vec![Replacement { from: " ".into(), to: "x".into() }], ..Default::default() };
        assert!(d.problem().is_some());
        let d = DictationSettings { vocabulary: vec!["a\u{7}b".into()], ..Default::default() };
        assert!(d.problem().is_some());
    }

    #[test]
    fn sanitize_repairs_field_by_field() {
        let mut d = DictationSettings {
            enabled: true,
            shortcut: Shortcut { win: false, ..Shortcut::ctrl_win() },
            vocabulary: vec!["Talkr".into(), "  ".into(), "x".repeat(MAX_WORD_BYTES + 1)],
            model: Some(String::new()),
            ..Default::default()
        };
        let notices = d.sanitize();
        assert!(notices.len() >= 3, "{notices:?}");
        assert!(d.enabled, "unrelated fields stay");
        assert_eq!(d.shortcut, Shortcut::ctrl_win());
        assert_eq!(d.vocabulary, vec!["Talkr".to_string()]);
        assert_eq!(d.model, None);
        assert_eq!(d.problem(), None);
    }

    #[test]
    fn vocabulary_prompt_is_bounded() {
        let d = DictationSettings { vocabulary: vec!["Talkr".into(), " Kokoro ".into(), "".into()], ..Default::default() };
        assert_eq!(d.vocabulary_prompt().as_deref(), Some("Talkr, Kokoro"));
        let d = DictationSettings { vocabulary: vec!["word".repeat(20); 50], ..Default::default() };
        assert!(d.vocabulary_prompt().unwrap().len() <= MAX_VOCABULARY_PROMPT_BYTES);
        assert_eq!(DictationSettings::default().vocabulary_prompt(), None);
    }

    #[test]
    fn rules_match_case_insensitively() {
        let d = DictationSettings {
            app_rules: vec![AppRule { app: "mstsc.exe".into(), method: AppMethod::Type, learned: false }],
            ..Default::default()
        };
        assert_eq!(d.rule_for("MSTSC.EXE").map(|r| r.method), Some(AppMethod::Type));
        assert!(d.rule_for("slack.exe").is_none());
    }

    #[test]
    fn json_shape_is_camel_case_and_tolerant() {
        let json = serde_json::to_string(&DictationSettings::default()).unwrap();
        assert!(json.contains("\"pasteLastShortcut\"") && json.contains("\"keyLabel\""), "{json}");
        let d: DictationSettings = serde_json::from_str(r#"{"enabled":true,"mode":"toggle"}"#).unwrap();
        assert!(d.enabled);
        assert_eq!(d.mode, ActivationMode::Toggle);
        assert_eq!(d.shortcut, Shortcut::ctrl_win());
    }
}
