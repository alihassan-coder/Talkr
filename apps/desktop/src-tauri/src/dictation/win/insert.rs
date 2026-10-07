//! Getting text into the field the user chose, and making sure it got there.
//!
//! 1. Check: the field is still the one the user started in (or bring it back), it takes text,
//!    and Windows lets us type into it.
//! 2. Insert, the best way for that field: straight into classic edit boxes, by pasting into
//!    most apps, by typing into remote desktops and other apps that do not take pastes.
//! 3. Verify by reading the field back, and only when it is certain nothing went in, try the
//!    next way. Text is never inserted twice.
//! 4. Recover: when it cannot be inserted, it is left on the clipboard and the user is told.

use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{SendMessageTimeoutW, SMTO_ABORTIFHUNG, WM_GETTEXTLENGTH};
use super::keys::{self, LineBreak};
use super::target::{self, Target};
use super::uia::{self, FieldInfo, Verdict};
use super::clipboard;
use crate::dictation::settings::{AppMethod, DictationSettings, FocusPolicy, InsertMethod};

const EM_REPLACESEL: u32 = 0x00C2;
const EM_GETSEL: u32 = 0x00B0;

/// Remote desktops and virtual machines: their clipboard is not this one, but keystrokes pass.
const TYPE_APPS: &[&str] = &[
    "mstsc.exe", "msrdc.exe", "vmconnect.exe", "vmware.exe", "vmplayer.exe", "virtualboxvm.exe", "wfica32.exe",
    "cdviewer.exe", "parsecd.exe", "anydesk.exe", "teamviewer.exe", "rustdesk.exe",
];
/// Terminals where Ctrl + V is a control key, not paste.
const SHIFT_INSERT_APPS: &[&str] = &["mintty.exe", "putty.exe", "kitty.exe"];
/// A line break here runs a command: dictation goes in as one line.
/// Terminal emulators, by executable. Shells (cmd, PowerShell) are not listed: their consoles are
/// recognised by window class below, and a window a PowerShell script opens is no terminal.
const TERMINAL_APPS: &[&str] = &[
    "windowsterminal.exe", "wt.exe", "openconsole.exe", "mintty.exe", "putty.exe", "kitty.exe", "alacritty.exe",
    "wezterm-gui.exe", "tabby.exe", "hyper.exe", "warp.exe", "mobaxterm.exe",
];
const TERMINAL_CLASSES: &[&str] = &["ConsoleWindowClass", "CASCADIA_HOSTING_WINDOW_CLASS", "PseudoConsoleWindow"];
/// Enter sends the message here, so typed line breaks are Shift + Enter.
const CHAT_APPS: &[&str] = &[
    "slack.exe", "teams.exe", "ms-teams.exe", "discord.exe", "whatsapp.exe", "whatsapp.root.exe", "telegram.exe",
    "signal.exe", "element.exe", "zoom.exe", "skype.exe", "messenger.exe",
];
/// Apps that read a paste slowly: the clipboard waits longer before it is restored.
const SLOW_PASTE_APPS: &[&str] = &["winword.exe", "excel.exe", "powerpnt.exe", "outlook.exe", "onenote.exe", "olk.exe"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// EM_REPLACESEL into a classic edit control.
    Direct,
    Paste { shift_insert: bool },
    Type { line_break: LineBreak },
}

impl Method {
    pub fn name(&self) -> &'static str {
        match self {
            Method::Direct => "direct",
            Method::Paste { .. } => "paste",
            Method::Type { .. } => "type",
        }
    }
}

/// Why text was copied instead of inserted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyReason {
    NoTextField,
    FocusChanged,
    TargetClosed,
    Elevated,
    AppOff,
    NotDelivered,
}

impl CopyReason {
    pub fn message(&self) -> &'static str {
        match self {
            CopyReason::NoTextField => "No text field was selected",
            CopyReason::FocusChanged => "The window changed",
            CopyReason::TargetClosed => "That window was closed",
            CopyReason::Elevated => "Windows blocks typing into apps run as administrator",
            CopyReason::AppOff => "Dictation is off for this app",
            CopyReason::NotDelivered => "The app did not accept the text",
        }
    }
}

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
    Ready(Ready),
    Copy(CopyReason),
}

