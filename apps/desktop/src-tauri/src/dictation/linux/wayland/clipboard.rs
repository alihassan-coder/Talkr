//! The clipboard on Wayland. An app may only set it while one of its windows has focus, which
//! Talkr's never has when dictating, so it goes around that:
//!
//! - **data-control** (wlr or ext protocol; wlroots compositors, KDE): set and read the clipboard
//!   from a connection of its own. wl-clipboard-rs serves what Talkr copies from its own thread
//!   until another app takes the clipboard.
//! - **the Clipboard portal** on the keyboard session (GNOME): see `remote`.
//!
//! The user's clipboard is saved whole (every type it offers) and put back after pasting, but
//! only while Talkr's text is still there.

use std::io::Read;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::Duration;
use wl_clipboard_rs::copy::{self, MimeSource, MimeType, Source};
use wl_clipboard_rs::paste;

/// A clipboard larger than this is not saved (and so not replaced: Talkr types instead).
pub const MAX_SAVED_BYTES: usize = 16 * 1024 * 1024;
/// How long reading the user's clipboard from another app may take.
pub const READ_TIMEOUT: Duration = Duration::from_millis(1_500);

/// The text types Talkr offers its own text under.
pub const TEXT_TYPES: &[&str] = &["text/plain;charset=utf-8", "text/plain", "UTF8_STRING", "STRING", "TEXT"];

/// Offered with a pasted text, so restoring knows the clipboard still holds it.
const MARKER: &str = "application/x-talkr-dictation";

/// Clipboard contents: each MIME type with its data, in the owner's order.
pub type Contents = Vec<(String, Vec<u8>)>;

/// `text` under every text type.
pub fn text_contents(text: &str) -> Contents {
    TEXT_TYPES.iter().map(|t| (t.to_string(), text.as_bytes().to_vec())).collect()
}

/// X11 conversion targets, which are not data.
pub fn worth_saving(mime: &str) -> bool {
    !matches!(mime, "TARGETS" | "MULTIPLE" | "TIMESTAMP" | "SAVE_TARGETS" | "DELETE" | "INCR")
}

/// Read to the end, giving up past `cap` bytes.
pub fn read_capped(reader: impl Read, cap: usize) -> Option<Vec<u8>> {
    let mut data = Vec::new();
    reader.take(cap as u64 + 1).read_to_end(&mut data).ok()?;
    (data.len() <= cap).then_some(data)
}

/// wl-clipboard-rs expects a well-formed seat list and unwraps; a compositor that sends
/// something odd must not take Talkr down.
fn guarded<T>(what: &str, f: impl FnOnce() -> T) -> Option<T> {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(value) => Some(value),
        Err(_) => {
            log::warn!("dictation: the Wayland clipboard failed while {}", what);
            None
        }
    }
}

/// Put `text` on the clipboard (and the primary selection, which some terminals paste with
/// Shift + Insert). `transient`: only for pasting; marked, and hinted to clipboard managers as
/// not worth keeping.
pub fn put_text(text: &str, transient: bool, primary: bool) -> bool {
    let mut sources = vec![MimeSource { source: Source::Bytes(text.as_bytes().into()), mime_type: MimeType::Text }];
    if transient {
        sources.push(MimeSource { source: Source::Bytes(b"1"[..].into()), mime_type: MimeType::Specific(MARKER.into()) });
    }
    let mut options = copy::Options::new();
    options
        .clipboard(if primary { copy::ClipboardType::Both } else { copy::ClipboardType::Regular })
        .sensitive(transient);
    match guarded("copying", || options.copy_multi(sources)) {
        Some(Ok(())) => true,
        Some(Err(e)) => {
            log::warn!("dictation: could not set the clipboard: {}", e);
            false
        }
        None => false,
    }
}

/// The clipboard still holds the text Talkr pasted.
pub fn holds_ours() -> bool {
    guarded("reading", || paste::get_mime_types_ordered(paste::ClipboardType::Regular, paste::Seat::Unspecified))
        .and_then(Result::ok)
        .is_some_and(|types| types.iter().any(|t| t == MARKER))
}

