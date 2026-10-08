//! Keyboard access on Wayland: a RemoteDesktop portal session with a keyboard, which the user
//! allows once. Talkr keeps it open while dictation is on (the desktop may show that an app can
//! control the keyboard) and stores the portal's restore token, so later starts do not ask again.
//!
//! The session also carries the Clipboard portal where the desktop has it (GNOME, which has no
//! data-control protocol): Talkr offers the selection and writes it out when an app pastes.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use ashpd::desktop::clipboard::{Clipboard, SetSelectionOptions};
use ashpd::desktop::remote_desktop::{DeviceType, KeyState, RemoteDesktop, SelectDevicesOptions};
use ashpd::desktop::{PersistMode, ResponseError, Session};
use ashpd::enumflags2::BitFlags;
use futures_util::StreamExt;
use super::clipboard::{read_capped, worth_saving, Contents, MAX_SAVED_BYTES, READ_TIMEOUT, TEXT_TYPES};
use super::keysym::{held_after, Stroke};
use super::portal::{self, describe, lock};

/// Typing pauses briefly after this many key events, so slow apps keep up.
const PACE_EVERY: usize = 48;
const PACE: Duration = Duration::from_millis(3);

pub struct Remote {
    id: u64,
    portal: RemoteDesktop,
    session: Session<RemoteDesktop>,
    /// The Clipboard portal, when the desktop enabled it on this session.
    clipboard: Option<Clipboard>,
    /// What Talkr offers while it owns the selection.
    offer: Mutex<Option<Arc<Contents>>>,
    owns_selection: AtomicBool,
    /// The MIME types of the selection another app owns, as last announced.
    foreign_types: Mutex<Option<Vec<String>>>,
    /// Pastes of Talkr's selection served so far.
    transfers: AtomicU64,
    /// Talkr closed the session: stop watching it.
    ended: tokio::sync::Notify,
}

/// Typing stopped: how many key events went through, and why.
#[derive(Debug)]
pub struct Stopped {
    pub sent: usize,
    pub error: String,
}

impl Remote {
    pub fn has_clipboard(&self) -> bool {
        self.clipboard.is_some()
    }

    pub fn owns_selection(&self) -> bool {
        self.owns_selection.load(Ordering::SeqCst)
    }

    pub fn transfers(&self) -> u64 {
        self.transfers.load(Ordering::SeqCst)
    }

    /// Send key events, in order. On a failure the keys still down are released.
    pub fn send(self: &Arc<Self>, strokes: &[Stroke]) -> Result<(), Stopped> {
        if strokes.is_empty() {
            return Ok(());
        }
        let sent = Arc::new(AtomicUsize::new(0));
        let timeout = Duration::from_secs(3) + Duration::from_millis(4) * strokes.len() as u32;
        let (me, list, counter) = (self.clone(), strokes.to_vec(), sent.clone());
        let result = portal::block(timeout, async move {
            for (i, &(keysym, down)) in list.iter().enumerate() {
                me.key(keysym, down).await?;
                counter.fetch_add(1, Ordering::SeqCst);
                if i % PACE_EVERY == PACE_EVERY - 1 {
                    tokio::time::sleep(PACE).await;
                }
            }
            Ok::<(), String>(())
        });
        let error = match result {
            Some(Ok(())) => return Ok(()),
            Some(Err(e)) => e,
            None => "the desktop did not answer in time".to_string(),
        };
        let sent = sent.load(Ordering::SeqCst);
        let stuck = held_after(strokes, sent);
        if !stuck.is_empty() {
            let me = self.clone();
            portal::block(Duration::from_secs(1), async move {
                for keysym in stuck.into_iter().rev() {
                    let _ = me.key(keysym, false).await;
                }
            });
        }
        Err(Stopped { sent, error })
    }

    async fn key(&self, keysym: u32, down: bool) -> Result<(), String> {
        let state = if down { KeyState::Pressed } else { KeyState::Released };
        // Keysyms fit in 30 bits; the portal takes them as a signed int.
        self.portal
            .notify_keyboard_keysym(&self.session, keysym as i32, state, Default::default())
            .await
            .map_err(|e| describe(&e))
    }

