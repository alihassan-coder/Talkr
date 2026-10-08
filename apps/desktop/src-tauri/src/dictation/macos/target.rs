//! Where the text should go, and how: the app that had focus when the shortcut was pressed, and
//! the per-app knowledge that picks the insertion method. Pure; the system calls that fill a
//! [`Target`] live in `workspace`.

use crate::dictation::settings::{AppMethod, DictationSettings, InsertMethod};

/// The app that had keyboard focus.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Target {
    pub pid: i32,
    /// "com.tinyspeck.slackmacgap"; empty for the few processes without a bundle.
    pub bundle_id: String,
    /// "Slack", as the system names it.
    pub name: Option<String>,
}

/// Remote desktops and virtual machines: their clipboard is not this Mac's, but keystrokes pass.
const TYPE_APPS: &[&str] = &[
    "com.microsoft.rdc.macos",
    "com.microsoft.rdc.mac",
    "com.microsoft.rdc.osx.beta",
    "com.vmware.fusion",
    "com.parallels.desktop.console",
    "org.virtualbox.app.virtualboxvm",
    "com.utmapp.utm",
    "com.citrix.receiver.icaviewer.mac",
    "com.teamviewer.teamviewer",
    "com.philandro.anydesk",
    "com.carriez.rustdesk",
    "tv.parsec.www",
    "com.p5sys.jump.mac.viewer",
];
/// A line break here runs a command: dictation goes in as one line.
const TERMINAL_APPS: &[&str] = &[
    "com.apple.terminal",
    "com.googlecode.iterm2",
    "dev.warp.warp-stable",
    "dev.warp.warp",
    "io.alacritty",
    "org.alacritty",
    "net.kovidgoyal.kitty",
    "com.github.wez.wezterm",
    "co.zeit.hyper",
    "com.mitchellh.ghostty",
    "org.tabby",
    "dev.commandline.waveterm",
];
/// Return sends the message here, so typed line breaks are Shift + Return.
const CHAT_APPS: &[&str] = &[
    "com.tinyspeck.slackmacgap",
    "com.microsoft.teams",
    "com.microsoft.teams2",
    "com.hnc.discord",
    "net.whatsapp.whatsapp",
    "desktop.whatsapp",
    "ru.keepcoder.telegram",
    "org.telegram.desktop",
    "org.whispersystems.signal-desktop",
    "im.riot.app",
    "us.zoom.xos",
    "com.facebook.archon",
    "com.skype.skype",
];
/// Apps that read a paste slowly: the clipboard waits longer before it is restored.
const SLOW_PASTE_APPS: &[&str] = &[
    "com.microsoft.word",
    "com.microsoft.excel",
    "com.microsoft.powerpoint",
    "com.microsoft.outlook",
    "com.microsoft.onenote.mac",
];
/// The desktop, the Dock and the menu bar: no text field unless one clearly has focus (renaming
/// a file, Finder's search field).
const SHELL_APPS: &[&str] = &["com.apple.finder", "com.apple.dock", "com.apple.systemuiserver", "com.apple.loginwindow"];

impl Target {
    /// The identifier per-app rules and logs use: the bundle id, lowercase.
    pub fn app_id(&self) -> Option<String> {
        if !self.bundle_id.is_empty() {
            return Some(self.bundle_id.to_lowercase());
        }
        self.name.as_ref().map(|n| n.to_lowercase()).filter(|n| !n.is_empty())
    }

    /// A friendly name for the pill.
    pub fn label(&self) -> Option<String> {
        self.name.clone().filter(|n| !n.trim().is_empty())
    }

    fn is(&self, list: &[&str]) -> bool {
        self.app_id().is_some_and(|id| list.contains(&id.as_str()))
    }

    pub fn is_terminal(&self) -> bool {
        self.is(TERMINAL_APPS)
    }

    pub fn is_slow_paste(&self) -> bool {
        self.is(SLOW_PASTE_APPS)
    }

    pub fn is_shell(&self) -> bool {
        self.is(SHELL_APPS)
    }

    /// Typing that suits this app: line breaks as Shift + Return in chat apps, real keys for
    /// remote desktops and virtual machines.
    pub fn typing(&self) -> Method {
        Method::Type {
            line_break: if self.is(CHAT_APPS) { LineBreak::ShiftReturn } else { LineBreak::Return },
            keys: self.is(TYPE_APPS),
        }
    }
}

/// How text gets typed: line breaks as Return, or as Shift + Return where Return would send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineBreak {
    Return,
    ShiftReturn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// Through the clipboard and ⌘V.
    Paste,
    /// As keystrokes carrying the text or, with `keys`, as the keys that type each character on
    /// the current layout (for apps that pass key codes on and drop the text: see `layout`).
    Type { line_break: LineBreak, keys: bool },
}

