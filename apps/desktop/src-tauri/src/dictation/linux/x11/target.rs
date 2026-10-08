//! The window the user is dictating into, as the window manager reports it (EWMH), and bringing
//! it back to the front.

use std::time::{Duration, Instant};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{AtomEnum, ClientMessageEvent, ConnectionExt as _, EventMask, InputFocus, Window};
use super::xconn::{Display, Fail};

/// What had focus when the shortcut was pressed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Target {
    /// The top-level client window.
    pub window: Window,
    /// WM_CLASS: the instance ("gnome-terminal-server") and the class ("Gnome-terminal").
    pub instance: String,
    pub class: String,
    pub pid: Option<u32>,
}

impl Target {
    /// Lower-case WM_CLASS class (the instance when there is none): the app id for rules.
    pub fn app_id(&self) -> Option<String> {
        let id = if self.class.is_empty() { &self.instance } else { &self.class };
        (!id.is_empty()).then(|| id.to_lowercase())
    }

    /// This is one of Talkr's own windows (the pill, the main window).
    pub fn is_ours(&self) -> bool {
        self.pid == Some(std::process::id())
    }
}

/// The active top-level window: `_NET_ACTIVE_WINDOW`, or the input focus where no EWMH window
/// manager runs.
pub fn active_window(d: &Display) -> Result<Option<Window>, String> {
    let reply = d.conn.get_property(false, d.root, d.atoms._NET_ACTIVE_WINDOW, AtomEnum::WINDOW, 0, 1).x()?.reply().x()?;
    if reply.type_ == u32::from(AtomEnum::WINDOW) {
        return Ok(reply.value32().and_then(|mut v| v.next()).filter(|&w| w != 0));
    }
    let focus = d.conn.get_input_focus().x()?.reply().x()?.focus;
    // 0 is None and 1 PointerRoot: no window has focus.
    if focus <= 1 || focus == d.root {
        return Ok(None);
    }
    Ok(Some(top_level(d, focus)?))
}

/// The nearest window at or above `window` that has a WM_CLASS (focus can sit on a child).
fn top_level(d: &Display, window: Window) -> Result<Window, String> {
    let mut current = window;
    for _ in 0..32 {
        if wm_class(d, current)?.is_some() {
            return Ok(current);
        }
        let tree = d.conn.query_tree(current).x()?.reply().x()?;
        if tree.parent == 0 || tree.parent == tree.root {
            break;
        }
        current = tree.parent;
    }
    Ok(window)
}

fn wm_class(d: &Display, window: Window) -> Result<Option<(String, String)>, String> {
    let reply = d.conn.get_property(false, window, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 256).x()?.reply().x()?;
    if reply.value.is_empty() {
        return Ok(None);
    }
    let mut parts = reply.value.split(|&b| b == 0).map(|p| String::from_utf8_lossy(p).into_owned());
    Ok(Some((parts.next().unwrap_or_default(), parts.next().unwrap_or_default())))
}

fn pid(d: &Display, window: Window) -> Result<Option<u32>, String> {
    let reply = d.conn.get_property(false, window, d.atoms._NET_WM_PID, AtomEnum::CARDINAL, 0, 1).x()?.reply().x()?;
    Ok(reply.value32().and_then(|mut v| v.next()).filter(|&p| p != 0))
}

pub fn describe(d: &Display, window: Window) -> Result<Target, String> {
    let (instance, class) = wm_class(d, window)?.unwrap_or_default();
    Ok(Target { window, instance, class, pid: pid(d, window)? })
}

pub fn snapshot(d: &Display) -> Result<Option<Target>, String> {
    match active_window(d)? {
        Some(window) => Ok(Some(describe(d, window)?)),
        None => Ok(None),
    }
}

pub fn exists(d: &Display, window: Window) -> bool {
    d.conn.get_window_attributes(window).ok().and_then(|c| c.reply().ok()).is_some()
}

fn supports_active_window(d: &Display) -> bool {
    d.conn
        .get_property(false, d.root, d.atoms._NET_SUPPORTED, AtomEnum::ATOM, 0, 4096)
        .ok()
        .and_then(|c| c.reply().ok())
        .and_then(|r| r.value32().map(|mut atoms| atoms.any(|a| a == d.atoms._NET_ACTIVE_WINDOW)))
        .unwrap_or(false)
}

/// Bring `window` to the front with keyboard focus and check that it worked: through the window
/// manager (a `_NET_ACTIVE_WINDOW` request, as a pager would send it), or directly where none
/// runs.
pub fn activate(d: &Display, window: Window) -> Result<bool, String> {
    if active_window(d)? == Some(window) {
        return Ok(true);
    }
    if supports_active_window(d) {
        // Source 2: a pager or similar tool acting for the user, which window managers honour
        // even when they refuse focus stealing from applications.
        let message = ClientMessageEvent::new(32, window, d.atoms._NET_ACTIVE_WINDOW, [2, x11rb::CURRENT_TIME, 0, 0, 0]);
        d.conn
            .send_event(false, d.root, EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY, message)
            .x()?;
    } else {
        d.conn.set_input_focus(InputFocus::PARENT, window, x11rb::CURRENT_TIME).x()?;
    }
    d.conn.flush().x()?;
    let deadline = Instant::now() + Duration::from_millis(800);
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(25));
        if active_window(d)? == Some(window) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// A name for the pill from WM_CLASS: "Google-chrome" is "Google Chrome", "org.gnome.Nautilus"
