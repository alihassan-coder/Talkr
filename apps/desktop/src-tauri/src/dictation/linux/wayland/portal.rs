//! Where the portal calls run, and what the desktop offers.
//!
//! The backend's methods are synchronous and called from Talkr's threads (some of them inside
//! tauri's own tokio runtime, where blocking on another runtime would panic), so portal work is
//! spawned on a small runtime of its own and waited for through a channel.

use std::future::Future;
use std::sync::{Condvar, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};
use ashpd::desktop::clipboard::Clipboard;
use ashpd::desktop::global_shortcuts::GlobalShortcuts;
use ashpd::desktop::remote_desktop::{DeviceType, RemoteDesktop};
use tokio::runtime::Runtime;
use super::plan::Availability;

/// The desktop file name Talkr is installed under (tauri.conf.json `identifier`): portals that
/// remember a decision per app (shortcuts, keyboard access) need to know which app is asking.
pub const APP_ID: &str = "app.talkr";

/// How long one probe of the portals may take. D-Bus starts xdg-desktop-portal on demand, which
/// can take a few seconds right after login.
const PROBE_TIMEOUT: Duration = Duration::from_secs(8);
/// How long `availability` waits for the first probe before answering "nothing yet".
const FIRST_WAIT: Duration = Duration::from_millis(2_500);
/// A probe that found something missing is repeated after this long (the portal may have been
/// starting up).
const RECHECK: Duration = Duration::from_secs(60);

pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn runtime() -> Option<&'static Runtime> {
    static RUNTIME: OnceLock<Option<Runtime>> = OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .thread_name("talkr-portal")
                .enable_all()
                .build()
                .map_err(|e| log::error!("dictation: could not start the portal runtime: {}", e))
                .ok()
        })
        .as_ref()
}

/// Run `future` on the portal runtime without waiting for it.
pub fn spawn(future: impl Future<Output = ()> + Send + 'static) -> bool {
    match runtime() {
        Some(rt) => {
            rt.spawn(future);
            true
        }
        None => false,
    }
}

/// Run `future` on the portal runtime and wait for its result, at most `timeout` (after which it
/// is dropped). `None` when it timed out or panicked. Never call this from the portal runtime.
pub fn block<T: Send + 'static>(timeout: Duration, future: impl Future<Output = T> + Send + 'static) -> Option<T> {
    let (tx, rx) = flume::bounded(1);
    let spawned = spawn(async move {
        if let Ok(value) = tokio::time::timeout(timeout, future).await {
            let _ = tx.send(value);
        }
    });
    if !spawned {
        return None;
    }
    // The sender goes away when the task ends without a value: no need to wait out the timeout.
    rx.recv_timeout(timeout + Duration::from_millis(250)).ok()
}

/// A portal error, in words for the settings page and the log.
pub fn describe(e: &ashpd::Error) -> String {
    match e {
        ashpd::Error::Response(ashpd::desktop::ResponseError::Cancelled) => "you declined it in the desktop's dialog".into(),
        ashpd::Error::PortalNotFound(name) => format!("the desktop has no {} portal", name),
        ashpd::Error::RequiresVersion(want, have) => format!("the desktop's portal is too old (version {} of {})", have, want),
        other => other.to_string(),
    }
}

struct Probe {
    result: Option<Availability>,
    checked: Option<Instant>,
    running: bool,
}

static PROBE: Mutex<Probe> = Mutex::new(Probe { result: None, checked: None, running: false });
static PROBED: Condvar = Condvar::new();

/// What this desktop offers. The first call waits briefly for the probe; later calls answer
/// from the cache (re-checking in the background when something was missing).
pub fn availability() -> Availability {
    availability_within(FIRST_WAIT)
}

/// Like `availability`, waiting up to the whole probe for the first answer: for decisions that
/// are not revisited (starting the shortcut listener), where "not probed yet" must not read as
/// "missing".
pub fn settled_availability() -> Availability {
    availability_within(PROBE_TIMEOUT + Duration::from_secs(1))
}

