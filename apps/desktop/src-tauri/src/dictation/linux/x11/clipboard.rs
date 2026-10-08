//! The X clipboard. A selection has no content of its own: its owner hands the text to whoever
//! asks. Talkr owns CLIPBOARD (and PRIMARY for xterm-style pastes) from a helper window on its
//! own thread, which answers those requests until another app takes ownership.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ConnectionExt as _, EventMask, PropMode, Property, SelectionNotifyEvent, SelectionRequestEvent, Window,
    SELECTION_NOTIFY_EVENT,
};
use x11rb::protocol::Event;
use x11rb::wrapper::ConnectionExt as _;
use super::xconn::{Display, Fail};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    Clipboard,
    Primary,
}

struct Server {
    display: Display,
    window: Window,
    /// What Talkr offers for each selection it owns.
    content: Mutex<HashMap<Atom, String>>,
    /// Requests answered with text, so a paste can wait until the app has read it.
    served: AtomicU64,
}

static SERVER: OnceLock<Option<Arc<Server>>> = OnceLock::new();

fn server() -> Option<Arc<Server>> {
    SERVER
        .get_or_init(|| match start() {
            Ok(server) => Some(server),
            Err(e) => {
                log::warn!("the clipboard is not available: {}", e);
                None
            }
        })
        .clone()
}

