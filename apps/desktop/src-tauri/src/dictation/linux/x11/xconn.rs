//! Connections to the X server. The listener and the clipboard keep their own (each has an event
//! loop); everything else shares one, opened on first use and reopened after the server drops it.

use std::sync::{Arc, Mutex};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt as _, CreateWindowAux, EventMask, Window, WindowClass};
use x11rb::rust_connection::RustConnection;

x11rb::atom_manager! {
    pub Atoms: AtomsCookie {
        CLIPBOARD,
        UTF8_STRING,
        TEXT,
        TARGETS,
        INCR,
        TEXT_PLAIN_UTF8: b"text/plain;charset=utf-8",
        TEXT_PLAIN: b"text/plain",
        _NET_ACTIVE_WINDOW,
        _NET_SUPPORTED,
        _NET_WM_PID,
        _TALKR_SELECTION,
        _TALKR_WAKE,
    }
}

pub struct Display {
    pub conn: RustConnection,
    pub root: Window,
    pub atoms: Atoms,
}

impl Display {
    pub fn open() -> Result<Self, String> {
        let (conn, screen) = x11rb::connect(None).map_err(|e| format!("Could not connect to the X server: {}", e))?;
        let root = conn.setup().roots.get(screen).map(|s| s.root).ok_or("The X server has no such screen")?;
        let atoms = Atoms::new(&conn).map_err(|e| e.to_string())?.reply().map_err(|e| e.to_string())?;
        Ok(Self { conn, root, atoms })
    }

    /// An unmapped window of this connection's own, for selections and wake-ups.
    pub fn helper_window(&self, events: EventMask) -> Result<Window, String> {
        let id = self.conn.generate_id().map_err(|e| e.to_string())?;
        self.conn
            .create_window(
                0, // copy the parent's depth
                id,
                self.root,
                -10,
                -10,
                1,
                1,
                0,
                WindowClass::INPUT_OUTPUT,
                0, // copy the parent's visual
                &CreateWindowAux::new().event_mask(events).override_redirect(1),
            )
            .map_err(|e| e.to_string())?;
        self.conn.flush().map_err(|e| e.to_string())?;
        Ok(id)
    }
}

static SHARED: Mutex<Option<Arc<Display>>> = Mutex::new(None);

/// The shared connection, opened if needed.
pub fn shared() -> Option<Arc<Display>> {
    let mut slot = SHARED.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(display) = slot.as_ref() {
        return Some(display.clone());
    }
    match Display::open() {
        Ok(display) => {
            let display = Arc::new(display);
            *slot = Some(display.clone());
            Some(display)
        }
        Err(e) => {
            log::warn!("{}", e);
            None
        }
    }
}

/// Run `f` on the shared connection; a failure drops it, so the next call reconnects.
pub fn with<T>(f: impl FnOnce(&Display) -> Result<T, String>) -> Option<T> {
    let display = shared()?;
    match f(&display) {
        Ok(value) => Some(value),
        Err(e) => {
            log::debug!("X request failed: {}", e);
            if display.conn.flush().is_err() {
                *SHARED.lock().unwrap_or_else(|e| e.into_inner()) = None;
            }
            None
        }
    }
}

/// Shorthand for turning x11rb's errors into strings.
pub trait Fail<T> {
    fn x(self) -> Result<T, String>;
}

impl<T, E: std::fmt::Display> Fail<T> for Result<T, E> {
    fn x(self) -> Result<T, String> {
        self.map_err(|e| e.to_string())
    }
}
