use tauri::{command, AppHandle, Manager, State};
use crate::audio::record::{list_inputs, InputDevice};
use crate::dictation::{Dictation, Status};
use crate::error::Result;
use crate::AppState;

/// Whether dictation works here, is listening, and which model it will use.
#[command]
pub async fn dictation_status(app: AppHandle) -> Result<Status> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        Ok(app.state::<Dictation>().status(&state))
    })
    .await?
}

/// The microphones connected right now (the system default first).
#[command]
pub async fn list_microphones() -> Result<Vec<InputDevice>> {
    tauri::async_runtime::spawn_blocking(list_inputs).await?
}

/// Start (or stop) recording a new shortcut: the next key combination pressed anywhere is
/// reported through `dictation://captured` instead of reaching apps. Escape cancels.
#[command]
pub async fn dictation_capture_shortcut(dictation: State<'_, Dictation>, active: bool) -> Result<()> {
    dictation.capture(active);
    Ok(())
}

/// The pill's stop button: finish a hands-free dictation.
#[command]
pub async fn dictation_stop(dictation: State<'_, Dictation>) -> Result<()> {
    dictation.stop();
    Ok(())
}

/// The pill's cancel button.
#[command]
pub async fn dictation_cancel(dictation: State<'_, Dictation>) -> Result<()> {
    dictation.cancel();
    Ok(())
}

/// Insert the last dictation again where the cursor is.
#[command]
pub async fn dictation_paste_last(dictation: State<'_, Dictation>) -> Result<()> {
    dictation.paste_last();
    Ok(())
}

/// Load the dictation model now (the settings page calls this after a model change).
#[command]
pub async fn dictation_warm_up(app: AppHandle) -> Result<()> {
    crate::dictation::warm_up(&app);
    Ok(())
}