pub enum Outcome {
    Inserted { method: Method, verdict: Verdict, learned_typing: bool },
    Copied(CopyReason),
}

fn is_terminal(t: &Target) -> bool {
    TERMINAL_APPS.contains(&t.exe.as_str()) || TERMINAL_CLASSES.contains(&t.window_class.as_str())
}

/// The insertion method for a target, or why there is none.
pub fn choose(target: &Target, settings: &DictationSettings) -> Result<(Method, bool), CopyReason> {
    let exe = target.exe.as_str();
    let paste = Method::Paste { shift_insert: SHIFT_INSERT_APPS.contains(&exe) };
    let typing =
        Method::Type { line_break: if CHAT_APPS.contains(&exe) { LineBreak::ShiftEnter } else { LineBreak::Enter } };
    match settings.rule_for(exe).map(|r| r.method) {
        Some(AppMethod::Off) => return Err(CopyReason::AppOff),
        Some(AppMethod::Paste) => return Ok((paste, true)),
        Some(AppMethod::Type) => return Ok((typing, true)),
        Some(AppMethod::Auto) | None => {}
    }
    Ok(match settings.insert_method {
        InsertMethod::Paste => (paste, true),
        InsertMethod::Type => (typing, true),
        InsertMethod::Auto if target.is_classic_edit() => (Method::Direct, false),
        InsertMethod::Auto if TYPE_APPS.contains(&exe) => (typing, false),
        InsertMethod::Auto => (paste, false),
    })
}

/// Make sure the right field has focus and decide how to insert. Waits for the user to let go
/// of the shortcut's keys first: pasting with Ctrl and Win still down would be Ctrl + Win + V.
pub fn prepare(original: &Target, settings: &DictationSettings, overlay: isize) -> Prepared {
    if !keys::wait_for_modifiers_released(Duration::from_millis(2_000)) {
        log::warn!("modifier keys still held after 2 s; inserting anyway");
    }

    let mut target = original.clone();
    let foreground = target::foreground();
    if foreground != original.hwnd {
        let ours = foreground == overlay;
        match settings.focus_policy {
            // A click on the pill can take focus: always go back from there.
            FocusPolicy::Original | FocusPolicy::Current if ours => {
                if !target::activate(original) {
                    return Prepared::Copy(CopyReason::FocusChanged);
                }
            }
            FocusPolicy::Original => {
                if !original.still_exists() {
                    return Prepared::Copy(CopyReason::TargetClosed);
                }
                if !target::activate(original) {
                    return Prepared::Copy(CopyReason::FocusChanged);
                }
            }
            FocusPolicy::Current => match target::snapshot() {
                Some(now) => target = now,
                None => return Prepared::Copy(CopyReason::NoTextField),
            },
            FocusPolicy::Copy => return Prepared::Copy(CopyReason::FocusChanged),
        }
    }
    // The user may have clicked another field in the same window: insert where the cursor is.
    if let Some(now) = target::snapshot().filter(|now| now.hwnd == target.hwnd) {
        target.focus_hwnd = now.focus_hwnd;
        target.focus_class = now.focus_class;
    }

    if target.is_shell() {
        return Prepared::Copy(CopyReason::NoTextField);
    }
    if target.elevated {
        return Prepared::Copy(CopyReason::Elevated);
    }
    let terminal = is_terminal(&target);
    let field = uia::inspect(Duration::from_millis(800)).filter(|f| f.pid == 0 || f.pid == target.pid);
    // Terminals expose their screen as read-only text, yet take typing: skip the check there.
    if !terminal && field.as_ref().is_some_and(|f| f.editable == Some(false)) {
        return Prepared::Copy(CopyReason::NoTextField);
    }
    match choose(&target, settings) {
        Ok((method, forced)) => Prepared::Ready(Ready { target, field, method, terminal, forced }),
        Err(reason) => Prepared::Copy(reason),
    }
}

