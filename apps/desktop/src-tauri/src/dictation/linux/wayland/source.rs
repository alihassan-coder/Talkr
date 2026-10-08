//! Talkr's own data-control source for a paste (wlr protocol: wlroots compositors, KDE). Unlike
//! wl-clipboard-rs, which serves a copy from a thread Talkr cannot see into, this one counts the
//! apps' requests, so Talkr knows when the pasted text was read: it puts the user's clipboard back
//! only then, however slow the app, and does not claim a paste nobody read (a terminal that
//! pasted something else, an app that ignored the keys).
//!
//! It serves from a thread of its own until another app takes the clipboard. A compositor with
//! only the newer ext protocol gets the wl-clipboard-rs copy and a fixed wait instead.

use std::io::{ErrorKind, Write};
use std::os::fd::OwnedFd;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::{event_created_child, Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols_wlr::data_control::v1::client::zwlr_data_control_device_v1::{self, ZwlrDataControlDeviceV1};
use wayland_protocols_wlr::data_control::v1::client::zwlr_data_control_manager_v1::ZwlrDataControlManagerV1;
use wayland_protocols_wlr::data_control::v1::client::zwlr_data_control_offer_v1::ZwlrDataControlOfferV1;
use wayland_protocols_wlr::data_control::v1::client::zwlr_data_control_source_v1::{self, ZwlrDataControlSourceV1};
use super::clipboard::{Contents, SENSITIVE_HINT};

/// How long writing one answer may take (the reading app may be slow, or gone).
const WRITE_TIMEOUT: Duration = Duration::from_secs(3);

struct Shared {
    requests: AtomicU64,
    /// Still the clipboard: no other app took it.
    live: AtomicBool,
}

/// What Talkr put on the clipboard for one paste, while it serves it.
pub struct Offer {
    shared: Arc<Shared>,
}

impl Offer {
    /// The apps' requests for it so far (a clipboard manager checking the hint is not counted).
    pub fn requests(&self) -> u64 {
        self.shared.requests.load(Ordering::SeqCst)
    }

    /// It is still the clipboard.
    pub fn live(&self) -> bool {
        self.shared.live.load(Ordering::SeqCst)
    }
}

/// Make `contents` the clipboard (and the primary selection, with `primary`), served from a
/// thread of its own. `None` when the compositor has no wlr data-control (or it failed).
pub fn offer(contents: Contents, primary: bool) -> Option<Offer> {
    let shared = Arc::new(Shared { requests: AtomicU64::new(0), live: AtomicBool::new(true) });
    let (ready_tx, ready) = flume::bounded::<()>(1);
    let state = State { contents: contents.into_iter().map(|(t, d)| (t, Arc::from(d))).collect(), shared: shared.clone() };
    std::thread::Builder::new()
        .name("talkr-clipboard-offer".into())
        .spawn(move || {
            let live = state.shared.clone();
            match catch_unwind(AssertUnwindSafe(|| serve(state, primary, ready_tx))) {
                Ok(Ok(())) => {}
                Ok(Err(e)) => log::info!("dictation: the data-control clipboard stopped: {}", e),
                Err(_) => log::warn!("dictation: the data-control clipboard failed"),
            }
            live.live.store(false, Ordering::SeqCst);
        })
        .ok()?;
    ready.recv_timeout(Duration::from_secs(2)).ok()?;
    Some(Offer { shared })
}

struct State {
    contents: Vec<(String, Arc<[u8]>)>,
    shared: Arc<Shared>,
}

fn serve(mut state: State, primary: bool, ready: flume::Sender<()>) -> Result<(), String> {
    let conn = Connection::connect_to_env().map_err(|e| format!("no Wayland connection ({:?})", e))?;
    let (globals, mut queue) = registry_queue_init::<State>(&conn).map_err(|e| format!("{:?}", e))?;
    let qh = queue.handle();
    let manager: ZwlrDataControlManagerV1 = globals.bind(&qh, 1..=2, ()).map_err(|e| format!("no wlr data-control ({:?})", e))?;
    let primary = primary && manager.version() >= 2;
    let registry = globals.registry();
    let seats: Vec<WlSeat> = globals.contents().with_list(|list| {
        list.iter()
            .filter(|g| g.interface == WlSeat::interface().name && g.version >= 2)
            .map(|g| -> WlSeat { registry.bind(g.name, 2, &qh, ()) })
            .collect()
    });
    if seats.is_empty() {
        return Err("no seat".into());
    }
    let mut sources = Vec::new();
    for seat in &seats {
        let device = manager.get_data_device(seat, &qh, ());
        for is_primary in [false, true] {
            if is_primary && !primary {
                continue;
            }
            let source = manager.create_data_source(&qh, is_primary);
            for (mime, _) in &state.contents {
                source.offer(mime.clone());
            }
            if is_primary {
                device.set_primary_selection(Some(&source));
            } else {
                device.set_selection(Some(&source));
            }
            sources.push(source);
        }
    }
    queue.roundtrip(&mut state).map_err(|e| format!("{:?}", e))?;
    let _ = ready.send(());
    while sources.iter().any(|s| s.is_alive()) {
        queue.blocking_dispatch(&mut state).map_err(|e| format!("{:?}", e))?;
    }
    Ok(())
}

/// Write `data` for a reader, off the dispatch thread (the pipe may be non-blocking, and the
/// reader slow).
fn answer(fd: OwnedFd, data: Arc<[u8]>) {
    let _ = std::thread::Builder::new().name("talkr-clipboard-write".into()).spawn(move || {
        let mut file = std::fs::File::from(fd);
        let deadline = Instant::now() + WRITE_TIMEOUT;
        let mut rest: &[u8] = &data;
        while !rest.is_empty() && Instant::now() < deadline {
            match file.write(rest) {
                Ok(0) => break,
                Ok(n) => rest = &rest[n..],
                Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::Interrupted => {
                    std::thread::sleep(Duration::from_millis(2));
                }
                Err(_) => break,
            }
        }
    });
}

impl Dispatch<ZwlrDataControlSourceV1, bool> for State {
    fn event(
        state: &mut Self,
        source: &ZwlrDataControlSourceV1,
        event: <ZwlrDataControlSourceV1 as Proxy>::Event,
        primary: &bool,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_data_control_source_v1::Event::Send { mime_type, fd } => {
                let Some((_, data)) = state.contents.iter().find(|(t, _)| *t == mime_type) else { return };
                if mime_type != SENSITIVE_HINT {
                    state.shared.requests.fetch_add(1, Ordering::SeqCst);
                }
                answer(fd, data.clone());
            }
            zwlr_data_control_source_v1::Event::Cancelled => {
                source.destroy();
                if !*primary {
                    state.shared.live.store(false, Ordering::SeqCst);
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwlrDataControlDeviceV1, ()> for State {
    fn event(
        _: &mut Self,
        device: &ZwlrDataControlDeviceV1,
        event: <ZwlrDataControlDeviceV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            // Other apps' selections: not needed.
            zwlr_data_control_device_v1::Event::DataOffer { id } => id.destroy(),
            zwlr_data_control_device_v1::Event::Finished => device.destroy(),
            _ => {}
        }
    }

    event_created_child!(State, ZwlrDataControlDeviceV1, [
        zwlr_data_control_device_v1::EVT_DATA_OFFER_OPCODE => (ZwlrDataControlOfferV1, ()),
    ]);
}

impl Dispatch<ZwlrDataControlOfferV1, ()> for State {
    fn event(_: &mut Self, _: &ZwlrDataControlOfferV1, _: <ZwlrDataControlOfferV1 as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<ZwlrDataControlManagerV1, ()> for State {
    fn event(_: &mut Self, _: &ZwlrDataControlManagerV1, _: <ZwlrDataControlManagerV1 as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<WlSeat, ()> for State {
    fn event(_: &mut Self, _: &WlSeat, _: <WlSeat as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_are_written_whole() {
        let (reader, writer) = std::io::pipe().unwrap();
        let data: Arc<[u8]> = Arc::from(vec![7u8; 200_000]);
        answer(OwnedFd::from(writer), data.clone());
        let file = std::fs::File::from(OwnedFd::from(reader));
        assert_eq!(super::super::clipboard::read_capped(file, 300_000).as_deref(), Some(&data[..]));
    }
}