    /// Make `contents` the clipboard, through the Clipboard portal.
    pub fn set_selection(self: &Arc<Self>, contents: Contents) -> bool {
        if self.clipboard.is_none() {
            return false;
        }
        let contents = Arc::new(contents);
        *lock(&self.offer) = Some(contents.clone());
        let me = self.clone();
        let done = portal::block(Duration::from_secs(2), async move {
            let Some(clipboard) = &me.clipboard else { return Err("no clipboard".to_string()) };
            let types: Vec<&str> = contents.iter().map(|(t, _)| t.as_str()).collect();
            clipboard
                .set_selection(&me.session, SetSelectionOptions::default().set_mime_types(&types))
                .await
                .map_err(|e| describe(&e))
        });
        match done {
            Some(Ok(())) => {
                self.owns_selection.store(true, Ordering::SeqCst);
                true
            }
            other => {
                log::warn!("dictation: could not set the clipboard through the portal ({:?})", other);
                *lock(&self.offer) = None;
                false
            }
        }
    }

    /// The clipboard as it is now, every type, to put back later. `None` when it cannot be read
    /// completely.
    pub fn read_selection(self: &Arc<Self>) -> Option<Contents> {
        self.clipboard.as_ref()?;
        if self.owns_selection() {
            return lock(&self.offer).as_ref().map(|o| o.as_slice().to_vec());
        }
        // Not announced yet: only plain text can be asked for blindly, and if that fails the
        // clipboard may hold something else that would be lost.
        let types = lock(&self.foreign_types).clone().unwrap_or_else(|| vec![TEXT_TYPES[0].to_string()]);
        let types: Vec<String> = types.into_iter().filter(|t| worth_saving(t)).collect();
        if types.is_empty() {
            return Some(Vec::new());
        }
        let me = self.clone();
        portal::block(READ_TIMEOUT, async move {
            let clipboard = me.clipboard.as_ref()?;
            let mut out = Vec::new();
            let mut total = 0;
            for mime in types {
                let fd = clipboard.selection_read(&me.session, &mime).await.ok()?;
                let file = std::fs::File::from(std::os::fd::OwnedFd::from(fd));
                let data = tokio::task::spawn_blocking(move || read_capped(file, MAX_SAVED_BYTES)).await.ok()??;
                total += data.len();
                if total > MAX_SAVED_BYTES {
                    return None;
                }
                out.push((mime, data));
            }
            Some(out)
        })
        .flatten()
    }

    /// An app asked for Talkr's selection: write it out.
    async fn serve(&self, mime: String, serial: u32) {
        let Some(clipboard) = &self.clipboard else { return };
        let offer = lock(&self.offer).clone();
        let fd = match clipboard.selection_write(&self.session, serial).await {
            Ok(fd) => fd,
            Err(e) => {
                log::warn!("dictation: could not answer a paste ({})", describe(&e));
                return;
            }
        };
        let written = match offer.and_then(|o| pick(&o, &mime).map(|i| (o, i))) {
            Some((offer, index)) => {
                let write = tokio::task::spawn_blocking(move || {
                    let mut file = std::fs::File::from(std::os::fd::OwnedFd::from(fd));
                    file.write_all(&offer[index].1).is_ok()
                });
                matches!(tokio::time::timeout(Duration::from_secs(3), write).await, Ok(Ok(true)))
            }
            None => false,
        };
        let _ = clipboard.selection_write_done(&self.session, serial, written).await;
        if written {
            self.transfers.fetch_add(1, Ordering::SeqCst);
        }
    }
}

/// Which offered item answers a request for `mime`: the same type, or for any text type, the
/// text.
fn pick(offer: &Contents, mime: &str) -> Option<usize> {
    offer.iter().position(|(t, _)| t == mime).or_else(|| {
        let text = |t: &str| TEXT_TYPES.contains(&t) || t.starts_with("text/plain");
        if text(mime) {
            offer.iter().position(|(t, _)| text(t))
        } else {
            None
        }
    })
}

struct Slot {
    active: Option<Arc<Remote>>,
    starting: bool,
    /// A silent restore failed: wait for the user to ask before trying again.
    restore_failed: bool,
    error: Option<String>,
    next_id: u64,
}

static SLOT: Mutex<Slot> = Mutex::new(Slot { active: None, starting: false, restore_failed: false, error: None, next_id: 1 });
static CHANGED: Condvar = Condvar::new();

/// The running session, if keyboard access was allowed.
pub fn active() -> Option<Arc<Remote>> {
    lock(&SLOT).active.clone()
}

/// Why the last attempt failed, if it did.
pub fn last_error() -> Option<String> {
    lock(&SLOT).error.clone()
}