/// Insert `text` (already fitted to the field). `owner` is a Talkr window for clipboard calls.
pub fn deliver(ready: &Ready, text: &str, settings: &DictationSettings, owner: Option<isize>) -> Outcome {
    let owner = owner.map(target::hwnd);
    let mut method = ready.method;

    if method == Method::Direct {
        if direct(target::hwnd(ready.target.focus_hwnd), text) {
            return Outcome::Inserted { method, verdict: Verdict::Arrived, learned_typing: false };
        }
        log::info!("the edit control did not take the text directly; pasting instead");
        method = Method::Paste { shift_insert: false };
    }

    // When it cannot be inserted, the caller copies it (and says whether that worked).
    match run(method, ready, text, settings, owner) {
        Some(Verdict::Unchanged) if matches!(method, Method::Paste { .. }) && can_retry(ready) => {
            // A slow app can take the paste after the check gave up: look once more before
            // typing, or the text could arrive twice.
            std::thread::sleep(Duration::from_millis(350));
            let late = uia::inspect(Duration::from_millis(400));
            let verdict = uia::verdict(ready.field.as_ref(), late.as_ref(), text);
            if verdict != Verdict::Unchanged {
                return Outcome::Inserted { method, verdict, learned_typing: false };
            }
            // Certain the paste did not land: type it instead, and remember that for this app.
            log::info!("paste did not reach {}; typing instead", ready.target.exe);
            let typing = typing_for(&ready.target);
            match run(typing, ready, text, settings, owner) {
                Some(Verdict::Unchanged) | None => Outcome::Copied(CopyReason::NotDelivered),
                Some(v) => Outcome::Inserted { method: typing, verdict: v, learned_typing: v == Verdict::Arrived },
            }
        }
        Some(Verdict::Unchanged) | None => Outcome::Copied(CopyReason::NotDelivered),
        Some(v) => Outcome::Inserted { method, verdict: v, learned_typing: false },
    }
}

fn typing_for(target: &Target) -> Method {
    Method::Type {
        line_break: if CHAT_APPS.contains(&target.exe.as_str()) { LineBreak::ShiftEnter } else { LineBreak::Enter },
    }
}

/// A second attempt is only safe where the field is known to take text and could be read.
fn can_retry(ready: &Ready) -> bool {
    !ready.forced && ready.field.as_ref().is_some_and(|f| f.editable == Some(true))
}

