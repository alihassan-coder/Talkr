//! Getting text into the window the user chose, the way dictation and accessibility tools do it
//! on X11: put it on the clipboard and send the app's paste keystroke (keeping the user's own
//! clipboard), or type it key by key where pasting does not work.
//!
//! X11 offers no general way to read a field back, so insertion is not verified: the text is
//! sent once, and never again by another method.

use std::time::{Duration, Instant};
use super::chord::{CTRL, SHIFT};
use super::clipboard::{self, Saved, Selection};
use super::input::{self, LineBreak};
use super::keymap::Keymap;
use super::keys;
use super::target::{self, PasteKeys, Target};
use super::xconn::Display;
use crate::dictation::backend::Delivery;
use crate::dictation::settings::{AppMethod, DictationSettings, FocusPolicy, InsertMethod};

/// Remote desktops and virtual machines: their clipboard is not this one, but keystrokes pass.
const TYPE_APPS: &[&str] = &[
    "remmina", "org.remmina.remmina", "xfreerdp", "wlfreerdp", "sdl-freerdp", "vncviewer", "tigervnc", "realvnc-vncviewer",
    "vinagre", "krdc", "org.kde.krdc", "virt-viewer", "remote-viewer", "virtualbox machine", "virtualboxvm", "vmware",
    "vmplayer", "anydesk", "rustdesk", "teamviewer", "parsecd", "wfica",
];
/// Enter sends the message here, so typed line breaks are Shift + Enter.
const CHAT_APPS: &[&str] = &[
    "slack", "discord", "telegramdesktop", "telegram-desktop", "org.telegram.desktop", "signal", "signal-desktop", "element",
    "teams-for-linux", "microsoft teams - preview", "whatsapp", "zoom", "skype", "mattermost", "rocket.chat",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Paste(PasteKeys),
    Type(LineBreak),
}

