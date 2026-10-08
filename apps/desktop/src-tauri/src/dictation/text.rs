//! Turning a raw transcript into the text that gets typed: whisper's non-speech markers and
//! stock hallucinations out, the user's fillers, commands and replacements applied, and the
//! spacing and capital letter fitted to the text already before the cursor.

use super::settings::{DictationSettings, Replacement};

/// What whisper writes for audio with no words in it. On short or quiet clips these come out
/// instead of nothing; typing them into someone's email would be worse than typing nothing.
const HALLUCINATIONS: &[&str] = &[
    "thank you",
    "thank you very much",
    "thanks",
    "thanks for watching",
    "thank you for watching",
    "thanks for listening",
    "please subscribe",
    "subscribe",
    "you",
    "bye",
    "bye bye",
    "okay",
    "the end",
    "subtitles by the amara org community",
    "transcription by castingwords",
];

/// Words in brackets or parentheses that describe sound rather than speech.
const SOUND_WORDS: &[&str] = &[
    "music", "silence", "applause", "laugh", "noise", "inaudible", "blank", "sigh", "cough", "breath", "clap",
    "beep", "sound", "static", "background",
];

const FILLERS: &[&str] = &["um", "umm", "uh", "uhh", "uhm", "erm", "er", "ah", "hmm", "mm", "mmm"];

/// Options for [`finish`], from the settings.
pub struct Options<'a> {
    pub remove_fillers: bool,
    pub voice_commands: bool,
    pub smart_spacing: bool,
    pub replacements: &'a [Replacement],
}

impl<'a> Options<'a> {
    pub fn from_settings(s: &'a DictationSettings) -> Self {
        Self {
            remove_fillers: s.remove_fillers,
            voice_commands: s.voice_commands,
            smart_spacing: s.smart_spacing,
            replacements: &s.replacements,
        }
    }
}

/// The text to insert, or `None` when nothing was said. `before_caret` is the text just before
/// the cursor in the target field when it could be read (`Some("")` for an empty field).
/// `speech_ms` is how much speech the audio held; very short clips are where whisper invents.
pub fn finish(raw: &str, options: &Options, before_caret: Option<&str>, speech_ms: u64) -> Option<String> {
    let mut text = strip_non_speech(raw);
    if is_hallucination(&text, speech_ms) {
        return None;
    }
    if options.remove_fillers {
        text = remove_fillers(&text);
    }
    if options.voice_commands {
        text = apply_voice_commands(&text);
    }
    text = apply_replacements(&text, options.replacements);
    text = tidy(&text);
    if text.trim().is_empty() {
        return None;
    }
    if options.smart_spacing {
        if let Some(before) = before_caret {
            text = fit_to_context(&text, before);
        }
    }
    Some(text)
}

/// Remove `[BLANK_AUDIO]`, `(upbeat music)`, `♪` and the like.
pub fn strip_non_speech(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '[' | '(' | '*' => {
                let close = match c {
                    '[' => ']',
                    '(' => ')',
                    _ => '*',
                };
                // Look ahead for the closing mark; only a short span counts as an annotation.
                let rest: String = chars.clone().take(60).collect();
                if let Some(end) = rest.find(close) {
                    let inner = &rest[..end];
                    let lower = inner.to_lowercase();
                    // Brackets are always annotations in whisper output; parentheses and
                    // asterisks only when they describe a sound.
                    if c == '[' || SOUND_WORDS.iter().any(|w| lower.contains(w)) {
                        for _ in 0..inner.chars().count() + 1 {
                            chars.next();
                        }
                        continue;
                    }
                }
                out.push(c);
            }
            '♪' | '♫' | '🎵' | '🎶' => {}
            _ => out.push(c),
        }
    }
    tidy(&out)
}

