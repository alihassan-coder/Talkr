//! The floating pill that shows dictation's state. Its window is created hidden at launch, so
//! showing it is instant (a webview takes a few hundred milliseconds to start); it never takes
//! focus from the app being typed in, and lets clicks through except while it shows buttons.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use super::backend::Backend;
use super::settings::OverlayPosition;
use super::sys::{Os, Target};

pub const LABEL: &str = "overlay";
/// Logical size of the overlay window. The pill is drawn inside it, with room for its glow.
pub const SIZE: (f64, f64) = (460.0, 96.0);
const EVENT_STATE: &str = "dictation://overlay";
const EVENT_LEVEL: &str = "dictation://level";

/// What the pill shows. Each change bumps a generation, so a delayed hide never hides a newer
/// state.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum State {
    Hidden,
    #[serde(rename_all = "camelCase")]
    Listening {
        session: u64,
        locked: bool,
        app: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Working {
        session: u64,
        label: String,
    },
    #[serde(rename_all = "camelCase")]
    Done {
        session: u64,
        preview: String,
        app: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Notice {
        session: u64,
        tone: Tone,
        title: String,
        detail: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Cancelled {
        session: u64,
    },
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Tone {
    Info,
    Warn,
    Error,
}

#[derive(Clone)]
pub struct Overlay {
    app: AppHandle,
    generation: Arc<AtomicU64>,
    /// Held while a state is decided and sent, so "show this unless something newer is up" and
    /// delayed hides cannot interleave with a new dictation's pill.
    gate: Arc<Mutex<()>>,
}

impl Overlay {
    /// Create the hidden overlay window. Transparent, undecorated, always on top, never focused.
    pub fn create(app: &AppHandle) -> tauri::Result<Self> {
        let builder = tauri::WebviewWindowBuilder::new(app, LABEL, tauri::WebviewUrl::App("overlay.html".into()))
            .title("Talkr dictation")
            .inner_size(SIZE.0, SIZE.1)
            .decorations(false)
            .shadow(false)
            .always_on_top(true)
            .visible_on_all_workspaces(true)
            .skip_taskbar(true)
            .resizable(false)
            .maximizable(false)
            .minimizable(false)
            .focused(false)
            .focusable(false)
            .visible(false);
        // macOS needs the private-API feature for transparent windows (enabled in Cargo.toml).
        let window = builder.transparent(true).build()?;
        window.set_ignore_cursor_events(true)?;
        Ok(Self { app: app.clone(), generation: Arc::new(AtomicU64::new(0)), gate: Arc::new(Mutex::new(())) })
    }

    pub fn window(&self) -> Option<tauri::WebviewWindow> {
        self.app.get_webview_window(LABEL)
    }

    fn gate(&self) -> MutexGuard<'_, ()> {
        self.gate.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Show `state`. Returns its generation, for [`hide_after`](Self::hide_after).
    pub fn set(&self, state: State) -> u64 {
        let _gate = self.gate();
        self.emit(state)
    }

    fn emit(&self, state: State) -> u64 {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let _ = self.app.emit_to(LABEL, EVENT_STATE, &state);
        generation
    }

    /// Show `state` unless `busy` is set (a newer dictation owns the pill), deciding and showing
    /// in one step.
    pub fn set_unless(&self, busy: &AtomicBool, state: State) -> Option<u64> {
        let _gate = self.gate();
        (!busy.load(Ordering::SeqCst)).then(|| self.emit(state))
    }

    pub fn level(&self, session: u64, level: f32) {
        #[derive(Clone, Serialize)]
        struct Level {
            session: u64,
            level: f32,
        }
        let _ = self.app.emit_to(LABEL, EVENT_LEVEL, Level { session, level });
    }

    /// Show the window near `anchor` (the target), without taking focus.
    pub fn show(&self, anchor: Option<&Target>, position: OverlayPosition) {
        if let Some(window) = self.window() {
            Os::show_overlay(&window, anchor, SIZE, position);
        }
    }

    /// Clickable while the pill shows buttons (hands-free), click-through otherwise.
    pub fn set_interactive(&self, interactive: bool) {
        if let Some(window) = self.window() {
            let _ = window.set_ignore_cursor_events(!interactive);
        }
    }

    /// Fade out and hide after `delay`, unless the pill has shown anything after `generation`.
    pub fn hide_after(&self, generation: u64, delay: Duration) {
        let this = self.clone();
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            let hidden = {
                let _gate = this.gate();
                if this.generation.load(Ordering::SeqCst) != generation {
                    return;
                }
                this.emit(State::Hidden)
            };
            // Let the exit animation play before the window goes.
            std::thread::sleep(Duration::from_millis(320));
            let _gate = this.gate();
            if this.generation.load(Ordering::SeqCst) == hidden {
                this.set_interactive(false);
                if let Some(window) = this.window() {
                    Os::hide_overlay(&window);
                }
            }
        });
    }

    /// Fade out and hide after `delay`, unless the pill changes before then.
    pub fn hide_latest_after(&self, delay: Duration) {
        self.hide_after(self.generation.load(Ordering::SeqCst), delay);
    }
}

/// Place and show the overlay with Tauri's own window calls: centred on the monitor with the
/// mouse (or the primary one), above the bottom edge of its work area or below the top. The
/// default for systems without a better native way.
pub fn show_portable(window: &tauri::WebviewWindow, logical: (f64, f64), position: OverlayPosition) {
    let app = window.app_handle();
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    if let Some(monitor) = monitor {
        let scale = monitor.scale_factor();
        let area = monitor.work_area();
        let (w, h) = ((logical.0 * scale).round() as i32, (logical.1 * scale).round() as i32);
        let margin = (14.0 * scale).round() as i32;
        let x = area.position.x + (area.size.width as i32 - w) / 2;
        let y = match position {
            OverlayPosition::Bottom => area.position.y + area.size.height as i32 - h - margin,
            OverlayPosition::Top => area.position.y + margin,
        };
        let _ = window.set_size(tauri::PhysicalSize::new(w as u32, h as u32));
        let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
    }
    let _ = window.set_always_on_top(true);
    let _ = window.show();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_serialize_for_the_pill() {
        let json = serde_json::to_string(&State::Listening { session: 3, locked: true, app: Some("Slack".into()) }).unwrap();
        assert_eq!(json, r#"{"kind":"listening","session":3,"locked":true,"app":"Slack"}"#);
        let json = serde_json::to_string(&State::Notice {
            session: 1,
            tone: Tone::Warn,
            title: "Copied".into(),
            detail: None,
        })
        .unwrap();
        assert_eq!(json, r#"{"kind":"notice","session":1,"tone":"warn","title":"Copied","detail":null}"#);
        assert_eq!(serde_json::to_string(&State::Hidden).unwrap(), r#"{"kind":"hidden"}"#);
    }
}
