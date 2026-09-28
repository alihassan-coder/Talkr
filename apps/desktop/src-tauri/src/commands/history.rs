use tauri::{command, State};
use crate::lib::AppState;
use crate::error::{AppError, Result};
use crate::db::{list_history, get_history, delete_history, toggle_favorite, clear_history, HistoryListResult, HistoryItem};
use crate::paths::AppPaths;

#[command]
pub async fn history_list(
    state: State<'_, AppState>,
    cursor: Option<i64>,
    query: Option<String>,
    kind: Option<String>,
    favorites_only: bool,
) -> Result<HistoryListResult> {
    let kind = kind.and_then(|k| match k.as_str() {
        "tts" => Some(crate::db::HistoryKind::Tts),
        "stt" => Some(crate::db::HistoryKind::Stt),
        _ => None,
    });

    list_history(&state.paths, cursor, query, kind, favorites_only, 50)
}

#[command]
pub async fn history_get(state: State<'_, AppState>, id: String) -> Result<Option<HistoryItem>> {
    get_history(&state.paths, &id)
}

#[command]
pub async fn history_delete(state: State<'_, AppState>, id: String) -> Result<()> {
    let item = get_history(&state.paths, &id)?.ok_or_else(|| AppError::NotFound("History item not found".into()))?;

    if let Some(audio_path) = item.audio_path {
        let full_path = state.paths.home.join(&audio_path);
        if full_path.exists() {
            std::fs::remove_file(full_path).ok();
        }
    }

    delete_history(&state.paths, &id)
}

#[command]
pub async fn history_toggle_favorite(state: State<'_, AppState>, id: String) -> Result<bool> {
    toggle_favorite(&state.paths, &id)
}

#[command]
pub async fn history_export(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    id: String,
    format: String,
) -> Result<String> {
    use crate::dialog::DialogExt;

    let item = get_history(&state.paths, &id)?.ok_or_else(|| AppError::NotFound("History item not found".into()))?;

    let content = match format.as_str() {
        "txt" => item.text,
        "srt" => {
            if let Some(segments_json) = item.segments_json {
                let segments: Vec<crate::db::TranscriptSegment> = serde_json::from_str(&segments_json)?;
                let mut srt = String::new();
                for (i, seg) in segments.iter().enumerate() {
                    srt.push_str(&format!("{}\n{} --> {}\n{}\n\n",
                        i + 1,
                        format_timestamp(seg.start_ms),
                        format_timestamp(seg.end_ms),
                        seg.text
                    ));
                }
                srt
            } else {
                item.text
            }
        },
        "wav" => {
            if let Some(audio_path) = item.audio_path {
                let full_path = state.paths.home.join(&audio_path);
                if full_path.exists() {
                    let saved_path = app.dialog().file()
                        .add_filter("WAV Audio", &["wav"])
                        .set_file_name(format!("{}.wav", item.title))
                        .blocking_save_file()
                        .ok_or_else(|| AppError::Audio("Save cancelled".into()))?;

                    std::fs::copy(&full_path, &saved_path)?;
                    return Ok(saved_path.to_string_lossy().to_string());
                }
            }
            return Err(AppError::NotFound("Audio file not found".into()));
        },
        _ => return Err(AppError::Validation("Unsupported format".into())),
    };

    let saved_path = app.dialog().file()
        .add_filter(&format.to_uppercase(), &[format.as_str()])
        .set_file_name(format!("{}.{}", item.title, format))
        .blocking_save_file()
        .ok_or_else(|| AppError::Audio("Save cancelled".into()))?;

    std::fs::write(&saved_path, content)?;
    Ok(saved_path.to_string_lossy().to_string())
}

#[command]
pub async fn history_clear(state: State<'_, AppState>) -> Result<usize> {
    clear_history(&state.paths)
}

fn format_timestamp(ms: i64) -> String {
    let hours = ms / 3_600_000;
    let minutes = (ms % 3_600_000) / 60_000;
    let seconds = (ms % 60_000) / 1000;
    let millis = ms % 1000;
    format!("{:02}:{:02}:{:02},{:03}", hours, minutes, seconds, millis)
}