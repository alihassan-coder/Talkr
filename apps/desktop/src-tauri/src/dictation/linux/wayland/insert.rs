//! Getting a dictation into the focused field on Wayland.
//!
//! 1. Inserting needs keyboard access (the RemoteDesktop session). Without it the text is
//!    copied, and the reason says how to allow it.
//! 2. Pasting is preferred: save the user's clipboard whole, put the text there, press
//!    Shift + Insert, and put the clipboard back once the app has read it, but only while the
//!    text is still there (the user may have copied something meanwhile).
//! 3. Otherwise each character is typed.
//!
//! Nothing can be verified (an app cannot read another app's field on Wayland), and nothing is
//! inserted twice: once a key went through, a failure ends in copying, never in another try.

use std::sync::Arc;
use std::time::{Duration, Instant};
use super::clipboard::{self, text_contents, Contents};
use super::keysym;
use super::plan::{self, Availability, CopyReason, Method};
use super::portal;
use super::remote::{self, Remote};
use super::shortcuts;
use crate::dictation::backend::Delivery;
use crate::dictation::settings::{DictationSettings, InsertMethod};

/// How long a dictation waits for keyboard access the user allowed before to be restored.
const RESTORE_WAIT: Duration = Duration::from_secs(4);
/// How long the user may still hold the shortcut's keys before Talkr types anyway.
const RELEASE_WAIT: Duration = Duration::from_millis(1_500);
/// After pressing paste: how long the app may take to ask for the clipboard, and how long to
/// wait when Talkr cannot see the request (data-control serves it on its own thread).
const PASTE_READ_WAIT: Duration = Duration::from_millis(1_500);
const PASTE_SETTLE: Duration = Duration::from_millis(500);

/// Where Talkr can set the clipboard.
enum Route {
    DataControl { primary: bool },
    Portal(Arc<Remote>),
}

impl Route {
    /// Data-control first: it needs no session and also sets the primary selection.
    fn pick(av: &Availability, remote: Option<&Arc<Remote>>) -> Option<Route> {
        if av.data_control {
            return Some(Route::DataControl { primary: av.primary });
        }
        remote.filter(|r| r.has_clipboard()).map(|r| Route::Portal(r.clone()))
    }

    fn save(&self) -> Option<Contents> {
        match self {
            Route::DataControl { .. } => clipboard::save(),
            Route::Portal(r) => r.read_selection(),
        }
    }

    /// Put `text` there for pasting.
    fn put_for_paste(&self, text: &str) -> bool {
        match self {
            Route::DataControl { primary } => clipboard::put_text(text, true, *primary),
            Route::Portal(r) => r.set_selection(text_contents(text)),
        }
    }

    /// Put `text` there for the user to paste.
    fn copy(&self, text: &str) -> bool {
        match self {
            Route::DataControl { .. } => clipboard::put_text(text, false, false),
            Route::Portal(r) => r.set_selection(text_contents(text)),
        }
    }

    fn holds_ours(&self) -> bool {
        match self {
            Route::DataControl { .. } => clipboard::holds_ours(),
            Route::Portal(r) => r.owns_selection(),
        }
    }

    fn restore(&self, saved: Contents) {
        let restored = match self {
            Route::DataControl { .. } => clipboard::restore(saved),
            // The portal cannot empty the clipboard: an empty one keeps the dictation.
            Route::Portal(_) if saved.is_empty() => true,
            Route::Portal(r) => r.set_selection(saved),
        };
        if !restored {
            log::warn!("dictation: the clipboard could not be put back");
        }
    }

    /// Wait until the app has read the pasted text.
    fn wait_for_paste(&self, transfers_before: u64) {
        match self {
            Route::Portal(r) => {
                let deadline = Instant::now() + PASTE_READ_WAIT;
                while r.transfers() == transfers_before && Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(10));
                }
                // Some apps ask for a second type right after the first.
                std::thread::sleep(Duration::from_millis(150));
            }
            Route::DataControl { .. } => std::thread::sleep(PASTE_SETTLE),
        }
    }
}

