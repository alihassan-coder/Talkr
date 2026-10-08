//! The tray icon: Talkr keeps running there while dictation is on, so the shortcut works with
//! the window closed.

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Wry};
use crate::dictation::{Dictation, EVENT_SETTINGS};
use crate::error::Result;
use crate::AppState;

/// The dictation check item, kept to follow the setting.
pub struct Tray {
    dictation: CheckMenuItem<Wry>,
}

fn show_main(app: &AppHandle) {
    crate::show_main_window(app);
}

/// Create the tray icon, and say whether there is one. Never fails the app: on Linux the
/// indicator library is loaded at run time, and libappindicator-sys panics (rather than returning
/// an error) when none is installed.
pub fn create(app: &AppHandle) -> bool {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| build(app))) {
        Ok(Ok(())) => true,
        Ok(Err(e)) => {
            log::error!("could not create the tray icon: {}", e);
            false
        }
        Err(_) => {
            log::error!("could not create the tray icon (on Linux, install libayatana-appindicator)");
            false
        }
    }
}

/// Whether Talkr has a tray icon to come back from (see [`create`]).
pub fn exists(app: &AppHandle) -> bool {
    app.try_state::<Tray>().is_some()
}

fn build(app: &AppHandle) -> tauri::Result<()> {
    let enabled = app.state::<AppState>().settings().dictation.enabled;
    let open = MenuItem::with_id(app, "open", "Open Talkr", true, None::<&str>)?;
    let dictation = CheckMenuItem::with_id(app, "dictation", "Dictation", true, enabled, None::<&str>)?;
    let paste = MenuItem::with_id(app, "copy-last", "Copy last dictation", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Talkr", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&open, &PredefinedMenuItem::separator(app)?, &dictation, &paste, &PredefinedMenuItem::separator(app)?, &quit],
    )?;
    let mut builder = TrayIconBuilder::with_id("talkr")
        .tooltip("Talkr")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "dictation" => {
                let on = !app.state::<AppState>().settings().dictation.enabled;
                if let Err(e) = set_dictation_enabled(app, on) {
                    log::warn!("could not switch dictation from the tray: {}", e);
                }
            }
            "copy-last" => app.state::<Dictation>().copy_last(),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    app.manage(Tray { dictation });
    Ok(())
}

/// Keep the tray's check mark in step with the setting.
pub fn sync(app: &AppHandle) {
    if let Some(tray) = app.try_state::<Tray>() {
        let enabled = app.state::<AppState>().settings().dictation.enabled;
        let _ = tray.dictation.set_checked(enabled);
    }
}

/// Turn dictation on or off, save it, and tell the window.
pub fn set_dictation_enabled(app: &AppHandle, enabled: bool) -> Result<()> {
    let state = app.state::<AppState>();
    let updated = {
        let mut current = state.settings();
        let mut updated = current.clone();
        updated.dictation.enabled = enabled;
        updated.save(&state.paths)?;
        *current = updated.clone();
        updated
    };
    let _ = app.emit(EVENT_SETTINGS, &updated);
    app.state::<Dictation>().reconfigure();
    sync(app);
    Ok(())
}