fn availability_within(first_wait: Duration) -> Availability {
    let mut probe = lock(&PROBE);
    let stale = match (probe.result, probe.checked) {
        (Some(av), Some(at)) => !complete(&av) && at.elapsed() >= RECHECK,
        _ => true,
    };
    if stale && !probe.running {
        probe.running = true;
        let started = std::thread::Builder::new().name("talkr-portal-probe".into()).spawn(|| {
            let found = run_probe();
            let mut probe = lock(&PROBE);
            probe.result = Some(found);
            probe.checked = Some(Instant::now());
            probe.running = false;
            PROBED.notify_all();
        });
        if started.is_err() {
            probe.running = false;
        }
    }
    let deadline = Instant::now() + first_wait;
    while probe.result.is_none() && probe.running {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        probe = PROBED.wait_timeout(probe, left).unwrap_or_else(|e| e.into_inner()).0;
    }
    probe.result.unwrap_or_default()
}

/// Everything a portal desktop can offer was found: nothing to look for again.
fn complete(av: &Availability) -> bool {
    av.shortcuts && av.keyboard
}

fn run_probe() -> Availability {
    let mut av = block(PROBE_TIMEOUT, probe_portals()).unwrap_or_default();
    match std::panic::catch_unwind(wl_clipboard_rs::utils::is_primary_selection_supported) {
        Ok(Ok(primary)) => {
            av.data_control = true;
            av.primary = primary;
        }
        Ok(Err(e)) => log::info!("dictation: no Wayland data-control clipboard ({})", e),
        Err(_) => log::warn!("dictation: checking the Wayland clipboard failed"),
    }
    log::info!("dictation: Wayland portals: {:?}", av);
    av
}

async fn probe_portals() -> Availability {
    register().await;
    let mut av = Availability::default();
    match GlobalShortcuts::new().await {
        Ok(_) => av.shortcuts = true,
        Err(e) => log::info!("dictation: no global-shortcuts portal ({})", describe(&e)),
    }
    match RemoteDesktop::new().await {
        Ok(portal) => match portal.available_device_types().await {
            Ok(types) => av.keyboard = types.contains(DeviceType::Keyboard),
            Err(e) => log::info!("dictation: the remote-desktop portal did not list its devices ({})", describe(&e)),
        },
        Err(e) => log::info!("dictation: no remote-desktop portal ({})", describe(&e)),
    }
    av.portal_clipboard = av.keyboard && Clipboard::new().await.is_ok();
    av
}

/// Tell the portals which app this is, before any other portal call on the connection (an app
/// outside Flatpak or Snap has no id otherwise). Once; a failure only costs the app's name in
/// the desktop's dialogs.
async fn register() {
    static REGISTERED: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();
    REGISTERED
        .get_or_init(|| async {
            match ashpd::AppID::try_from(APP_ID) {
                Ok(id) => {
                    if let Err(e) = ashpd::register_host_app(id).await {
                        log::info!("dictation: the portals did not register Talkr's app id ({})", describe(&e));
                    }
                }
                Err(e) => log::warn!("dictation: invalid app id {}: {}", APP_ID, e),
            }
        })
        .await;
}

/// Every portal call starts here, so the app id is registered first.
pub async fn ready() {
    register().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_complete_desktop_is_not_probed_again() {
        assert!(complete(&Availability { shortcuts: true, keyboard: true, ..Availability::default() }));
        assert!(!complete(&Availability { shortcuts: true, ..Availability::default() }));
        assert!(!complete(&Availability::default()));
    }

    #[test]
    fn block_returns_the_value_or_gives_up() {
        assert_eq!(block(Duration::from_secs(2), async { 7 }), Some(7));
        let started = Instant::now();
        let slow = block(Duration::from_millis(100), async {
            tokio::time::sleep(Duration::from_secs(5)).await;
            1
        });
        assert_eq!(slow, None);
        assert!(started.elapsed() < Duration::from_secs(2));
        // A panic in the task is a missing value, not a panic here.
        let panicked = block(Duration::from_secs(2), async { "x".parse::<u8>().expect("portal bug") });
        assert_eq!(panicked, None);
    }

    #[test]
    fn errors_read_well() {
        let cancelled = ashpd::Error::Response(ashpd::desktop::ResponseError::Cancelled);
        assert_eq!(describe(&cancelled), "you declined it in the desktop's dialog");
        assert!(describe(&ashpd::Error::RequiresVersion(2, 1)).contains("too old"));
    }
}
