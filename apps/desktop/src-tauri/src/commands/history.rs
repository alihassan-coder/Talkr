use tauri::{command, AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use crate::commands::{remove_owned_audio, resolve_path};
use crate::db::{clear_history, delete_history, get_history, list_history, toggle_favorite, HistoryItem, HistoryKind, HistoryListResult};
use crate::engines::TranscriptSegment;
use crate::error::{AppError, Result};
use crate::AppState;

const PAGE_SIZE: usize = 50;

/// List history newest-first, 50 per page. Pass the returned `nextCursor` to get the next page.
#[command]
pub async fn history_list(
    state: State<'_, AppState>,
    cursor: Option<i64>,
    query: Option<String>,
    kind: Option<String>,
    favorites_only: Option<bool>,
) -> Result<HistoryListResult> {
    let kind = kind.and_then(|k| match k.as_str() {
        "tts" => Some(HistoryKind::Tts),
        "stt" => Some(HistoryKind::Stt),
        _ => None,
    });
    let query = query.filter(|q| !q.trim().is_empty());

    list_history(&state.paths, cursor, query, kind, favorites_only.unwrap_or(false), PAGE_SIZE)
}

#[command]
pub async fn history_get(state: State<'_, AppState>, id: String) -> Result<Option<HistoryItem>> {
    get_history(&state.paths, &id)
}

#[command]
pub async fn history_delete(state: State<'_, AppState>, id: String) -> Result<()> {
    let item = get_history(&state.paths, &id)?.ok_or_else(|| AppError::NotFound("History item not found".into()))?;
    delete_history(&state.paths, &id)?;
    if let Some(audio_path) = item.audio_path {
        remove_owned_audio(&state.paths, &[audio_path]);
    }
    Ok(())
}

/// Toggle favorite; returns the new value.
#[command]
pub async fn history_toggle_favorite(state: State<'_, AppState>, id: String) -> Result<bool> {
    toggle_favorite(&state.paths, &id)
}

/// Export an item as "txt", "srt" or "wav" via a native save dialog.
/// Returns the saved path, or `null` if the user cancelled the dialog.
#[command]
pub async fn history_export(
    state: State<'_, AppState>,
    app: AppHandle,
    id: String,
    format: String,
) -> Result<Option<String>> {
    let item = get_history(&state.paths, &id)?.ok_or_else(|| AppError::NotFound("History item not found".into()))?;
    let base_name = sanitize_file_name(&item.title);

    enum Payload {
        Text(String),
        Copy(std::path::PathBuf),
    }

    let (payload, filter_name) = match format.as_str() {
        "txt" => (Payload::Text(item.text), "Text"),
        "srt" => {
            let segments: Vec<TranscriptSegment> = match item.segments_json.as_deref() {
                Some(json) => serde_json::from_str(json)?,
                None => Vec::new(),
            };
            if segments.is_empty() {
                return Err(AppError::Validation("This item has no timed segments to export as SRT".into()));
            }
            (Payload::Text(to_srt(&segments)), "SubRip subtitles")
        }
        "wav" => {
            let full_path = item
                .audio_path
                .as_deref()
                .map(|p| resolve_path(&state.paths, p))
                .filter(|p| p.is_file())
                .ok_or_else(|| AppError::NotFound("Audio file not found".into()))?;
            (Payload::Copy(full_path), "WAV audio")
        }
        _ => return Err(AppError::Validation(format!("Unsupported format: {}", format))),
    };

    let file_name = format!("{}.{}", base_name, format);
    let ext = format.clone();
    let filter_name = filter_name.to_string();
    let dialog_app = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        dialog_app
            .dialog()
            .file()
            .add_filter(filter_name, &[ext.as_str()])
            .set_file_name(file_name)
            .blocking_save_file()
    })
    .await?;

    let Some(picked) = picked else { return Ok(None) };
    let saved_path = picked
        .into_path()
        .map_err(|e| AppError::Path(e.to_string()))?;

    match payload {
        Payload::Text(content) => std::fs::write(&saved_path, content)?,
        Payload::Copy(src) => {
            std::fs::copy(&src, &saved_path)?;
        }
    }
    Ok(Some(saved_path.to_string_lossy().to_string()))
}

/// Delete all non-favorite history items (and their audio). Returns the number deleted.
#[command]
pub async fn history_clear(state: State<'_, AppState>) -> Result<usize> {
    let deleted = clear_history(&state.paths)?;
    remove_owned_audio(&state.paths, &deleted.audio_paths);
    Ok(deleted.count)
}

fn to_srt(segments: &[TranscriptSegment]) -> String {
    let mut srt = String::new();
    for (i, seg) in segments.iter().enumerate() {
        srt.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            i + 1,
            format_timestamp(seg.start_ms),
            format_timestamp(seg.end_ms),
            seg.text.trim()
        ));
    }
    srt
}

fn format_timestamp(ms: i64) -> String {
    let ms = ms.max(0);
    let hours = ms / 3_600_000;
    let minutes = (ms % 3_600_000) / 60_000;
    let seconds = (ms % 60_000) / 1000;
    let millis = ms % 1000;
    format!("{:02}:{:02}:{:02},{:03}", hours, minutes, seconds, millis)
}

fn sanitize_file_name(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || c.is_control() { '_' } else { c })
        .collect();
    let cleaned = cleaned.trim().trim_end_matches(['.', '…']).trim();
    if cleaned.is_empty() {
        "talkr-export".into()
    } else {
        cleaned.chars().take(80).collect()
    }
}
