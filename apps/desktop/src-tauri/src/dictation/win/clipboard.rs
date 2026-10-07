//! The clipboard, borrowed for a paste and given back. Talkr saves what the user had, puts the
//! dictated text there marked so clipboard history (Win + V), cloud clipboard and clipboard
//! managers skip it, and afterwards restores the user's content, unless they copied something
//! new in the meantime.

use std::time::Duration;
use windows::core::w;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData, GetClipboardSequenceNumber, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};

const CF_UNICODETEXT: u32 = 13;
/// Saving more than this is skipped (a huge image copied in an editor): the paste still works,
/// but that content is not restored.
const MAX_SAVED_BYTES: usize = 64 * 1024 * 1024;

/// Formats whose data is not an HGLOBAL (GDI objects, metafiles, owner-drawn): they cannot be
/// copied byte for byte. Windows recreates CF_BITMAP from CF_DIB on its own.
fn is_restorable(format: u32) -> bool {
    !matches!(format, 2 | 3 | 9 | 14 | 0x80 | 0x82 | 0x83 | 0x8E) && !(0x0200..=0x03FF).contains(&format)
}

/// Holds the clipboard open; closes it when dropped. Other apps may hold it for a moment (a
/// clipboard manager reading the last copy), so opening retries.
struct Open;

impl Open {
    fn new(owner: Option<HWND>) -> Option<Self> {
        for attempt in 0..20 {
            // SAFETY: closed again in Drop.
            if unsafe { OpenClipboard(owner) }.is_ok() {
                return Some(Open);
            }
            std::thread::sleep(Duration::from_millis(if attempt < 5 { 10 } else { 30 }));
        }
        log::warn!("the clipboard stayed busy; another app is holding it");
        None
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        // SAFETY: we opened it.
        let _ = unsafe { CloseClipboard() };
    }
}

/// The user's clipboard content, saved to put back after a paste.
pub struct Saved {
    formats: Vec<(u32, Vec<u8>)>,
    complete: bool,
}

fn global_bytes(handle: HANDLE) -> Option<Vec<u8>> {
    let global = HGLOBAL(handle.0);
    // SAFETY: clipboard data of a restorable format is an HGLOBAL; it is locked for the copy only.
    unsafe {
        let size = GlobalSize(global);
        let ptr = GlobalLock(global) as *const u8;
        if ptr.is_null() {
            return None;
        }
        let bytes = std::slice::from_raw_parts(ptr, size).to_vec();
        let _ = GlobalUnlock(global);
        Some(bytes)
    }
}

fn global_from(bytes: &[u8]) -> Option<HGLOBAL> {
    // SAFETY: a fresh allocation, filled while locked. Ownership passes to the clipboard on a
    // successful SetClipboardData; otherwise the caller frees it.
    unsafe {
        let global = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)).ok()?;
        let ptr = GlobalLock(global) as *mut u8;
        if ptr.is_null() {
            let _ = GlobalFree(Some(global));
            return None;
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
        let _ = GlobalUnlock(global);
        Some(global)
    }
}

fn set(format: u32, bytes: &[u8]) -> bool {
    let Some(global) = global_from(bytes) else { return false };
    // SAFETY: on success the clipboard owns the memory; on failure it is still ours to free.
    unsafe {
        if SetClipboardData(format, Some(HANDLE(global.0))).is_ok() {
            true
        } else {
            let _ = GlobalFree(Some(global));
            false
        }
    }
}

/// Mark the content being set as private to Talkr: not kept in Win + V history, not synced to
/// other devices, and ignored by clipboard managers that honour the convention.
fn mark_transient() {
    let zero = 0u32.to_ne_bytes();
    // SAFETY: registering names is idempotent; the values are DWORD 0 as the convention asks.
    unsafe {
        for name in [
            w!("ExcludeClipboardContentFromMonitorProcessing"),
            w!("CanIncludeInClipboardHistory"),
            w!("CanUploadToCloudClipboard"),
        ] {
            let format = RegisterClipboardFormatW(name);
            if format != 0 {
                set(format, &zero);
            }
        }
    }
}

