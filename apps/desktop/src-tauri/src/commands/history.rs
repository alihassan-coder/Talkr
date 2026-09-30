//! History commands. SQLite calls are synchronous (and may wait up to the 5 s busy timeout on a
//! locked database), so every command runs its database and file work on Tokio's blocking pool
//! via `spawn_blocking`, never on an async worker thread.

use std::path::PathBuf;
use tauri::{command, AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use crate::commands::{remove_owned_audio, resolve_path};
use crate::db::{clear_history, delete_history, get_history, list_history, toggle_favorite, HistoryItem, HistoryKind, HistoryListResult};
use crate::paths::AppPaths;
use talkr_protocol::TranscriptSegment;
use crate::error::{AppError, Result};
use crate::AppState;

const PAGE_SIZE: usize = 50;

/// Run blocking work (SQLite, file I/O) off the async runtime.
async fn blocking<T, F>(f: F) -> Result<T>
where
    F: FnOnce() -> Result<T> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f).await?
}

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
    let paths = state.paths.clone();
    blocking(move || list_history(&paths, cursor, query, kind, favorites_only.unwrap_or(false), PAGE_SIZE)).await
}

#[command]
pub async fn history_get(state: State<'_, AppState>, id: String) -> Result<Option<HistoryItem>> {
    let paths = state.paths.clone();
    blocking(move || get_history(&paths, &id)).await
}

#[command]
pub async fn history_delete(state: State<'_, AppState>, id: String) -> Result<()> {
    let paths = state.paths.clone();
    blocking(move || {
        let item = get_history(&paths, &id)?.ok_or_else(|| AppError::NotFound("History item not found".into()))?;
        delete_history(&paths, &id)?;
        if let Some(audio_path) = item.audio_path {
            remove_owned_audio(&paths, &[audio_path]);
        }
        Ok(())
    })
    .await
}

/// Toggle favorite; returns the new value.
#[command]
pub async fn history_toggle_favorite(state: State<'_, AppState>, id: String) -> Result<bool> {
    let paths = state.paths.clone();
    blocking(move || toggle_favorite(&paths, &id)).await
}

/// What an export writes: text, or a copy of an audio file.
#[derive(Debug, PartialEq)]
enum Payload {
    Text(String),
    Copy(PathBuf),
}

/// An export ready to save: the content, the save-dialog filter name and the suggested file name.
#[derive(Debug)]
struct Export {
    payload: Payload,
    filter_name: &'static str,
    file_name: String,
}

/// Build the export of `item` as "txt", "srt" or "wav".
fn prepare_export(paths: &AppPaths, item: HistoryItem, format: &str) -> Result<Export> {
    let base_name = sanitize_file_name(&item.title);
    let (payload, filter_name) = match format {
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
                .map(|p| resolve_path(paths, p))
                .filter(|p| p.is_file())
                .ok_or_else(|| AppError::NotFound("Audio file not found".into()))?;
            (Payload::Copy(full_path), "WAV audio")
        }
        _ => return Err(AppError::Validation(format!("Unsupported format: {}", format))),
    };
    Ok(Export { payload, filter_name, file_name: format!("{}.{}", base_name, format) })
}

fn write_export(payload: Payload, dest: &std::path::Path) -> Result<()> {
    match payload {
        Payload::Text(content) => std::fs::write(dest, content)?,
        Payload::Copy(src) => {
            // Exporting onto the source itself would truncate it.
            if src.canonicalize().ok() == dest.canonicalize().ok() && dest.exists() {
                return Ok(());
            }
            std::fs::copy(&src, dest)?;
        }
    }
    Ok(())
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
    let paths = state.paths.clone();
    let export = blocking(move || {
        let item = get_history(&paths, &id)?.ok_or_else(|| AppError::NotFound("History item not found".into()))?;
        prepare_export(&paths, item, &format)
    })
    .await?;

    let ext = export.file_name.rsplit('.').next().unwrap_or_default().to_string();
    let file_name = export.file_name;
    let filter_name = export.filter_name.to_string();
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

    let payload = export.payload;
    let dest = saved_path.clone();
    blocking(move || write_export(payload, &dest)).await?;
    Ok(Some(saved_path.to_string_lossy().to_string()))
}