/// Lowercase words only, for comparing against [`HALLUCINATIONS`].
fn words_only(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_alphanumeric() || c == '\'' { c.to_ascii_lowercase() } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether the whole transcript is one of whisper's stock phrases for silence. Only for short
/// speech: someone who really dictated "Thank you." for three seconds gets it.
pub fn is_hallucination(text: &str, speech_ms: u64) -> bool {
    let words = words_only(text);
    if words.is_empty() {
        return true;
    }
    speech_ms < 1_500 && HALLUCINATIONS.contains(&words.as_str())
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '\''
}

/// Remove "um", "uh" and friends as whole words, with a comma that only belonged to them.
pub fn remove_fillers(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut sentence_start = true;
    let mut capitalize_next = false;
    let mut rest = text;
    while !rest.is_empty() {
        let word_len = rest.find(|c: char| !is_word_char(c)).unwrap_or(rest.len());
        if word_len == 0 {
            let c = rest.chars().next().expect("non-empty");
            out.push(c);
            if matches!(c, '.' | '!' | '?' | '\n') {
                sentence_start = true;
            } else if !c.is_whitespace() {
                sentence_start = false;
            }
            rest = &rest[c.len_utf8()..];
            continue;
        }
        let word = &rest[..word_len];
        rest = &rest[word_len..];
        if FILLERS.contains(&word.to_lowercase().as_str()) {
            // Drop a comma (and the space) that followed the filler: "Um, so" -> "so". A filler
            // that was a whole sentence takes its full stop too: "Hmm. Okay" -> "Okay".
            let trimmed = match rest.strip_prefix(',') {
                Some(after) => after,
                None if sentence_start => rest.strip_prefix(['.', '!', '?']).unwrap_or(rest),
                None => rest,
            };
            rest = trimmed.strip_prefix(' ').unwrap_or(trimmed);
            if sentence_start {
                capitalize_next = true;
            }
            continue;
        }
        if capitalize_next {
            out.push_str(&capitalize_first(word));
            capitalize_next = false;
        } else {
            out.push_str(word);
        }
        sentence_start = false;
    }
    tidy(&out)
}

/// "new line" and "new paragraph" (English) become line breaks, taking the punctuation whisper
/// put around the command with them.
pub fn apply_voice_commands(text: &str) -> String {
    const COMMANDS: &[(&str, &str)] = &[("new paragraph", "\n\n"), ("new line", "\n"), ("newline", "\n")];
    let lower = text.to_lowercase();
    // Lowercasing can change byte lengths outside ASCII; then just leave the text alone.
    if lower.len() != text.len() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let bytes = text.as_bytes();
    'scan: while i < text.len() {
        for (phrase, replacement) in COMMANDS {
            if lower[i..].starts_with(phrase)
                && (i == 0 || !is_word_char(text[..i].chars().next_back().unwrap_or(' ')))
                && !text[i + phrase.len()..].chars().next().is_some_and(is_word_char)
            {
                // The command replaces the sentence break: drop spaces and punctuation before it
                // that are not part of a word ("Hello, new line" -> "Hello\n").
                while out.ends_with([' ', ',']) {
                    out.pop();
                }
                out.push_str(replacement);
                i += phrase.len();
                // And the punctuation and spaces after it ("new line. World" -> "\nWorld").
                while i < text.len() && matches!(bytes[i], b'.' | b',' | b'!' | b'?' | b';' | b':' | b' ') {
                    i += 1;
                }
                // The next word starts a line: capitalize it.
                if let Some(next) = text[i..].chars().next() {
                    if next.is_lowercase() {
                        let upper: String = next.to_uppercase().collect();
                        out.push_str(&upper);
                        i += next.len_utf8();
                    }
                }
                continue 'scan;
            }
        }
        let c = text[i..].chars().next().expect("in bounds");
        out.push(c);
        i += c.len_utf8();
    }
    out
}

/// Whole-word, case-insensitive replacements, longest first so "Talk R" beats "Talk".
pub fn apply_replacements(text: &str, replacements: &[Replacement]) -> String {
    let mut sorted: Vec<&Replacement> = replacements.iter().filter(|r| !r.from.trim().is_empty()).collect();
    sorted.sort_by_key(|r| std::cmp::Reverse(r.from.len()));
    let mut text = text.to_string();
    for r in sorted {
        text = replace_words(&text, r.from.trim(), &r.to);
    }
    text
}