/// The session, starting it when needed and waiting for it at most `wait`. Without
/// `interactive` only a session the user allowed before is restored (no dialog expected).
pub fn ensure(interactive: bool, wait: Duration) -> Option<Arc<Remote>> {
    let deadline = Instant::now() + wait;
    let mut slot = lock(&SLOT);
    let mut started = false;
    loop {
        if let Some(remote) = &slot.active {
            return Some(remote.clone());
        }
        if !slot.starting {
            // Our own attempt ended without a session. (One that was already running, a silent
            // restore, is followed by ours when the user asked.)
            if started {
                return None;
            }
            // A token that failed to restore would fail again: ask afresh.
            let token = load_token().filter(|_| !slot.restore_failed);
            if !interactive && token.is_none() {
                return None;
            }
            let id = slot.next_id;
            slot.next_id += 1;
            slot.starting = true;
            if !portal::spawn(start(token, id)) {
                slot.starting = false;
                return None;
            }
            started = true;
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return None;
        }
        slot = CHANGED.wait_timeout(slot, left).unwrap_or_else(|e| e.into_inner()).0;
    }
}

/// End the session (dictation was turned off). The restore token stays.
pub fn close() {
    let Some(remote) = lock(&SLOT).active.take() else { return };
    CHANGED.notify_all();
    remote.ended.notify_one();
    portal::block(Duration::from_secs(2), async move {
        let _ = remote.session.close().await;
    });
}

enum Failure {
    Declined,
    Other(String),
}

async fn start(token: Option<String>, id: u64) {
    let restoring = token.is_some();
    let result = open(token, id).await;
    let mut slot = lock(&SLOT);
    slot.starting = false;
    match result {
        Ok((remote, token)) => {
            if let Some(token) = token {
                save_token(&token);
            }
            log::info!("dictation: keyboard access allowed{}", if remote.has_clipboard() { ", with the clipboard" } else { "" });
            let remote = Arc::new(remote);
            slot.active = Some(remote.clone());
            slot.error = None;
            slot.restore_failed = false;
            portal::spawn(watch(remote));
        }
        Err(Failure::Declined) => {
            log::info!("dictation: keyboard access was declined");
            forget_token();
            slot.error = Some("Keyboard access was declined".into());
            slot.restore_failed = true;
        }
        Err(Failure::Other(e)) => {
            log::warn!("dictation: no keyboard access: {}", e);
            slot.error = Some(e);
            slot.restore_failed |= restoring;
        }
    }
    CHANGED.notify_all();
}

async fn open(token: Option<String>, id: u64) -> Result<(Remote, Option<String>), Failure> {
    let fail = |what: &str, e: ashpd::Error| match e {
        ashpd::Error::Response(ResponseError::Cancelled) => Failure::Declined,
        e => Failure::Other(format!("could not {} ({})", what, describe(&e))),
    };
    portal::ready().await;
    let portal = RemoteDesktop::new().await.map_err(|e| fail("reach the remote-desktop portal", e))?;
    let session = portal.create_session(Default::default()).await.map_err(|e| fail("start a session", e))?;
    let mut options = SelectDevicesOptions::default().set_devices(BitFlags::from(DeviceType::Keyboard));
    if portal.version() >= 2 {
        // Allowed once, remembered until the user revokes it.
        options = options.set_persist_mode(PersistMode::ExplicitlyRevoked).set_restore_token(token.as_deref());
    }
    let selected = async { portal.select_devices(&session, options).await?.response() };
    selected.await.map_err(|e| fail("ask for the keyboard", e))?;
    // The clipboard must be asked for before the session starts.
    let clipboard = match Clipboard::new().await {
        Ok(clipboard) => match clipboard.request(&session, Default::default()).await {
            Ok(()) => Some(clipboard),
            Err(e) => {
                log::info!("dictation: the portal clipboard is not available ({})", describe(&e));
                None
            }
        },
        Err(_) => None,
    };
    let started = async { portal.start(&session, None, Default::default()).await?.response() };
    let started = match started.await {
        Ok(started) => started,
        Err(e) => {
            let _ = session.close().await;
            return Err(fail("start keyboard access", e));
        }
    };
    if !started.devices().contains(DeviceType::Keyboard) {
        let _ = session.close().await;
        return Err(Failure::Other("the desktop did not share the keyboard".into()));
    }
    let clipboard = clipboard.filter(|_| started.is_clipboard_enabled());
    let token = started.restore_token().map(str::to_string);
    let remote = Remote {
        id,
        portal,
        session,
        clipboard,
        offer: Mutex::new(None),
        owns_selection: AtomicBool::new(false),
        foreign_types: Mutex::new(None),
        transfers: AtomicU64::new(0),
        ended: tokio::sync::Notify::new(),
    };
    Ok((remote, token))
}