#[cfg(test)]
impl Saved {
    /// The plain text in a saved clipboard, as UTF-16 (for tests).
    pub fn text_units(&self) -> Option<Vec<u16>> {
        self.formats
            .iter()
            .find(|(f, _)| *f == CF_UNICODETEXT)
            .map(|(_, bytes)| bytes.as_chunks::<2>().0.iter().map(|c| u16::from_ne_bytes(*c)).collect())
    }
}

/// Save everything on the clipboard that can be saved.
pub fn save(owner: Option<HWND>) -> Option<Saved> {
    let _open = Open::new(owner)?;
    let mut formats = Vec::new();
    let mut total = 0usize;
    let mut complete = true;
    let mut format = 0u32;
    loop {
        // SAFETY: enumeration while the clipboard is open.
        format = unsafe { EnumClipboardFormats(format) };
        if format == 0 {
            break;
        }
        if !is_restorable(format) {
            continue;
        }
        // SAFETY: reading data while the clipboard is open; delayed formats render now.
        let Ok(handle) = (unsafe { GetClipboardData(format) }) else { continue };
        if handle.0.is_null() {
            continue;
        }
        match global_bytes(handle) {
            Some(bytes) if total + bytes.len() <= MAX_SAVED_BYTES => {
                total += bytes.len();
                formats.push((format, bytes));
            }
            Some(_) => {
                complete = false;
                log::info!("clipboard content is too large to keep while pasting; it will not be restored");
                break;
            }
            None => {}
        }
    }
    Some(Saved { formats, complete })
}

/// Put `text` on the clipboard as plain text. Returns the clipboard's sequence number afterwards,
/// which changes as soon as anyone else writes to it.
pub fn put_text(text: &str, owner: Option<HWND>, transient: bool) -> Option<u32> {
    let mut units: Vec<u16> = crlf(text).encode_utf16().collect();
    units.push(0);
    let bytes: Vec<u8> = units.iter().flat_map(|u| u.to_ne_bytes()).collect();
    {
        let _open = Open::new(owner)?;
        // SAFETY: the clipboard is open.
        unsafe { EmptyClipboard() }.ok()?;
        if !set(CF_UNICODETEXT, &bytes) {
            return None;
        }
        if transient {
            mark_transient();
        }
    }
    // SAFETY: plain query.
    Some(unsafe { GetClipboardSequenceNumber() })
}

/// The clipboard's change counter right now.
pub fn sequence() -> u32 {
    // SAFETY: plain query.
    unsafe { GetClipboardSequenceNumber() }
}

/// Clipboard text uses Windows line breaks: classic edit boxes show a bare LF as nothing.
pub fn crlf(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\n', "\r\n")
}

/// Put the saved content back, unless the clipboard changed since `sequence` (the user copied
/// something new, which must win).
pub fn restore(saved: Saved, sequence: u32, owner: Option<HWND>) -> bool {
    // SAFETY: plain query.
    if unsafe { GetClipboardSequenceNumber() } != sequence {
        log::info!("the clipboard changed during the paste; leaving the new content");
        return false;
    }
    if !saved.complete {
        return false;
    }
    let Some(_open) = Open::new(owner) else { return false };
    // SAFETY: the clipboard is open.
    if unsafe { EmptyClipboard() }.is_err() {
        return false;
    }
    for (format, bytes) in &saved.formats {
        set(*format, bytes);
    }
    // Restoring is not a new copy: keep it out of history too.
    mark_transient();
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_text_uses_windows_line_breaks() {
        assert_eq!(crlf("a\nb"), "a\r\nb");
        assert_eq!(crlf("a\r\nb\n\nc"), "a\r\nb\r\n\r\nc");
        assert_eq!(crlf("plain"), "plain");
    }

    #[test]
    fn gdi_and_private_formats_are_skipped() {
        assert!(is_restorable(CF_UNICODETEXT));
        assert!(is_restorable(8)); // CF_DIB
        assert!(is_restorable(0xC123)); // registered formats (HTML, RTF)
        assert!(!is_restorable(2)); // CF_BITMAP
        assert!(!is_restorable(14)); // CF_ENHMETAFILE
        assert!(!is_restorable(0x0250));
        assert!(!is_restorable(0x0300));
    }
}