fn replace_words(text: &str, from: &str, to: &str) -> String {
    let lower = text.to_lowercase();
    let needle = from.to_lowercase();
    if lower.len() != text.len() || needle.is_empty() {
        // Non-ASCII case folding changed lengths: fall back to an exact match.
        return text.replace(from, to);
    }
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    let mut search = 0;
    while let Some(pos) = lower[search..].find(&needle) {
        let start = search + pos;
        let end = start + needle.len();
        let before_ok = start == 0 || !text[..start].chars().next_back().is_some_and(is_word_char);
        let after_ok = !text[end..].chars().next().is_some_and(is_word_char);
        if before_ok && after_ok {
            out.push_str(&text[last..start]);
            out.push_str(to);
            last = end;
        }
        search = end;
        if search >= text.len() {
            break;
        }
    }
    out.push_str(&text[last..]);
    out
}

/// Collapse runs of spaces, remove spaces before punctuation, trim. Line breaks are kept.
pub fn tidy(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c == ' ' || c == '\t' {
            if !out.is_empty() && !out.ends_with([' ', '\n']) {
                out.push(' ');
            }
            continue;
        }
        if matches!(c, ',' | '.' | '!' | '?' | ';' | ':') && out.ends_with(' ') {
            out.pop();
        }
        if c == '\n' && out.ends_with(' ') {
            out.pop();
        }
        out.push(c);
    }
    // A lone comma or period left at the start by removed words.
    let trimmed = out.trim().trim_start_matches([',', ';']).trim_start();
    trimmed.to_string()
}

