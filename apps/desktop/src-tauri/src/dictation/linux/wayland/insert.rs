//! Getting a dictation into the focused field on Wayland.
//!
//! 1. Inserting needs keyboard access (the RemoteDesktop session). Without it the text is
//!    copied, and the reason says how to allow it.
//! 2. Pasting is preferred (see `plan::choose`): save the user's clipboard whole, put the text
//!    there (and in the primary selection, which terminals paste with Shift + Insert), press
//!    Shift + Insert, wait until the app has read it, and put the clipboard back, but only while
//!    the text is still there (the user may have copied something meanwhile). A paste no app
//!    read is reported as copied, not inserted.
//! 3. Otherwise each character is typed.
//!
//! Nothing can be verified beyond that (an app cannot read another app's field on Wayland), and
//! nothing is inserted twice: once a key went through, a failure ends in copying, never in
//! another try.

use std::sync::Arc;
use std::time::{Duration, Instant};
use super::clipboard::{self, paste_contents, text_contents, Contents};
use super::keysym;
use super::plan::{self, Availability, CopyReason, Means, Method};
use super::portal;
use super::remote::{self, Remote};
use super::shortcuts;
use super::source;
use crate::dictation::backend::Delivery;
use crate::dictation::settings::DictationSettings;

/// How long a dictation waits for keyboard access the user allowed before to be restored.
const RESTORE_WAIT: Duration = Duration::from_secs(4);
/// How long the user may still hold the shortcut's keys before Talkr types anyway.
const RELEASE_WAIT: Duration = Duration::from_millis(1_500);
/// Clipboard managers read a new clipboard at once: their reads, before this, are not the paste.
const MANAGER_SETTLE: Duration = Duration::from_millis(60);
/// After pressing paste: how long the app may take to ask for the clipboard.
const PASTE_READ_WAIT: Duration = Duration::from_millis(2_000);
/// After the app asked: some apps ask for a second type right after the first.
const SECOND_READ: Duration = Duration::from_millis(150);
/// How long to wait when Talkr cannot see the request (wl-clipboard-rs serves it on its own
/// thread, on a compositor with only the ext data-control protocol).
const PASTE_SETTLE: Duration = Duration::from_millis(1_000);

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

    /// The primary selection is set with the clipboard (the portal has none).
    fn primary(&self) -> bool {
        matches!(self, Route::DataControl { primary: true })
    }

    fn save(&self) -> Option<Contents> {
        match self {
            Route::DataControl { .. } => clipboard::save(),
            Route::Portal(r) => r.read_selection(),
        }
    }

    /// Put `text` there for pasting.
    fn put_for_paste(&self, text: &str) -> Option<Put> {
        match self {
            Route::DataControl { primary } => match source::offer(paste_contents(text), *primary) {
                Some(offer) => Some(Put::Counted(offer)),
                None => clipboard::put_text(text, true, *primary).then_some(Put::Blind),
            },
            Route::Portal(r) => r.set_selection(paste_contents(text)).then(|| Put::Portal(r.clone())),
        }
    }

    /// Put `text` there for the user to paste.
    fn copy(&self, text: &str) -> bool {
        match self {
            Route::DataControl { .. } => clipboard::put_text(text, false, false),
            Route::Portal(r) => r.set_selection(text_contents(text)),
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
}

/// The text on the clipboard for one paste.
enum Put {
    /// Served by Talkr's own data-control source, which counts the reads.
    Counted(source::Offer),
    /// Served through the portal session, which sees each read.
    Portal(Arc<Remote>),
    /// Served by wl-clipboard-rs: reads cannot be seen.
    Blind,
}

impl Put {
    /// The reads of it so far, where Talkr can see them.
    fn reads(&self) -> Option<u64> {
        match self {
            Put::Counted(offer) => Some(offer.requests()),
            Put::Portal(r) => Some(r.transfers()),
            Put::Blind => None,
        }
    }

    fn holds_ours(&self) -> bool {
        match self {
            Put::Counted(offer) => offer.live(),
            Put::Portal(r) => r.owns_selection(),
            Put::Blind => clipboard::holds_ours(),
        }
    }

    /// Wait until an app read it (more than `before` reads): false when none did in time. Where
    /// reads cannot be seen, wait a while and assume it was.
    fn wait_read(&self, before: Option<u64>) -> bool {
        let Some(before) = before else {
            std::thread::sleep(PASTE_SETTLE);
            return true;
        };
        let deadline = Instant::now() + PASTE_READ_WAIT;
        loop {
            if self.reads().is_some_and(|n| n > before) {
                std::thread::sleep(SECOND_READ);
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

/// Insert `text` wherever the keyboard focus is.
pub fn deliver(text: &str, settings: &DictationSettings) -> Delivery {
    // Settled: "not probed yet" must not read as "this desktop cannot type".
    let av = portal::settled_availability();
    let remote = if av.keyboard { remote::ensure(false, RESTORE_WAIT) } else { None };
    let route = Route::pick(&av, remote.as_ref());
    let Some(remote) = remote else {
        let reason = if av.keyboard { CopyReason::NotAllowed } else { CopyReason::NoPortal };
        return copied(reason, route.as_ref(), text, settings);
    };
    shortcuts::wait_released(RELEASE_WAIT);

    let mut can = Means {
        keyboard: true,
        clipboard: route.is_some(),
        keeps_clipboard: true,
        primary: route.as_ref().is_some_and(Route::primary),
        plain: plan::plain(text),
    };
    // Save the clipboard only when pasting may follow.
    let saved = match &route {
        Some(route) if settings.restore_clipboard && plan::choose(settings.insert_method, can) == Some(Method::Paste) => {
            let saved = route.save();
            can.keeps_clipboard = saved.is_some();
            saved
        }
        _ => None,
    };
    match (plan::choose(settings.insert_method, can), route) {
        (Some(Method::Paste), Some(route)) => paste(&remote, &route, text, saved, settings),
        (_, route) => type_text(&remote, route.as_ref(), text, settings),
    }
}

fn paste(remote: &Arc<Remote>, route: &Route, text: &str, saved: Option<Contents>, settings: &DictationSettings) -> Delivery {
    let Some(put) = route.put_for_paste(text) else {
        log::info!("dictation: could not set the clipboard; typing instead");
        return type_text(remote, Some(route), text, settings);
    };
    std::thread::sleep(MANAGER_SETTLE);
    let before = put.reads();
    let keys = keysym::paste();
    if let Err(stopped) = remote.send(&keys) {
        log::warn!("dictation: pressing paste failed after {} of {} keys: {}", stopped.sent, keys.len(), stopped.error);
        // Shift alone changes nothing; once Insert may have gone down, the app may have pasted.
        if stopped.attempted < 2 {
            // The text is on the clipboard already.
            return Delivery::copied(CopyReason::Failed.message(), true);
        }
    }
    if !put.wait_read(before) {
        // A terminal may have pasted the primary selection instead, or the app ignored the keys:
        // either way not this text. It stays on the clipboard.
        log::info!("dictation: no app asked for the pasted text");
        return Delivery::copied(CopyReason::NotPasted.message(), true);
    }
    if settings.restore_clipboard {
        if let Some(saved) = saved {
            if put.holds_ours() {
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
            let reason = if stopped.attempted == 0 { CopyReason::Failed } else { CopyReason::Interrupted };
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
    let av = portal::settled_availability();
    Route::pick(&av, remote::active().as_ref()).is_some_and(|r| r.copy(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_control_is_preferred() {
        let av = Availability { data_control: true, primary: true, ..Availability::default() };
        let route = Route::pick(&av, None);
        assert!(matches!(route, Some(Route::DataControl { primary: true })));
        assert!(route.unwrap().primary());
        let av = Availability { data_control: true, ..Availability::default() };
        let route = Route::pick(&av, None);
        assert!(matches!(route, Some(Route::DataControl { primary: false })));
        assert!(!route.unwrap().primary());
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