impl Method {
    pub fn name(&self) -> &'static str {
        match self {
            Method::Paste(_) => "paste",
            Method::Type(_) => "type",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyReason {
    NoTextField,
    NoDisplay,
    FocusChanged,
    TargetClosed,
    AppOff,
    NotDelivered,
}

impl CopyReason {
    pub fn message(&self) -> &'static str {
        match self {
            CopyReason::NoTextField => "No text field was selected",
            CopyReason::NoDisplay => "Talkr could not reach the X server",
            CopyReason::FocusChanged => "The window changed",
            CopyReason::TargetClosed => "That window was closed",
            CopyReason::AppOff => "Dictation is off for this app",
            CopyReason::NotDelivered => "The app did not accept the text",
        }
    }
}

fn listed(target: &Target, list: &[&str]) -> bool {
    target.app_id().is_some_and(|id| list.contains(&id.as_str()))
        || (!target.instance.is_empty() && list.contains(&target.instance.to_lowercase().as_str()))
}

fn typing_for(target: &Target) -> Method {
    Method::Type(if listed(target, CHAT_APPS) { LineBreak::ShiftEnter } else { LineBreak::Enter })
}

/// The insertion method for a target, and whether the user chose it (a setting or app rule), or
/// why there is none.
pub fn choose(target: &Target, settings: &DictationSettings) -> Result<(Method, bool), CopyReason> {
    let paste = Method::Paste(target::paste_keys(target));
    let typing = typing_for(target);
    let rule = target.app_id().and_then(|id| settings.rule_for(&id).map(|r| r.method));
    match rule {
        Some(AppMethod::Off) => return Err(CopyReason::AppOff),
        Some(AppMethod::Paste) => return Ok((paste, true)),
        Some(AppMethod::Type) => return Ok((typing, true)),
        Some(AppMethod::Auto) | None => {}
    }
    Ok(match settings.insert_method {
        InsertMethod::Paste => (paste, true),
        InsertMethod::Type => (typing, true),
        InsertMethod::Auto if listed(target, TYPE_APPS) => (typing, false),
        InsertMethod::Auto => (paste, false),
    })
}

/// Put `text` in `target` (see the module docs). Falls back to copying it, and says why.
pub fn deliver(text: &str, target: Option<&Target>, settings: &DictationSettings) -> Delivery {
    let copy = |reason: CopyReason| Delivery::copied(reason.message(), clipboard::put(Selection::Clipboard, text));
    let Some(original) = target else { return copy(CopyReason::NoTextField) };
    let Some(d) = super::xconn::shared() else { return copy(CopyReason::NoDisplay) };
    let keymap = match Keymap::load(&d.conn) {
        Ok(keymap) => keymap,
        Err(e) => {
            log::warn!("could not read the keyboard map: {}", e);
            return copy(CopyReason::NoDisplay);
        }
    };
    if !input::wait_for_modifiers_released(&d, &keymap, Duration::from_millis(2_000)) {
        log::warn!("modifier keys still held after 2 s; inserting anyway");
    }
    let target = match focus(&d, original, settings.focus_policy) {
        Ok(target) => target,
        Err(reason) => return copy(reason),
    };
    let (method, _forced) = match choose(&target, settings) {
        Ok(choice) => choice,
        Err(reason) => return copy(reason),
    };
    // A line break in a terminal would run the command.
    let text = if target::is_terminal(&target) { crate::dictation::text::single_line(text) } else { text.to_string() };
    let sent = match method {
        Method::Paste(keys) => paste(&d, &keymap, &target, &text, keys, settings.restore_clipboard),
        Method::Type(line_break) => type_into(&d, &keymap, &text, line_break),
    };
    match sent {
        Ok(method) => Delivery { inserted: true, verified: false, method: Some(method.name()), ..Delivery::default() },
        Err(e) => {
            log::warn!("could not insert into {}: {}", target.app_id().unwrap_or_default(), e);
            copy(CopyReason::NotDelivered)
        }
    }
}

/// Make sure the window to insert into has focus, as `policy` says.
fn focus(d: &Display, original: &Target, policy: FocusPolicy) -> Result<Target, CopyReason> {
    let active = target::active_window(d).ok().flatten();
    if active == Some(original.window) {
        return Ok(original.clone());
    }
    let now = active.and_then(|w| target::describe(d, w).ok());
    // Focus on one of Talkr's own windows (a click on the pill) always goes back.
    let ours = now.as_ref().is_some_and(Target::is_ours);
    let back = |original: &Target| -> Result<Target, CopyReason> {
        if !target::exists(d, original.window) {
            return Err(CopyReason::TargetClosed);
        }
        match target::activate(d, original.window) {
            Ok(true) => Ok(original.clone()),
            _ => Err(CopyReason::FocusChanged),
        }
    };
    match policy {
        FocusPolicy::Original => back(original),
        FocusPolicy::Current | FocusPolicy::Copy if ours => back(original),
        FocusPolicy::Current => now.ok_or(CopyReason::NoTextField),
        FocusPolicy::Copy => Err(CopyReason::FocusChanged),
    }
}

fn type_into(d: &Display, keymap: &Keymap, text: &str, line_break: LineBreak) -> Result<Method, String> {
    input::type_text(d, keymap, text, line_break)?;
    Ok(Method::Type(line_break))
}

/// Paste `text` with the app's paste keystroke, keeping what the user had copied: it is read
/// first and put back afterwards, unless something else was copied in between.
fn paste(d: &Display, keymap: &Keymap, target: &Target, text: &str, keys: PasteKeys, restore: bool) -> Result<Method, String> {
    let selections: &[Selection] =
        if keys == PasteKeys::ShiftInsert { &[Selection::Clipboard, Selection::Primary] } else { &[Selection::Clipboard] };
    let mut saved = Vec::new();
    if restore {
        for &selection in selections {
            match clipboard::read(selection) {
                Some(content) => saved.push((selection, content)),
                None => {
                    // An image or files, or an owner that does not answer: pasting would lose
                    // them for good. Type instead.
                    log::info!("the clipboard holds something Talkr cannot put back; typing instead");
                    return type_into(d, keymap, text, typing_line_break(target));
                }
            }
        }
    }
    for &selection in selections {
        if !clipboard::put(selection, text) {
            put_back(&saved, text);
            return Err("could not take the clipboard".into());
        }
    }
    let before = clipboard::served();
    let started = Instant::now();
    let sent = match keys {
        PasteKeys::CtrlV => input::chord(d, keymap, CTRL, keys::XK_V),
        PasteKeys::CtrlShiftV => input::chord(d, keymap, CTRL | SHIFT, keys::XK_V),
        PasteKeys::ShiftInsert => input::chord(d, keymap, SHIFT, keys::XK_INSERT),
    };
    if let Err(e) = sent {
        put_back(&saved, text);
        return Err(format!("the paste keystroke failed: {}", e));
    }
    // The app reads the text after it handles the keystroke: putting the old content back too
    // early would paste that instead. Wait for the request, then a little for the rest of it.
    while clipboard::served() == before && started.elapsed() < Duration::from_millis(1_000) {
        std::thread::sleep(Duration::from_millis(10));
    }
    std::thread::sleep(Duration::from_millis(if clipboard::served() == before { 0 } else { 150 }));
    put_back(&saved, text);
    Ok(Method::Paste(keys))
}

fn typing_line_break(target: &Target) -> LineBreak {
    match typing_for(target) {
        Method::Type(line_break) => line_break,
        Method::Paste(_) => LineBreak::Enter,
    }
}

/// Put the user's content back on each selection that still holds the dictated text.
pub(super) fn put_back(saved: &[(Selection, Saved)], text: &str) {
    for (selection, content) in saved {
        if !clipboard::holds(*selection, text) {
            continue; // something else was copied since: keep it
        }
        match content {
            Saved::Text(previous) => {
                clipboard::put(*selection, previous);
            }
            Saved::Empty => clipboard::clear(*selection),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dictation::settings::AppRule;

    fn app(class: &str) -> Target {
        Target { window: 1, instance: class.to_lowercase(), class: class.into(), pid: None }
    }

    #[test]
    fn methods_follow_the_app() {
        let s = DictationSettings::default();
        assert_eq!(choose(&app("firefox"), &s), Ok((Method::Paste(PasteKeys::CtrlV), false)));
        assert_eq!(choose(&app("kitty"), &s), Ok((Method::Paste(PasteKeys::CtrlShiftV), false)));
        assert_eq!(choose(&app("XTerm"), &s), Ok((Method::Paste(PasteKeys::ShiftInsert), false)));
        assert_eq!(choose(&app("Remmina"), &s), Ok((Method::Type(LineBreak::Enter), false)));
    }

    #[test]
    fn settings_and_rules_override() {
        let s = DictationSettings { insert_method: InsertMethod::Type, ..Default::default() };
        assert_eq!(choose(&app("Slack"), &s), Ok((Method::Type(LineBreak::ShiftEnter), true)));
        let s = DictationSettings {
            app_rules: vec![
                AppRule { app: "slack".into(), method: AppMethod::Off, learned: false },
                AppRule { app: "remmina".into(), method: AppMethod::Paste, learned: false },
            ],
            ..Default::default()
        };
        assert_eq!(choose(&app("Slack"), &s), Err(CopyReason::AppOff));
        assert_eq!(choose(&app("Remmina"), &s), Ok((Method::Paste(PasteKeys::CtrlV), true)));
    }
}
