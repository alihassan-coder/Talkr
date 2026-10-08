//! The macOS side of dictation. Not implemented yet: dictation reports itself unsupported here.

use crate::dictation::backend::{Backend, Capabilities, Delivery};
use crate::dictation::settings::{DictationSettings, Shortcut};
use crate::dictation::HotkeyEvent;

pub struct Os;

/// What had focus when the shortcut was pressed.
#[derive(Debug, Clone)]
pub struct Target;

impl Backend for Os {
    type Target = Target;

    fn capabilities() -> Capabilities {
        Capabilities {
            os: "macos",
            supported: false,
            hold_to_talk: false,
            modifier_only: false,
            records_shortcut: false,
            verifies_insertion: false,
            inserts_text: false,
            meta_key: "⌘",
            note: None,
        }
    }

    fn start_hotkeys(_events: flume::Sender<HotkeyEvent>) -> Result<(), String> {
        Err("Dictation is not available on macOS yet".into())
    }

    fn stop_hotkeys() {}

    fn hotkeys_running() -> bool {
        false
    }

    fn configure_hotkeys(_dictate: Option<&Shortcut>, _paste_last: Option<&Shortcut>) {}

    fn set_recording(_recording: bool) {}

    fn set_capturing(_capturing: bool) {}

    fn snapshot() -> Option<Self::Target> {
        None
    }

    fn app_label(_target: &Self::Target) -> Option<String> {
        None
    }

    fn app_id(_target: &Self::Target) -> Option<String> {
        None
    }

    fn deliver(
        _text: &str,
        _target: Option<&Self::Target>,
        _settings: &DictationSettings,
        _owner: Option<&tauri::WebviewWindow>,
    ) -> Delivery {
        Delivery::copied("Dictation is not available on macOS yet", false)
    }

    fn copy(_text: &str, _owner: Option<&tauri::WebviewWindow>) -> bool {
        false
    }
}