/// Delete all non-favorite history items (and their audio). Returns the number deleted.
#[command]
pub async fn history_clear(state: State<'_, AppState>) -> Result<usize> {
    let paths = state.paths.clone();
    blocking(move || {
        let deleted = clear_history(&paths)?;
        remove_owned_audio(&paths, &deleted.audio_paths);
        Ok(deleted.count)
    })
    .await
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

/// Windows device names that cannot be used as a file name, with or without an extension.
const RESERVED_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
    "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

fn sanitize_file_name(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || c.is_control() { '_' } else { c })
        .collect();
    let cleaned = cleaned.trim().trim_end_matches(['.', '…']).trim();
    let cleaned: String = cleaned.chars().take(80).collect();
    let cleaned = cleaned.trim_end_matches(['.', ' ']).to_string();
    let stem = cleaned.split('.').next().unwrap_or("").to_ascii_uppercase();
    if cleaned.is_empty() || RESERVED_NAMES.contains(&stem.as_str()) {
        "talkr-export".into()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start_ms: i64, end_ms: i64, text: &str) -> TranscriptSegment {
        TranscriptSegment { start_ms, end_ms, text: text.into() }
    }

    fn item(title: &str) -> HistoryItem {
        HistoryItem {
            id: "id".into(),
            kind: HistoryKind::Stt,
            created_at: 0,
            title: title.into(),
            text: "Hello world.".into(),
            audio_path: None,
            duration_ms: None,
            model_id: "m".into(),
            voice_id: None,
            language: None,
            device: "cpu".into(),
            processing_ms: 0,
            favorite: false,
            segments_json: None,
        }
    }

    fn temp_paths() -> (tempfile::TempDir, AppPaths) {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::init_at(dir.path()).unwrap();
        (dir, paths)
    }

    #[test]
    fn timestamps() {
        assert_eq!(format_timestamp(0), "00:00:00,000");
        assert_eq!(format_timestamp(1_234), "00:00:01,234");
        assert_eq!(format_timestamp(61_005), "00:01:01,005");
        assert_eq!(format_timestamp(3_600_000 + 59 * 60_000 + 59_999), "01:59:59,999");
        assert_eq!(format_timestamp(100 * 3_600_000), "100:00:00,000");
        assert_eq!(format_timestamp(-5), "00:00:00,000");
    }

    #[test]
    fn srt_numbering_and_layout() {
        let srt = to_srt(&[seg(0, 1500, "  Hello there. "), seg(1500, 3_723_004, "General Kenobi!")]);
        assert_eq!(
            srt,
            "1\n00:00:00,000 --> 00:00:01,500\nHello there.\n\n2\n00:00:01,500 --> 01:02:03,004\nGeneral Kenobi!\n\n"
        );
        assert_eq!(to_srt(&[]), "");
    }

    #[test]
    fn file_names_are_safe() {
        assert_eq!(sanitize_file_name("Meeting notes"), "Meeting notes");
        assert_eq!(sanitize_file_name("a/b\\c:d*e?f\"g<h>i|j"), "a_b_c_d_e_f_g_h_i_j");
        assert_eq!(sanitize_file_name("tab\there\nnewline"), "tab_here_newline");
        assert_eq!(sanitize_file_name("Trailing dots..."), "Trailing dots");
        assert_eq!(sanitize_file_name("Ellipsis…"), "Ellipsis");
        assert_eq!(sanitize_file_name("   "), "talkr-export");
        assert_eq!(sanitize_file_name("..."), "talkr-export");
        assert_eq!(sanitize_file_name("CON"), "talkr-export");
        assert_eq!(sanitize_file_name("nul.txt"), "talkr-export");
        assert_eq!(sanitize_file_name("Console"), "Console");
        assert_eq!(sanitize_file_name("Café ☕ notes"), "Café ☕ notes");
        let long = "x".repeat(200);
        assert_eq!(sanitize_file_name(&long).chars().count(), 80);
        // Cutting at 80 characters must not leave a trailing dot or space.
        let dotted = format!("{}. more", "y".repeat(79));
        assert_eq!(sanitize_file_name(&dotted), "y".repeat(79));
    }

    #[test]
    fn export_txt() {
        let (_dir, paths) = temp_paths();
        let e = prepare_export(&paths, item("My: title"), "txt").unwrap();
        assert_eq!(e.payload, Payload::Text("Hello world.".into()));
        assert_eq!(e.file_name, "My_ title.txt");
        assert_eq!(e.filter_name, "Text");
    }

    #[test]
    fn export_srt() {
        let (_dir, paths) = temp_paths();
        let mut it = item("t");
        it.segments_json = Some(r#"[{"startMs":0,"endMs":1000,"text":"One"},{"startMs":1000,"endMs":2500,"text":"Two"}]"#.into());
        let e = prepare_export(&paths, it, "srt").unwrap();
        assert_eq!(e.file_name, "t.srt");
        let Payload::Text(srt) = e.payload else { panic!("text expected") };
        assert!(srt.starts_with("1\n00:00:00,000 --> 00:00:01,000\nOne\n\n2\n"), "{srt}");

        // No segments, empty segments, or broken JSON cannot be exported as SRT.
        assert!(matches!(prepare_export(&paths, item("t"), "srt"), Err(AppError::Validation(_))));
        let mut empty = item("t");
        empty.segments_json = Some("[]".into());
        assert!(matches!(prepare_export(&paths, empty, "srt"), Err(AppError::Validation(_))));
        let mut broken = item("t");
        broken.segments_json = Some("{".into());
        assert!(matches!(prepare_export(&paths, broken, "srt"), Err(AppError::Json(_))));
    }

    #[test]
    fn export_wav_needs_an_existing_file() {
        let (_dir, paths) = temp_paths();
        let mut it = item("rec");
        it.audio_path = Some("audio/2026/01/a.wav".into());
        assert!(matches!(prepare_export(&paths, it.clone(), "wav"), Err(AppError::NotFound(_))));

        let full = paths.home.join("audio/2026/01/a.wav");
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(&full, b"RIFF").unwrap();
        let e = prepare_export(&paths, it, "wav").unwrap();
        assert_eq!(e.payload, Payload::Copy(full));
        assert_eq!(e.file_name, "rec.wav");

        assert!(matches!(prepare_export(&paths, item("x"), "wav"), Err(AppError::NotFound(_))));
    }

    #[test]
    fn export_rejects_unknown_formats() {
        let (_dir, paths) = temp_paths();
        for f in ["mp3", "", "TXT", "../txt"] {
            assert!(matches!(prepare_export(&paths, item("t"), f), Err(AppError::Validation(_))), "{f}");
        }
    }

    #[test]
    fn write_export_text_and_copy() {
        let dir = tempfile::tempdir().unwrap();
        let txt = dir.path().join("out.txt");
        write_export(Payload::Text("hi".into()), &txt).unwrap();
        assert_eq!(std::fs::read_to_string(&txt).unwrap(), "hi");

        let src = dir.path().join("src.wav");
        std::fs::write(&src, b"audio-bytes").unwrap();
        let dest = dir.path().join("copy.wav");
        write_export(Payload::Copy(src.clone()), &dest).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"audio-bytes");

        // Saving over the source leaves it intact.
        write_export(Payload::Copy(src.clone()), &src).unwrap();
        assert_eq!(std::fs::read(&src).unwrap(), b"audio-bytes");
    }
}