/// is "Nautilus".
pub fn app_label(class: &str) -> Option<String> {
    const KNOWN: &[(&str, &str)] = &[
        ("gnome-terminal-server", "Terminal"),
        ("org.gnome.console", "Console"),
        ("kgx", "Console"),
        ("code", "VS Code"),
        ("code-oss", "VS Code"),
        ("vscodium", "VSCodium"),
        ("telegramdesktop", "Telegram"),
        ("jetbrains-idea", "IntelliJ IDEA"),
        ("jetbrains-pycharm", "PyCharm"),
        ("libreoffice-writer", "LibreOffice Writer"),
        ("libreoffice-calc", "LibreOffice Calc"),
        ("soffice", "LibreOffice"),
        ("xterm", "XTerm"),
        ("urxvt", "URxvt"),
    ];
    let class = class.trim();
    if class.is_empty() {
        return None;
    }
    let lower = class.to_lowercase();
    if let Some((_, name)) = KNOWN.iter().find(|(k, _)| *k == lower) {
        return Some(name.to_string());
    }
    // Reverse-DNS ids name the app last.
    let name = if class.matches('.').count() >= 2 { class.rsplit('.').next().unwrap_or(class) } else { class };
    let words: Vec<String> = name
        .split(['-', '_', ' '])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut cs = w.chars();
            match cs.next() {
                Some(first) => first.to_uppercase().chain(cs).collect(),
                None => String::new(),
            }
        })
        .collect();
    (!words.is_empty()).then(|| words.join(" "))
}

/// How an app takes a paste.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasteKeys {
    CtrlV,
    /// Terminal emulators, where Ctrl + V is a control character.
    CtrlShiftV,
    /// xterm and its kin paste the PRIMARY selection with Shift + Insert.
    ShiftInsert,
}

/// Terminal emulators by WM_CLASS (instance or class, lower case).
const TERMINALS: &[&str] = &[
    "xterm", "uxterm", "rxvt", "urxvt", "st", "st-256color", "eterm", "aterm", "gnome-terminal", "gnome-terminal-server",
    "org.gnome.console", "kgx", "org.gnome.ptyxis", "ptyxis", "konsole", "org.kde.konsole", "yakuake", "xfce4-terminal",
    "mate-terminal", "lxterminal", "qterminal", "terminator", "tilix", "com.gexperts.tilix", "kitty", "alacritty",
    "org.wezfurlong.wezterm", "wezterm", "wezterm-gui", "foot", "footclient", "ghostty", "com.mitchellh.ghostty", "sakura",
    "guake", "tilda", "terminology", "deepin-terminal", "io.elementary.terminal", "pantheon-terminal", "cool-retro-term",
    "termite", "roxterm", "hyper", "tabby", "warp", "dev.warp.warp", "blackbox", "com.raggesilver.blackbox", "contour",
    "rio", "kermit", "lilyterm", "x-terminal-emulator",
];
const SHIFT_INSERT: &[&str] = &["xterm", "uxterm", "rxvt", "urxvt", "st", "st-256color", "eterm", "aterm"];

fn matches(target: &Target, list: &[&str]) -> bool {
    [&target.class, &target.instance].iter().any(|n| !n.is_empty() && list.contains(&n.to_lowercase().as_str()))
}

pub fn is_terminal(target: &Target) -> bool {
    matches(target, TERMINALS)
}

pub fn paste_keys(target: &Target) -> PasteKeys {
    if matches(target, SHIFT_INSERT) {
        PasteKeys::ShiftInsert
    } else if is_terminal(target) {
        PasteKeys::CtrlShiftV
    } else {
        PasteKeys::CtrlV
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(instance: &str, class: &str) -> Target {
        Target { window: 1, instance: instance.into(), class: class.into(), pid: None }
    }

    #[test]
    fn labels_are_readable() {
        assert_eq!(app_label("Google-chrome").as_deref(), Some("Google Chrome"));
        assert_eq!(app_label("firefox").as_deref(), Some("Firefox"));
        assert_eq!(app_label("org.gnome.Nautilus").as_deref(), Some("Nautilus"));
        assert_eq!(app_label("Gnome-terminal-server").as_deref(), Some("Terminal"));
        assert_eq!(app_label("Code").as_deref(), Some("VS Code"));
        assert_eq!(app_label("Slack").as_deref(), Some("Slack"));
        assert_eq!(app_label("VirtualBox Manager").as_deref(), Some("VirtualBox Manager"));
        assert_eq!(app_label(""), None);
    }

    #[test]
    fn app_ids_are_lower_case_classes() {
        assert_eq!(app("slack", "Slack").app_id().as_deref(), Some("slack"));
        assert_eq!(app("xterm", "").app_id().as_deref(), Some("xterm"));
        assert_eq!(app("", "").app_id(), None);
    }

    #[test]
    fn terminals_are_recognised() {
        assert_eq!(paste_keys(&app("gnome-terminal-server", "Gnome-terminal")), PasteKeys::CtrlShiftV);
        assert_eq!(paste_keys(&app("kitty", "kitty")), PasteKeys::CtrlShiftV);
        assert_eq!(paste_keys(&app("Alacritty", "Alacritty")), PasteKeys::CtrlShiftV);
        assert_eq!(paste_keys(&app("konsole", "konsole")), PasteKeys::CtrlShiftV);
        assert_eq!(paste_keys(&app("xterm", "XTerm")), PasteKeys::ShiftInsert);
        assert_eq!(paste_keys(&app("urxvt", "URxvt")), PasteKeys::ShiftInsert);
        assert_eq!(paste_keys(&app("Navigator", "firefox")), PasteKeys::CtrlV);
        assert_eq!(paste_keys(&app("code", "Code")), PasteKeys::CtrlV);
        assert!(is_terminal(&app("org.wezfurlong.wezterm", "org.wezfurlong.wezterm")));
        assert!(!is_terminal(&app("", "")));
        // A substring is not enough: "stacer" is no st.
        assert!(!is_terminal(&app("stacer", "Stacer")));
    }
}