impl Method {
    pub fn name(&self) -> &'static str {
        match self {
            Method::Paste => "paste",
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
    NoPermission,
    AppOff,
    NotDelivered,
    /// Typing real keys, and a character has no key on this layout.
    NotTypable,
    /// Typing stopped part-way: the field may hold the start of the text.
    TypingIncomplete,
}

impl CopyReason {
    pub fn message(&self) -> &'static str {
        match self {
            CopyReason::NoTextField => "No text field was selected",
            CopyReason::FocusChanged => "The app changed",
            CopyReason::TargetClosed => "That app was closed",
            CopyReason::NoPermission => "Allow Talkr in Privacy & Security › Accessibility to insert text",
            CopyReason::AppOff => "Dictation is off for this app",
            CopyReason::NotDelivered => "The app did not accept the text",
            CopyReason::NotTypable => "Some characters cannot be typed as keys into this app",
            CopyReason::TypingIncomplete => "Typing stopped part-way",
        }
    }
}

/// The insertion method for a target and whether the user chose it (a setting or an app rule),
/// or why there is none.
pub fn choose(target: &Target, settings: &DictationSettings) -> Result<(Method, bool), CopyReason> {
    let rule = target.app_id().and_then(|id| settings.rule_for(&id).map(|r| r.method));
    match rule {
        Some(AppMethod::Off) => return Err(CopyReason::AppOff),
        Some(AppMethod::Paste) => return Ok((Method::Paste, true)),
        Some(AppMethod::Type) => return Ok((target.typing(), true)),
        Some(AppMethod::Auto) | None => {}
    }
    Ok(match settings.insert_method {
        InsertMethod::Paste => (Method::Paste, true),
        InsertMethod::Type => (target.typing(), true),
        InsertMethod::Auto if target.is(TYPE_APPS) => (target.typing(), false),
        InsertMethod::Auto => (Method::Paste, false),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dictation::settings::AppRule;

    fn app(bundle_id: &str) -> Target {
        Target { pid: 42, bundle_id: bundle_id.into(), name: None }
    }

    #[test]
    fn ids_and_labels() {
        let slack = Target { pid: 1, bundle_id: "com.tinyspeck.slackmacgap".into(), name: Some("Slack".into()) };
        assert_eq!(slack.app_id().as_deref(), Some("com.tinyspeck.slackmacgap"));
        assert_eq!(slack.label().as_deref(), Some("Slack"));
        let terminal = app("com.apple.Terminal");
        assert_eq!(terminal.app_id().as_deref(), Some("com.apple.terminal"), "ids are lowercase");
        assert!(terminal.is_terminal());
        let tool = Target { pid: 2, bundle_id: String::new(), name: Some("MyTool".into()) };
        assert_eq!(tool.app_id().as_deref(), Some("mytool"));
        assert_eq!(Target::default().app_id(), None);
        assert_eq!(Target { name: Some("  ".into()), ..Target::default() }.label(), None);
    }

    #[test]
    fn methods_follow_the_app() {
        let s = DictationSettings::default();
        assert_eq!(choose(&app("com.apple.TextEdit"), &s), Ok((Method::Paste, false)));
        assert_eq!(choose(&app("com.google.Chrome"), &s), Ok((Method::Paste, false)));
        assert_eq!(
            choose(&app("com.microsoft.rdc.macos"), &s),
            Ok((Method::Type { line_break: LineBreak::Return, keys: true }, false)),
            "remote desktops get real keys"
        );
        assert!(app("com.apple.finder").is_shell());
        assert!(app("com.microsoft.Word").is_slow_paste());
    }

    #[test]
    fn settings_and_rules_override() {
        let s = DictationSettings { insert_method: InsertMethod::Type, ..Default::default() };
        assert_eq!(
            choose(&app("com.tinyspeck.slackmacgap"), &s),
            Ok((Method::Type { line_break: LineBreak::ShiftReturn, keys: false }, true))
        );
        assert_eq!(
            choose(&app("com.vmware.fusion"), &s),
            Ok((Method::Type { line_break: LineBreak::Return, keys: true }, true)),
            "typing into a virtual machine is always by key"
        );
        let s = DictationSettings {
            app_rules: vec![
                AppRule { app: "com.tinyspeck.slackmacgap".into(), method: AppMethod::Off, learned: false },
                AppRule { app: "com.apple.TextEdit".into(), method: AppMethod::Type, learned: true },
                AppRule { app: "com.microsoft.rdc.macos".into(), method: AppMethod::Paste, learned: false },
            ],
            ..Default::default()
        };
        assert_eq!(choose(&app("com.tinyspeck.slackmacgap"), &s), Err(CopyReason::AppOff));
        assert_eq!(
            choose(&app("com.apple.TextEdit"), &s),
            Ok((Method::Type { line_break: LineBreak::Return, keys: false }, true)),
            "rules match the id whatever its case"
        );
        assert_eq!(choose(&app("com.microsoft.rdc.macos"), &s), Ok((Method::Paste, true)));
    }

    #[test]
    fn reasons_read_well() {
        for reason in [
            CopyReason::NoTextField,
            CopyReason::FocusChanged,
            CopyReason::TargetClosed,
            CopyReason::NoPermission,
            CopyReason::AppOff,
            CopyReason::NotDelivered,
            CopyReason::NotTypable,
            CopyReason::TypingIncomplete,
        ] {
            let m = reason.message();
            assert!(!m.is_empty() && !m.ends_with('.') && m.len() < 80, "{m}");
        }
    }
}