/// The clipboard as it is now, every type. `Some(empty)` when it is empty; `None` when it could
/// not be read whole.
pub fn save() -> Option<Contents> {
    let types = match guarded("reading", || {
        paste::get_mime_types_ordered(paste::ClipboardType::Regular, paste::Seat::Unspecified)
    })? {
        Ok(types) => types,
        Err(paste::Error::ClipboardEmpty | paste::Error::NoMimeType) => return Some(Vec::new()),
        Err(e) => {
            log::info!("dictation: could not read the clipboard: {}", e);
            return None;
        }
    };
    let mut out = Vec::new();
    let mut total = 0;
    for mime in types.into_iter().filter(|t| worth_saving(t)) {
        let data = read_type(&mime)?;
        total += data.len();
        if total > MAX_SAVED_BYTES {
            log::info!("dictation: the clipboard is too large to keep");
            return None;
        }
        out.push((mime, data));
    }
    Some(out)
}

/// One type of the clipboard. The owning app writes it into a pipe; one that never finishes
/// is given up on (its reader thread ends when it does).
fn read_type(mime: &str) -> Option<Vec<u8>> {
    let request = mime.to_string();
    let (tx, rx) = flume::bounded(1);
    std::thread::Builder::new()
        .name("talkr-clipboard-read".into())
        .spawn(move || {
            let data = guarded("reading", || {
                paste::get_contents(paste::ClipboardType::Regular, paste::Seat::Unspecified, paste::MimeType::Specific(&request))
            })
            .and_then(Result::ok)
            .and_then(|(pipe, _)| read_capped(pipe, MAX_SAVED_BYTES));
            let _ = tx.send(data);
        })
        .ok()?;
    let data = rx.recv_timeout(READ_TIMEOUT).ok().flatten();
    if data.is_none() {
        log::info!("dictation: could not read the clipboard's {}", mime);
    }
    data
}

/// Put saved contents back exactly (no extra types); empty contents clear the clipboard.
pub fn restore(saved: Contents) -> bool {
    let result = if saved.is_empty() {
        guarded("clearing", || copy::clear(copy::ClipboardType::Regular, copy::Seat::All))
    } else {
        let sources = saved
            .into_iter()
            .map(|(mime, data)| MimeSource { source: Source::Bytes(data.into()), mime_type: MimeType::Specific(mime) })
            .collect();
        let mut options = copy::Options::new();
        options.omit_additional_text_mime_types(true);
        guarded("restoring", || options.copy_multi(sources))
    };
    match result {
        Some(Ok(())) => true,
        Some(Err(e)) => {
            log::warn!("dictation: could not restore the clipboard: {}", e);
            false
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_goes_under_every_text_type() {
        let c = text_contents("héllo");
        assert_eq!(c.len(), TEXT_TYPES.len());
        assert!(c.iter().all(|(_, d)| d == "héllo".as_bytes()));
        assert_eq!(c[0].0, "text/plain;charset=utf-8");
    }

    #[test]
    fn conversion_targets_are_not_saved() {
        assert!(worth_saving("text/plain"));
        assert!(worth_saving("image/png"));
        assert!(worth_saving(MARKER));
        assert!(!worth_saving("TARGETS"));
        assert!(!worth_saving("SAVE_TARGETS"));
    }

    #[test]
    fn reading_stops_at_the_cap() {
        assert_eq!(read_capped(&b"hello"[..], 5), Some(b"hello".to_vec()));
        assert_eq!(read_capped(&b"hello!"[..], 5), None);
        assert_eq!(read_capped(&b""[..], 5), Some(Vec::new()));
    }

    #[test]
    fn panics_become_failures() {
        assert_eq!(guarded("testing", || 3), Some(3));
        assert_eq!(guarded("testing", || -> u8 { "x".parse().expect("compositor bug") }), None);
    }
}