fn start() -> Result<Arc<Server>, String> {
    let display = Display::open()?;
    let window = display.helper_window(EventMask::PROPERTY_CHANGE)?;
    let server = Arc::new(Server { display, window, content: Mutex::new(HashMap::new()), served: AtomicU64::new(0) });
    let worker = server.clone();
    std::thread::Builder::new()
        .name("talkr-clipboard".into())
        .spawn(move || loop {
            match worker.display.conn.wait_for_event() {
                Ok(Event::SelectionRequest(request)) => {
                    if let Err(e) = worker.answer(&request) {
                        log::debug!("clipboard request failed: {}", e);
                    }
                }
                Ok(Event::SelectionClear(clear)) => {
                    worker.lock().remove(&clear.selection);
                }
                Ok(_) => {}
                Err(e) => {
                    log::warn!("the clipboard's X connection closed: {}", e);
                    return;
                }
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(server)
}

impl Server {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<Atom, String>> {
        self.content.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn atom(&self, selection: Selection) -> Atom {
        match selection {
            Selection::Clipboard => self.display.atoms.CLIPBOARD,
            Selection::Primary => AtomEnum::PRIMARY.into(),
        }
    }

    fn answer(&self, request: &SelectionRequestEvent) -> Result<(), String> {
        let conn = &self.display.conn;
        let atoms = &self.display.atoms;
        // Obsolete clients leave the property out: use the target's name, as ICCCM says.
        let property = if request.property == x11rb::NONE { request.target } else { request.property };
        let text = self.lock().get(&request.selection).cloned();
        let answered = match text {
            None => false,
            Some(_) if request.target == atoms.TARGETS => {
                let targets = [atoms.TARGETS, atoms.UTF8_STRING, atoms.TEXT_PLAIN_UTF8, atoms.TEXT, AtomEnum::STRING.into()];
                conn.change_property32(PropMode::REPLACE, request.requestor, property, AtomEnum::ATOM, &targets).x()?;
                true
            }
            Some(text) if [atoms.UTF8_STRING, atoms.TEXT_PLAIN_UTF8, atoms.TEXT].contains(&request.target) => {
                let kind = if request.target == atoms.TEXT { atoms.UTF8_STRING } else { request.target };
                conn.change_property8(PropMode::REPLACE, request.requestor, property, kind, text.as_bytes()).x()?;
                self.served.fetch_add(1, Ordering::SeqCst);
                true
            }
            Some(text) if request.target == u32::from(AtomEnum::STRING) || request.target == atoms.TEXT_PLAIN => {
                let latin1: Vec<u8> = text.chars().map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?')).collect();
                conn.change_property8(PropMode::REPLACE, request.requestor, property, AtomEnum::STRING, &latin1).x()?;
                self.served.fetch_add(1, Ordering::SeqCst);
                true
            }
            Some(_) => false,
        };
        let notify = SelectionNotifyEvent {
            response_type: SELECTION_NOTIFY_EVENT,
            sequence: 0,
            time: request.time,
            requestor: request.requestor,
            selection: request.selection,
            target: request.target,
            property: if answered { property } else { x11rb::NONE },
        };
        conn.send_event(false, request.requestor, EventMask::NO_EVENT, notify).x()?;
        conn.flush().x()
    }

    fn owns(&self, selection: Selection) -> bool {
        let atom = self.atom(selection);
        self.display.conn.get_selection_owner(atom).ok().and_then(|c| c.reply().ok()).is_some_and(|r| r.owner == self.window)
            && self.lock().contains_key(&atom)
    }
}

/// Offer `text` on `selection`. Returns whether Talkr now owns it.
pub fn put(selection: Selection, text: &str) -> bool {
    let Some(server) = server() else { return false };
    let atom = server.atom(selection);
    server.lock().insert(atom, text.to_string());
    let conn = &server.display.conn;
    if conn.set_selection_owner(server.window, atom, x11rb::CURRENT_TIME).is_err() || conn.flush().is_err() {
        return false;
    }
    let owned = server.owns(selection);
    if !owned {
        server.lock().remove(&atom);
    }
    owned
}

/// Whether Talkr still owns `selection` with exactly `text`: nobody copied anything since.
pub fn holds(selection: Selection, text: &str) -> bool {
    server().is_some_and(|s| s.owns(selection) && s.lock().get(&s.atom(selection)).is_some_and(|t| t == text))
}

/// Give up `selection`, leaving it empty, if Talkr owns it.
pub fn clear(selection: Selection) {
    let Some(server) = server() else { return };
    if server.owns(selection) {
        let atom = server.atom(selection);
        server.lock().remove(&atom);
        let _ = server.display.conn.set_selection_owner(x11rb::NONE, atom, x11rb::CURRENT_TIME);
        let _ = server.display.conn.flush();
    }
}

/// How many text requests Talkr has answered so far.
pub fn served() -> u64 {
    server().map_or(0, |s| s.served.load(Ordering::SeqCst))
}

/// What a selection holds now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Saved {
    /// Nobody owns it.
    Empty,
    Text(String),
}

/// Read `selection` as text. `None` when it holds something that is not text (an image, files)
/// or its owner does not answer: then it cannot be put back after a paste.
pub fn read(selection: Selection) -> Option<Saved> {
    match read_inner(selection) {
        Ok(saved) => saved,
        Err(e) => {
            log::debug!("could not read the clipboard: {}", e);
            None
        }
    }
}

fn read_inner(selection: Selection) -> Result<Option<Saved>, String> {
    // Its own connection: the answer comes as events, which nothing else should see.
    let d = Display::open()?;
    let atom = match selection {
        Selection::Clipboard => d.atoms.CLIPBOARD,
        Selection::Primary => AtomEnum::PRIMARY.into(),
    };
    if d.conn.get_selection_owner(atom).x()?.reply().x()?.owner == x11rb::NONE {
        return Ok(Some(Saved::Empty));
    }
    let window = d.helper_window(EventMask::PROPERTY_CHANGE)?;
    let property = d.atoms._TALKR_SELECTION;
    for target in [d.atoms.UTF8_STRING, AtomEnum::STRING.into()] {
        d.conn.convert_selection(window, atom, target, property, x11rb::CURRENT_TIME).x()?;
        d.conn.flush().x()?;
        let Some(notify) = wait(&d, Duration::from_millis(600), |e| match e {
            Event::SelectionNotify(n) if n.requestor == window => Some(n.property),
            _ => None,
        })?
        else {
            return Ok(None); // the owner did not answer
        };
        if notify == x11rb::NONE {
            continue; // refused this target
        }
        let reply = d.conn.get_property(true, window, property, AtomEnum::ANY, 0, u32::MAX / 4).x()?.reply().x()?;
        let bytes = if reply.type_ == d.atoms.INCR { read_incr(&d, window, property)? } else { reply.value };
        let text = if target == d.atoms.UTF8_STRING {
            String::from_utf8_lossy(&bytes).into_owned()
        } else {
            bytes.iter().map(|&b| char::from(b)).collect()
        };
        return Ok(Some(Saved::Text(text)));
    }
    Ok(None)
}

/// A large selection arrives in chunks (ICCCM's INCR): each one replaces the property, and an
/// empty one ends it.
fn read_incr(d: &Display, window: Window, property: Atom) -> Result<Vec<u8>, String> {
    let mut data = Vec::new();
    loop {
        let changed = wait(d, Duration::from_millis(1_500), |e| match e {
            Event::PropertyNotify(p) if p.window == window && p.atom == property && p.state == Property::NEW_VALUE => Some(()),
            _ => None,
        })?;
        if changed.is_none() {
            return Err("the clipboard owner stopped sending".into());
        }
        let chunk = d.conn.get_property(true, window, property, AtomEnum::ANY, 0, u32::MAX / 4).x()?.reply().x()?;
        if chunk.value.is_empty() {
            return Ok(data);
        }
        data.extend_from_slice(&chunk.value);
    }
}

/// Wait up to `limit` for an event `pick` accepts.
fn wait<T>(d: &Display, limit: Duration, mut pick: impl FnMut(&Event) -> Option<T>) -> Result<Option<T>, String> {
    let deadline = Instant::now() + limit;
    loop {
        while let Some(event) = d.conn.poll_for_event().x()? {
            if let Some(found) = pick(&event) {
                return Ok(Some(found));
            }
        }
        if Instant::now() >= deadline {
            return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