/// Insert once with `method`. `None` when it could not even be attempted.
fn run(method: Method, ready: &Ready, text: &str, settings: &DictationSettings, owner: Option<HWND>) -> Option<Verdict> {
    let text = if ready.terminal { crate::dictation::text::single_line(text) } else { text.to_string() };
    match method {
        Method::Direct => None,
        Method::Paste { shift_insert } => {
            let restore = settings.restore_clipboard;
            let saved = if restore {
                match clipboard::save(owner) {
                    Some(saved) => Some(saved),
                    // Another app holds the clipboard: pasting now would replace what the user
                    // copied with no way to put it back. Type instead.
                    None => return run(typing_for(&ready.target), ready, &text, settings, owner),
                }
            } else {
                None
            };
            let Some(sequence) = clipboard::put_text(&text, owner, restore) else {
                // Emptied but not filled: give the user's content back.
                if let Some(saved) = saved {
                    clipboard::restore(saved, clipboard::sequence(), owner);
                }
                return None;
            };
            let sent = if shift_insert {
                keys::chord(&[keys::VK_SHIFT], keys::VK_INSERT)
            } else {
                keys::chord(&[keys::VK_CONTROL], keys::VK_V)
            };
            if !sent {
                log::warn!("Windows did not accept the paste keystroke");
            }
            let started = Instant::now();
            let verdict = watch(ready.field.as_ref(), &text);
            // The app reads the clipboard after it handles the keystroke; restoring too early
            // would paste the old content.
            let slow = SLOW_PASTE_APPS.contains(&ready.target.exe.as_str()) || ready.field.is_none();
            let settle = Duration::from_millis(if slow { 450 } else { 220 });
            if let Some(rest) = settle.checked_sub(started.elapsed()) {
                std::thread::sleep(rest);
            }
            if let Some(saved) = saved {
                clipboard::restore(saved, sequence, owner);
            }
            Some(if sent { verdict } else { Verdict::Unchanged })
        }
        Method::Type { line_break } => {
            if !keys::type_text(&text, line_break) {
                log::warn!("Windows did not accept all typed keys");
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
        let after = uia::inspect(Duration::from_millis(400));
        last = uia::verdict(Some(before), after.as_ref(), text);
        if last == Verdict::Arrived {
            return last;
        }
        if after.is_none() {
            return Verdict::Unknown;
        }
    }
    last
}

/// Write straight into a classic edit control (Notepad, dialogs, many Win32 apps): no clipboard,
/// no keystrokes, and undoable. Checks the text length grew, since some controls ignore it.
fn direct(control: HWND, text: &str) -> bool {
    if control.0.is_null() {
        return false;
    }
    let length = |control: HWND| -> Option<usize> {
        let mut result = 0usize;
        // SAFETY: WM_GETTEXTLENGTH carries no pointers; the call gives up on a hung app.
        let ok = unsafe {
            SendMessageTimeoutW(control, WM_GETTEXTLENGTH, WPARAM(0), LPARAM(0), SMTO_ABORTIFHUNG, 500, Some(&mut result))
        };
        (ok.0 != 0).then_some(result)
    };
    let Some(before) = length(control) else { return false };
    // The selection, which the text replaces (packed in the result; exact below 64K characters).
    let mut packed = 0usize;
    // SAFETY: EM_GETSEL with null pointers only returns the packed positions.
    let selection = unsafe {
        SendMessageTimeoutW(control, EM_GETSEL, WPARAM(0), LPARAM(0), SMTO_ABORTIFHUNG, 500, Some(&mut packed))
    };
    let selected = (selection.0 != 0 && before < 0xFFFF).then(|| {
        let (start, end) = (packed & 0xFFFF, (packed >> 16) & 0xFFFF);
        end.saturating_sub(start)
    });
    // Edit controls want CRLF line breaks.
    let wide: Vec<u16> = text.replace("\r\n", "\n").replace('\n', "\r\n").encode_utf16().chain(Some(0)).collect();
    let mut result = 0usize;
    // SAFETY: EM_REPLACESEL is a system message, so Windows copies the string into the target
    // process; `wide` stays alive for the duration of the synchronous call.
    let ok = unsafe {
        SendMessageTimeoutW(
            control,
            EM_REPLACESEL,
            WPARAM(1), // undoable
            LPARAM(wide.as_ptr() as isize),
            SMTO_ABORTIFHUNG,
            1_000,
            Some(&mut result),
        )
    };
    if ok.0 == 0 {
        return false;
    }
    let Some(after) = length(control) else { return true };
    direct_landed(before, after, selected, wide.len() - 1)
}

/// Whether EM_REPLACESEL took the text, from the control's length before and after. When in
/// doubt it says yes: a wrong "no" would insert the text a second time by pasting.
fn direct_landed(before: usize, after: usize, selected: Option<usize>, inserted: usize) -> bool {
    if after != before {
        return true;
    }
    // Same length: either nothing happened, or the text replaced a selection of the same size.
    match selected {
        Some(selected) => selected == inserted && inserted > 0,
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dictation::settings::AppRule;

    fn target(exe: &str, focus_class: &str) -> Target {
        Target { hwnd: 1, focus_hwnd: 2, exe: exe.into(), focus_class: focus_class.into(), ..Default::default() }
    }

    #[test]
    fn methods_follow_the_app() {
        let s = DictationSettings::default();
        assert_eq!(choose(&target("notepad.exe", "Edit"), &s), Ok((Method::Direct, false)));
        assert_eq!(choose(&target("chrome.exe", "Chrome_RenderWidgetHostHWND"), &s), Ok((Method::Paste { shift_insert: false }, false)));
        assert_eq!(choose(&target("mintty.exe", ""), &s), Ok((Method::Paste { shift_insert: true }, false)));
        assert_eq!(choose(&target("mstsc.exe", ""), &s), Ok((Method::Type { line_break: LineBreak::Enter }, false)));
    }

    #[test]
    fn settings_and_rules_override() {
        let s = DictationSettings { insert_method: InsertMethod::Type, ..Default::default() };
        assert_eq!(choose(&target("slack.exe", ""), &s), Ok((Method::Type { line_break: LineBreak::ShiftEnter }, true)));
        let s = DictationSettings {
            app_rules: vec![
                AppRule { app: "slack.exe".into(), method: AppMethod::Off, learned: false },
                AppRule { app: "notepad.exe".into(), method: AppMethod::Paste, learned: false },
            ],
            ..Default::default()
        };
        assert_eq!(choose(&target("slack.exe", ""), &s), Err(CopyReason::AppOff));
        assert_eq!(choose(&target("notepad.exe", "Edit"), &s), Ok((Method::Paste { shift_insert: false }, true)));
    }

    #[test]
    fn direct_insertion_is_judged_without_double_inserts() {
        // Grew: in.
        assert!(direct_landed(10, 15, Some(0), 5));
        // Replaced a longer selection: shrank, still in.
        assert!(direct_landed(11, 2, Some(11), 2));
        // Same length with an empty selection: the control ignored it.
        assert!(!direct_landed(10, 10, Some(0), 5));
        // Same length, replaced a selection of the same size: in.
        assert!(direct_landed(10, 10, Some(5), 5));
        // Unknown selection and same length: assume in rather than risk typing it twice.
        assert!(direct_landed(10, 10, None, 5));
    }

    #[test]
    fn terminals_are_recognised() {
        assert!(is_terminal(&target("windowsterminal.exe", "")));
        assert!(is_terminal(&Target { window_class: "ConsoleWindowClass".into(), ..Default::default() }));
        assert!(!is_terminal(&target("code.exe", "")));
        // A form shown by a PowerShell script is an ordinary window; its console is a terminal.
        assert!(!is_terminal(&Target { exe: "powershell.exe".into(), window_class: "WindowsForms10.Window.8.app.0.1".into(), ..Default::default() }));
        assert!(is_terminal(&Target { exe: "powershell.exe".into(), window_class: "ConsoleWindowClass".into(), ..Default::default() }));
    }
}

/// The insertion lab: real keystrokes, the real clipboard and a real edit control, in a test
/// window of its own (never another app). Opt-in, because it takes keyboard focus for a few
/// seconds: `cargo test --lib insertion_lab -- --ignored --test-threads=1`. Each case types only
/// if its own window really has focus.
#[cfg(test)]
mod lab {
    use super::*;
    use windows::core::w;
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DispatchMessageW, GetMessageW, PostMessageW, SendMessageTimeoutW, ShowWindow, TranslateMessage,
        ES_AUTOVSCROLL, ES_MULTILINE, MSG, SMTO_ABORTIFHUNG, SW_SHOW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE,
        WM_GETTEXT, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
    };

    /// A top-level multi-line edit box on its own thread, with a message loop.
    struct LabWindow {
        hwnd: isize,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    impl LabWindow {
        fn open() -> Self {
            let (tx, rx) = flume::bounded(1);
            let thread = std::thread::spawn(move || unsafe {
                let style = WINDOW_STYLE(WS_OVERLAPPEDWINDOW.0 | WS_VISIBLE.0 | ES_MULTILINE as u32 | ES_AUTOVSCROLL as u32);
                let hwnd = CreateWindowExW(
                    WINDOW_EX_STYLE(0), w!("EDIT"), w!(""), style, 200, 200, 640, 300, None, None, None, None,
                )
                .expect("lab window");
                let _ = ShowWindow(hwnd, SW_SHOW);
                let _ = tx.send(hwnd.0 as isize);
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            });
            let hwnd = rx.recv_timeout(Duration::from_secs(5)).expect("lab window opens");
            Self { hwnd, thread: Some(thread) }
        }

        fn text(&self) -> String {
            let mut buf = vec![0u16; 4096];
            let mut copied = 0usize;
            unsafe {
                let _ = SendMessageTimeoutW(
                    target::hwnd(self.hwnd), WM_GETTEXT, WPARAM(buf.len()), LPARAM(buf.as_mut_ptr() as isize),
                    SMTO_ABORTIFHUNG, 1_000, Some(&mut copied),
                );
            }
            String::from_utf16_lossy(&buf[..copied]).replace("\r\n", "\n")
        }
    }

    impl Drop for LabWindow {
        fn drop(&mut self) {
            unsafe {
                let _ = PostMessageW(Some(target::hwnd(self.hwnd)), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
            // Closing a top-level edit destroys it; end its thread's loop too.
            if let Some(t) = self.thread.take() {
                unsafe {
                    use windows::Win32::UI::WindowsAndMessaging::PostThreadMessageW;
                    use windows::Win32::System::Threading::GetThreadId;
                    use std::os::windows::io::AsRawHandle;
                    let id = GetThreadId(windows::Win32::Foundation::HANDLE(t.as_raw_handle()));
                    let _ = PostThreadMessageW(id, windows::Win32::UI::WindowsAndMessaging::WM_QUIT, WPARAM(0), LPARAM(0));
                }
                let _ = t.join();
            }
        }
    }

    /// Insert `text` into a fresh lab window; `None` when the window could not get focus (then
    /// nothing was typed).
    fn run_case(settings: &DictationSettings, text: &str) -> Option<(Outcome, String)> {
        let lab = LabWindow::open();
        let mut target = Target { hwnd: lab.hwnd, ..Default::default() };
        if !target::activate(&target) {
            eprintln!("the lab window could not take focus; skipping");
            return None;
        }
        target = target::snapshot().filter(|t| t.hwnd == lab.hwnd)?;
        let Prepared::Ready(ready) = prepare(&target, settings, 0) else { panic!("lab window refused") };
        if target::foreground() != lab.hwnd {
            return None;
        }
        let outcome = deliver(&ready, text, settings, None);
        std::thread::sleep(Duration::from_millis(250));
        Some((outcome, lab.text()))
    }

    fn method_of(outcome: &Outcome) -> &'static str {
        match outcome {
            Outcome::Inserted { method, .. } => method.name(),
            Outcome::Copied(reason) => panic!("copied instead: {reason:?}"),
        }
    }

    #[test]
    #[ignore = "takes keyboard focus on the real desktop"]
    fn insertion_lab_auto_writes_into_edit_controls() {
        let text = "Hello from the Talkr insertion lab.";
        let Some((outcome, seen)) = run_case(&DictationSettings::default(), text) else { return };
        assert_eq!(method_of(&outcome), "direct");
        assert_eq!(seen, text);
    }

    #[test]
    #[ignore = "takes keyboard focus on the real desktop"]
    fn insertion_lab_paste_restores_the_clipboard() {
        clipboard::put_text("the user's clipboard", None, false);
        let text = "Pasted by the Talkr insertion lab.";
        let settings = DictationSettings { insert_method: InsertMethod::Paste, ..Default::default() };
        let Some((outcome, seen)) = run_case(&settings, text) else { return };
        assert_eq!(method_of(&outcome), "paste");
        assert_eq!(seen, text);
        let saved = clipboard::save(None).expect("clipboard readable");
        let restored = String::from_utf16_lossy(&saved.text_units().unwrap_or_default());
        assert_eq!(restored.trim_end_matches('\0'), "the user's clipboard");
    }

    #[test]
    #[ignore = "takes keyboard focus on the real desktop"]
    fn insertion_lab_types_unicode_and_line_breaks() {
        let text = "Typed by the lab.\nSecond line: ünïcödé ✓ 😀";
        let settings = DictationSettings { insert_method: InsertMethod::Type, ..Default::default() };
        let Some((outcome, seen)) = run_case(&settings, text) else { return };
        assert_eq!(method_of(&outcome), "type");
        assert_eq!(seen, text);
    }
}