/// Serve pastes of Talkr's selection, follow who owns the clipboard, and forget the session
/// when the desktop ends it (the user revoked access).
async fn watch(remote: Arc<Remote>) {
    let closed = match remote.session.receive_closed().await {
        Ok(stream) => stream,
        Err(e) => {
            log::warn!("dictation: cannot watch the keyboard session ({})", describe(&e));
            return;
        }
    };
    let mut closed = std::pin::pin!(closed);
    let (mut transfers, mut owners) = match &remote.clipboard {
        Some(c) => (
            c.receive_selection_transfer::<RemoteDesktop>().await.ok().map(Box::pin),
            c.receive_selection_owner_changed::<RemoteDesktop>().await.ok().map(Box::pin),
        ),
        None => (None, None),
    };
    loop {
        tokio::select! {
            _ = closed.next() => break,
            _ = remote.ended.notified() => return,
            Some((_, mime, serial)) = next(&mut transfers) => remote.serve(mime, serial).await,
            Some((_, change)) = next(&mut owners) => {
                let ours = change.session_is_owner() == Some(true);
                remote.owns_selection.store(ours, Ordering::SeqCst);
                if !ours {
                    *lock(&remote.offer) = None;
                    *lock(&remote.foreign_types) = Some(change.mime_types().to_vec());
                }
            }
        }
    }
    log::info!("dictation: the desktop ended keyboard access");
    let mut slot = lock(&SLOT);
    if slot.active.as_ref().is_some_and(|a| a.id == remote.id) {
        slot.active = None;
    }
    CHANGED.notify_all();
}

/// The next item of a stream that may not exist (then: never).
async fn next<S: futures_util::Stream + Unpin>(stream: &mut Option<S>) -> Option<S::Item> {
    match stream {
        Some(s) => s.next().await,
        None => std::future::pending().await,
    }
}

// ---- the restore token ----

fn token_path() -> Option<PathBuf> {
    let base = match std::env::var_os("TALKR_HOME").filter(|v| !v.is_empty()) {
        Some(v) => {
            let path = PathBuf::from(v);
            if path.is_absolute() {
                path
            } else {
                std::env::current_dir().ok()?.join(path)
            }
        }
        None => dirs::home_dir()?,
    };
    Some(token_file(&base))
}

/// Next to Talkr's other cached state (`AppPaths::cache`).
fn token_file(base: &Path) -> PathBuf {
    crate::paths::AppPaths::under(base).cache.join("wayland-keyboard-access.token")
}

fn load_token() -> Option<String> {
    let text = std::fs::read_to_string(token_path()?).ok()?;
    parse_token(&text)
}

fn parse_token(text: &str) -> Option<String> {
    let token = text.trim();
    (!token.is_empty() && token.len() <= 512 && !token.chars().any(char::is_control)).then(|| token.to_string())
}

fn save_token(token: &str) {
    use std::os::unix::fs::OpenOptionsExt;
    let Some(path) = token_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // It lets Talkr type without asking: readable by the user alone.
    let written = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)
        .and_then(|mut f| f.write_all(token.as_bytes()));
    if let Err(e) = written {
        log::warn!("dictation: could not save the keyboard access token: {}", e);
    }
}

fn forget_token() {
    if let Some(path) = token_path() {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_checked() {
        assert_eq!(parse_token("  abc-123\n"), Some("abc-123".into()));
        assert_eq!(parse_token(""), None);
        assert_eq!(parse_token(" \n"), None);
        assert_eq!(parse_token("a\u{1}b"), None);
        assert_eq!(parse_token(&"x".repeat(600)), None);
    }

    #[test]
    fn the_token_lives_in_the_cache() {
        let path = token_file(Path::new("/home/u"));
        assert_eq!(path, Path::new("/home/u/.talkr/cache/wayland-keyboard-access.token"));
    }

    #[test]
    fn pastes_get_the_right_type() {
        let offer: Contents = vec![("text/html".into(), b"<b>x</b>".to_vec()), ("text/plain;charset=utf-8".into(), b"x".to_vec())];
        assert_eq!(pick(&offer, "text/html"), Some(0));
        assert_eq!(pick(&offer, "text/plain;charset=utf-8"), Some(1));
        // Any text type gets the text.
        assert_eq!(pick(&offer, "UTF8_STRING"), Some(1));
        assert_eq!(pick(&offer, "text/plain"), Some(1));
        assert_eq!(pick(&offer, "image/png"), None);
        assert_eq!(pick(&Vec::new(), "text/plain"), None);
    }

    #[test]
    fn a_pipe_is_read_whole() {
        // How the portal hands over clipboard data.
        let (reader, mut writer) = std::io::pipe().unwrap();
        writer.write_all(b"hello").unwrap();
        drop(writer);
        let file = std::fs::File::from(std::os::fd::OwnedFd::from(reader));
        assert_eq!(read_capped(file, 5), Some(b"hello".to_vec()));
    }
}