fn capitalize_first(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Words that keep their capital letter mid-sentence.
fn always_capital(word: &str) -> bool {
    matches!(word, "I" | "I'm" | "I'll" | "I've" | "I'd")
}

/// Fit `text` to what is already before the cursor: a space after a word, no space after an
/// opening bracket, a capital letter where a sentence starts and a small one mid-sentence.
pub fn fit_to_context(text: &str, before: &str) -> String {
    let Some(prev) = before.chars().next_back() else {
        // Empty field: a new sentence.
        return capitalize_first(text);
    };
    let last_visible = before.trim_end_matches([' ', '\u{a0}', '\t']).chars().next_back();
    let sentence_start = match last_visible {
        None => true,
        Some(c) => matches!(c, '.' | '!' | '?' | '\n' | '\r' | '…' | ':') || before.ends_with('\n'),
    };
    let mid_sentence = last_visible.is_some_and(|c| c.is_alphanumeric() || matches!(c, ',' | ';'));

    let mut body = text.to_string();
    if sentence_start {
        body = capitalize_first(&body);
    } else if mid_sentence {
        let first_word: String = body.chars().take_while(|c| is_word_char(*c)).collect();
        let rest_lower = first_word.chars().skip(1).all(|c| !c.is_uppercase());
        if first_word.chars().count() > 1 && rest_lower && !always_capital(&first_word) {
            let mut chars = body.chars();
            if let Some(first) = chars.next() {
                body = first.to_lowercase().chain(chars).collect();
            }
        }
    }

    let starts_with_punct = body.starts_with([',', '.', '!', '?', ';', ':', ')', ']', '}', '\n']);
    let needs_space = !prev.is_whitespace() && !matches!(prev, '(' | '[' | '{' | '"' | '\'' | '“' | '‘' | '/' | '-' | '@' | '#');
    if needs_space && !starts_with_punct {
        body.insert(0, ' ');
    }
    body
}

/// A short single line for the pill: the first words of what was typed.
pub fn preview(text: &str, max_chars: usize) -> String {
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max_chars {
        return flat;
    }
    let cut: String = flat.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{}…", cut.trim_end())
}

/// For terminals: a line break would run the command, so lines are joined with spaces.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn single_line(text: &str) -> String {
    text.split(['\n', '\r']).map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> Options<'static> {
        Options { remove_fillers: true, voice_commands: true, smart_spacing: true, replacements: &[] }
    }

    #[test]
    fn non_speech_markers_are_removed() {
        assert_eq!(strip_non_speech("[BLANK_AUDIO]"), "");
        assert_eq!(strip_non_speech("Hello [Music] world"), "Hello world");
        assert_eq!(strip_non_speech("(upbeat music) Let's go ♪"), "Let's go");
        assert_eq!(strip_non_speech("*sighs* fine"), "fine");
        // Real parentheses stay.
        assert_eq!(strip_non_speech("Call me (at noon) please"), "Call me (at noon) please");
    }

    #[test]
    fn stock_phrases_on_short_clips_are_dropped() {
        assert!(is_hallucination("Thank you.", 600));
        assert!(is_hallucination(" Thanks for watching! ", 900));
        assert!(is_hallucination("", 5_000));
        assert!(!is_hallucination("Thank you.", 2_500), "really said");
        assert!(!is_hallucination("Thank you for the update.", 600));
        assert_eq!(finish("[BLANK_AUDIO]", &opts(), None, 400), None);
        assert_eq!(finish("You", &opts(), None, 300), None);
    }

    #[test]
    fn fillers_go_with_their_commas() {
        assert_eq!(remove_fillers("Um, so I think uh we should go."), "So I think we should go.");
        assert_eq!(remove_fillers("It was, uh, fine."), "It was, fine.");
        assert_eq!(remove_fillers("Hmm. Okay then."), "Okay then.");
        // Not inside words.
        assert_eq!(remove_fillers("The umbrella is uhm here"), "The umbrella is here");
    }

    #[test]
    fn voice_commands_make_line_breaks() {
        assert_eq!(apply_voice_commands("Hello. New line. World."), "Hello.\nWorld.");
        assert_eq!(apply_voice_commands("Dear Sam, new paragraph thanks for coming"), "Dear Sam\n\nThanks for coming");
        assert_eq!(apply_voice_commands("Add a newline here"), "Add a\nHere");
        // Inside other words: untouched.
        assert_eq!(apply_voice_commands("renew lines"), "renew lines");
    }

    #[test]
    fn replacements_are_whole_word_and_case_insensitive() {
        let r = vec![
            Replacement { from: "talker".into(), to: "Talkr".into() },
            Replacement { from: "talker app".into(), to: "Talkr app".into() },
        ];
        assert_eq!(apply_replacements("I use Talker app and talker.", &r), "I use Talkr app and Talkr.");
        assert_eq!(apply_replacements("talkers", &r), "talkers");
    }

    #[test]
    fn tidy_fixes_spacing() {
        assert_eq!(tidy("  Hello ,  world  . "), "Hello, world.");
        assert_eq!(tidy(", so then"), "so then");
        assert_eq!(tidy("a \nb"), "a\nb");
    }

    #[test]
    fn text_fits_the_cursor() {
        // After a word: space, and lowercase mid-sentence.
        assert_eq!(fit_to_context("And then we left.", "We ate"), " and then we left.");
        // After a sentence: space and capital.
        assert_eq!(fit_to_context("then we left.", "We ate."), " Then we left.");
        assert_eq!(fit_to_context("then", "We ate. "), "Then");
        // Empty field.
        assert_eq!(fit_to_context("hello there", ""), "Hello there");
        // After an opening bracket or a space: no extra space.
        assert_eq!(fit_to_context("Note", "("), "Note");
        assert_eq!(fit_to_context("Note", "see "), "note");
        // "I" keeps its capital, and so do acronyms.
        assert_eq!(fit_to_context("I think so", "well"), " I think so");
        assert_eq!(fit_to_context("NASA did it", "and"), " NASA did it");
        // Punctuation attaches.
        assert_eq!(fit_to_context(", right?", "ok"), ", right?");
    }

    #[test]
    fn finish_runs_everything() {
        let r = vec![Replacement { from: "talker".into(), to: "Talkr".into() }];
        let o = Options { replacements: &r, ..opts() };
        assert_eq!(
            finish(" Um, I love talker. New line. Thanks [Music]", &o, Some("Note:"), 4_000).as_deref(),
            Some(" I love Talkr.\nThanks")
        );
        // Without context, no spacing changes.
        assert_eq!(finish("hello world", &opts(), None, 2_000).as_deref(), Some("hello world"));
        // Only fillers: nothing to type.
        assert_eq!(finish("Um, uh.", &opts(), None, 2_000), None);
    }

    #[test]
    fn previews_and_single_lines() {
        assert_eq!(preview("short", 10), "short");
        assert_eq!(preview("a much longer sentence here", 10), "a much lo…");
        assert_eq!(single_line("ls -la\n\ncd ..\r\n"), "ls -la cd ..");
    }
}
