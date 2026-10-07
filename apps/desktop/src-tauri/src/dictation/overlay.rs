//! The floating pill that shows dictation's state. Its window is created hidden at launch, so
//! showing it is instant (a webview takes a few hundred milliseconds to start); it never takes
//! focus from the app being typed in, and lets clicks through except while it shows buttons.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use super::settings::OverlayPosition;

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
}

impl Overlay {
    /// Create the hidden overlay window.
    pub fn create(app: &AppHandle) -> tauri::Result<Self> {
        #[cfg(windows)]
        {
            let window = tauri::WebviewWindowBuilder::new(app, LABEL, tauri::WebviewUrl::App("overlay.html".into()))
                .title("Talkr dictation")
                .inner_size(SIZE.0, SIZE.1)
                .decorations(false)
                .transparent(true)
                .shadow(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .resizable(false)
                .maximizable(false)
                .minimizable(false)
                .focused(false)
                .focusable(false)
                .visible(false)
                .build()?;
            window.set_ignore_cursor_events(true)?;
        }
        Ok(Self { app: app.clone(), generation: Arc::new(AtomicU64::new(0)) })
    }

    /// The overlay window's handle (Windows), to recognise it as the foreground window and to
    /// own clipboard calls.
    pub fn hwnd(&self) -> Option<isize> {
        #[cfg(windows)]
        {
            self.app.get_webview_window(LABEL).and_then(|w| w.hwnd().ok()).map(|h| h.0 as isize)
        }
        #[cfg(not(windows))]
        {
            None
        }
    }

    pub fn set(&self, state: State) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        let _ = self.app.emit_to(LABEL, EVENT_STATE, &state);
    }

    pub fn level(&self, session: u64, level: f32) {
        #[derive(Clone, Serialize)]
        struct Level {
            session: u64,
            level: f32,
        }
        let _ = self.app.emit_to(LABEL, EVENT_LEVEL, Level { session, level });
    }

    /// Show the window near `anchor` (the target window), without taking focus.
    pub fn show(&self, anchor: isize, position: OverlayPosition) {
        #[cfg(windows)]
        if let Some(hwnd) = self.hwnd() {
            super::win::show_overlay(hwnd, anchor, SIZE, position);
        }
        #[cfg(not(windows))]
        let _ = (anchor, position);
    }

    /// Clickable while the pill shows buttons (hands-free), click-through otherwise.
    pub fn set_interactive(&self, interactive: bool) {
        if let Some(window) = self.app.get_webview_window(LABEL) {
            let _ = window.set_ignore_cursor_events(!interactive);
        }
    }

    /// Fade out and hide after `delay`, unless the pill changed state meanwhile.
    pub fn hide_after(&self, delay: Duration) {
        let generation = self.generation.load(Ordering::SeqCst);
        let this = self.clone();
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            if this.generation.load(Ordering::SeqCst) != generation {
                return;
            }
            this.set(State::Hidden);
            let hidden = this.generation.load(Ordering::SeqCst);
            // Let the exit animation play before the window goes.
            std::thread::sleep(Duration::from_millis(320));
            if this.generation.load(Ordering::SeqCst) == hidden {
                this.hide_now();
            }
        });
    }

    pub fn hide_now(&self) {
        self.set_interactive(false);
        #[cfg(windows)]
        if let Some(hwnd) = self.hwnd() {
            super::win::hide_overlay(hwnd);
        }
    }
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