/// Insert `text` wherever the keyboard focus is.
pub fn deliver(text: &str, settings: &DictationSettings) -> Delivery {
    let av = portal::availability();
    let remote = if av.keyboard { remote::ensure(false, RESTORE_WAIT) } else { None };
    let route = Route::pick(&av, remote.as_ref());
    let Some(remote) = remote else {
        let reason = if av.keyboard { CopyReason::NotAllowed } else { CopyReason::NoPortal };
        return copied(reason, route.as_ref(), text, settings);
    };
    shortcuts::wait_released(RELEASE_WAIT);

    // Save the clipboard only when pasting may follow.
    let wants_paste = route.is_some() && settings.insert_method != InsertMethod::Type;
    let saved = match &route {
        Some(route) if wants_paste && settings.restore_clipboard => route.save(),
        _ => None,
    };
    let keeps_clipboard = !settings.restore_clipboard || saved.is_some();
    match (plan::choose(settings.insert_method, true, route.is_some(), keeps_clipboard), route) {
        (Some(Method::Paste), Some(route)) => paste(&remote, &route, text, saved, settings),
        (_, route) => type_text(&remote, route.as_ref(), text, settings),
    }
}

fn paste(remote: &Arc<Remote>, route: &Route, text: &str, saved: Option<Contents>, settings: &DictationSettings) -> Delivery {
    if !route.put_for_paste(text) {
        log::info!("dictation: could not set the clipboard; typing instead");
        return type_text(remote, Some(route), text, settings);
    }
    let transfers = remote.transfers();
    let keys = keysym::paste();
    if let Err(stopped) = remote.send(&keys) {
        log::warn!("dictation: pressing paste failed after {} of {} keys: {}", stopped.sent, keys.len(), stopped.error);
        // Shift alone changes nothing; once Insert went down, the app may have pasted.
        if stopped.sent < 2 {
            // The text is on the clipboard already.
            return Delivery::copied(CopyReason::Failed.message(), true);
        }
    }
    route.wait_for_paste(transfers);
    if settings.restore_clipboard {
        if let Some(saved) = saved {
            if route.holds_ours() {
                route.restore(saved);
            }
        }
    }
    Delivery { inserted: true, method: Some(Method::Paste.name()), ..Delivery::default() }
}

fn type_text(remote: &Arc<Remote>, route: Option<&Route>, text: &str, settings: &DictationSettings) -> Delivery {
    match remote.send(&keysym::strokes(text)) {
        Ok(()) => Delivery { inserted: true, method: Some(Method::Type.name()), ..Delivery::default() },
        Err(stopped) => {
            log::warn!("dictation: typing stopped after {} key events: {}", stopped.sent, stopped.error);
            let reason = if stopped.sent == 0 { CopyReason::Failed } else { CopyReason::Interrupted };
            copied(reason, route, text, settings)
        }
    }
}

fn copied(reason: CopyReason, route: Option<&Route>, text: &str, settings: &DictationSettings) -> Delivery {
    let ok = route.is_some_and(|r| r.copy(text));
    Delivery::copied(copy_message(reason, ok, settings.save_history), ok)
}

/// Why the text was not inserted, and where it is when it could not be copied either.
fn copy_message(reason: CopyReason, copied: bool, history: bool) -> String {
    if copied || !history {
        reason.message().to_string()
    } else {
        format!("{}; the text is in Talkr's History", reason.message())
    }
}

/// Put `text` on the clipboard for the user (no keyboard access needed).
pub fn copy(text: &str) -> bool {
    let av = portal::availability();
    Route::pick(&av, remote::active().as_ref()).is_some_and(|r| r.copy(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_control_is_preferred() {
        let av = Availability { data_control: true, primary: true, ..Availability::default() };
        assert!(matches!(Route::pick(&av, None), Some(Route::DataControl { primary: true })));
        let av = Availability { data_control: true, ..Availability::default() };
        assert!(matches!(Route::pick(&av, None), Some(Route::DataControl { primary: false })));
        // The portal clipboard needs the keyboard session.
        let av = Availability { portal_clipboard: true, keyboard: true, ..Availability::default() };
        assert!(Route::pick(&av, None).is_none());
        assert!(Route::pick(&Availability::default(), None).is_none());
    }

    #[test]
    fn the_message_says_where_the_text_is() {
        assert_eq!(copy_message(CopyReason::NotAllowed, true, true), CopyReason::NotAllowed.message());
        let lost = copy_message(CopyReason::NoPortal, false, true);
        assert!(lost.starts_with(CopyReason::NoPortal.message()));
        assert!(lost.ends_with("History"));
        // Without history there is nowhere to point to.
        assert_eq!(copy_message(CopyReason::Failed, false, false), CopyReason::Failed.message());
    }
}
