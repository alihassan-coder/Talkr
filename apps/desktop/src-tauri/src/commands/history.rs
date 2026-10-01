//! History commands. SQLite calls are synchronous (and may wait up to the 5 s busy timeout on a
//! locked database), so every command runs its database and file work on Tokio's blocking pool
//! via `spawn_blocking`, never on an async worker thread.

use std::path::{Path, PathBuf};
use tauri::{command, AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use crate::commands::{remove_owned_audio, resolve_path};
use crate::db::{clear_history, delete_history, get_history, list_history, toggle_favorite, HistoryItem, HistoryKind, HistoryListResult};
use crate::paths::AppPaths;
use talkr_protocol::TranscriptSegment;
use crate::error::{AppError, Result};
use crate::AppState;

const PAGE_SIZE: usize = 50;

/// Largest MP3 the frontend may hand to `save_export_bytes` (about 9 hours of speech at 128 kbps).
const MAX_EXPORT_BYTES: usize = 512 * 1024 * 1024;

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

/// What an export writes: text, a copy of an audio file, or that audio re-encoded as FLAC.
#[derive(Debug, PartialEq)]
enum Payload {
    Text(String),
    Copy(PathBuf),
    Flac(PathBuf),
}

/// An export ready to save: the content, the save-dialog filter name and the suggested file name.
#[derive(Debug)]
struct Export {
    payload: Payload,
    filter_name: &'static str,
    file_name: String,
}

fn segments_of(item: &HistoryItem) -> Result<Vec<TranscriptSegment>> {
    Ok(match item.segments_json.as_deref() {
        Some(json) => serde_json::from_str(json)?,
        None => Vec::new(),
    })
}

/// Subtitle and spreadsheet formats need timed segments.
fn timed_segments(item: &HistoryItem, format: &str) -> Result<Vec<TranscriptSegment>> {
    let segments = segments_of(item)?;
    if segments.is_empty() {
        return Err(AppError::Validation(format!(
            "This item has no timed segments to export as {}",
            format.to_ascii_uppercase()
        )));
    }
    Ok(segments)
}

fn audio_file(paths: &AppPaths, item: &HistoryItem) -> Result<PathBuf> {
    item.audio_path
        .as_deref()
        .map(|p| resolve_path(paths, p))
        .filter(|p| p.is_file())
        .ok_or_else(|| AppError::NotFound("Audio file not found".into()))
}

/// Build the export of `item`: text as "txt", "md", "json", "srt", "vtt" or "csv", audio as
/// "wav" (a copy) or "flac" (lossless, about half the size).
fn prepare_export(paths: &AppPaths, item: HistoryItem, format: &str) -> Result<Export> {
    let base_name = sanitize_file_name(&item.title);
    let (payload, filter_name) = match format {
        "txt" => (Payload::Text(item.text.clone()), "Text"),
        "md" => (Payload::Text(to_markdown(&item, &segments_of(&item)?)), "Markdown"),
        "json" => (Payload::Text(to_json(&item, &segments_of(&item)?)?), "JSON"),
        "srt" => (Payload::Text(to_srt(&timed_segments(&item, format)?)), "SubRip subtitles"),
        "vtt" => (Payload::Text(to_vtt(&timed_segments(&item, format)?)), "WebVTT subtitles"),
        "csv" => (Payload::Text(to_csv(&timed_segments(&item, format)?)), "CSV spreadsheet"),
        "wav" => (Payload::Copy(audio_file(paths, &item)?), "WAV audio"),
        "flac" => {
            let src = audio_file(paths, &item)?;
            if !src.extension().is_some_and(|e| e.eq_ignore_ascii_case("wav")) {
                return Err(AppError::Validation("Only WAV audio can be converted to FLAC".into()));
            }
            (Payload::Flac(src), "FLAC audio")
        }
        _ => return Err(AppError::Validation(format!("Unsupported format: {}", format))),
    };
    Ok(Export { payload, filter_name, file_name: format!("{}.{}", base_name, format) })
}

fn same_file(a: &Path, b: &Path) -> bool {
    b.exists() && a.canonicalize().ok() == b.canonicalize().ok()
}

fn write_export(payload: Payload, dest: &Path) -> Result<()> {
    match payload {
        Payload::Text(content) => std::fs::write(dest, content)?,
        Payload::Copy(src) => {
            // Exporting onto the source itself would truncate it.
            if same_file(&src, dest) {
                return Ok(());
            }
            std::fs::copy(&src, dest)?;
        }
        Payload::Flac(src) => {
            if same_file(&src, dest) {
                return Err(AppError::Validation("Choose a different file than the original audio".into()));
            }
            // Encode next to the destination and move it into place, so a failed export never
            // leaves a half-written file under the chosen name.
            let partial = dest.with_extension("flac.part");
            crate::audio::flac::wav_file_to_flac_file(&src, &partial)?;
            std::fs::rename(&partial, dest).inspect_err(|_| {
                let _ = std::fs::remove_file(&partial);
            })?;
        }
    }
    Ok(())
}

/// Ask where to save with a native dialog. `None` when the user cancels.
async fn pick_save_path(app: &AppHandle, filter_name: String, ext: String, file_name: String) -> Result<Option<PathBuf>> {
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
    picked.into_path().map(Some).map_err(|e| AppError::Path(e.to_string()))
}

/// Export an item via a native save dialog (formats: see [`prepare_export`]).
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
    let Some(saved_path) = pick_save_path(&app, export.filter_name.to_string(), ext, export.file_name).await? else {
        return Ok(None);
    };
    let payload = export.payload;
    let dest = saved_path.clone();
    blocking(move || write_export(payload, &dest)).await?;
    Ok(Some(saved_path.to_string_lossy().to_string()))
}

/// Check a `save_export_bytes` request: the format must be "mp3" and the body a non-empty MP3
/// below [`MAX_EXPORT_BYTES`]. Returns the history id.
fn check_bytes_export(format: Option<&str>, id: Option<&str>, bytes: &[u8]) -> Result<String> {
    if format != Some("mp3") {
        return Err(AppError::Validation(format!("Unsupported format: {}", format.unwrap_or_default())));
    }
    let id = id.filter(|id| !id.is_empty() && id.len() <= 64).ok_or_else(|| AppError::Validation("Missing history item id".into()))?;
    if bytes.is_empty() || bytes.len() > MAX_EXPORT_BYTES {
        return Err(AppError::Validation("The encoded audio is empty or too large".into()));
    }
    // An MP3 starts with an ID3 tag or an MPEG frame sync; anything else is not what we encoded.
    let is_mp3 = bytes.starts_with(b"ID3") || (bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] & 0xE0 == 0xE0);
    if !is_mp3 {
        return Err(AppError::Validation("The encoded audio is not an MP3 file".into()));
    }
    Ok(id.to_string())
}

fn header<'a>(request: &'a tauri::ipc::Request<'_>, name: &str) -> Option<&'a str> {
    request.headers().get(name).and_then(|v| v.to_str().ok())
}

/// Save an MP3 the frontend encoded from a history item's audio, via a native save dialog.
///
/// The bytes arrive as the raw IPC body (no JSON round trip). Headers: `x-format: mp3` and
/// `x-history-id`, whose title becomes the suggested file name. Nothing is written anywhere the
/// user did not pick. Returns the saved path, or `null` on cancel.
#[command]
pub async fn save_export_bytes(
    state: State<'_, AppState>,
    app: AppHandle,
    request: tauri::ipc::Request<'_>,
) -> Result<Option<String>> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err(AppError::Validation("Expected the encoded audio as raw bytes".into()));
    };
    let id = check_bytes_export(header(&request, "x-format"), header(&request, "x-history-id"), bytes)?;
    let bytes = bytes.clone();

    let paths = state.paths.clone();
    let item = blocking(move || get_history(&paths, &id)?.ok_or_else(|| AppError::NotFound("History item not found".into())))
        .await?;
    let file_name = format!("{}.mp3", sanitize_file_name(&item.title));
    let Some(saved_path) = pick_save_path(&app, "MP3 audio".into(), "mp3".into(), file_name).await? else {
        return Ok(None);
    };
    let dest = saved_path.clone();
    blocking(move || Ok(std::fs::write(&dest, bytes)?)).await?;
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

/// One cue's text on as few lines as it needs: a blank line would end a subtitle cue early.
fn cue_text(text: &str) -> String {
    text.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n")
}

fn to_srt(segments: &[TranscriptSegment]) -> String {
    let mut srt = String::new();
    for (i, seg) in segments.iter().enumerate() {
        srt.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            i + 1,
            format_timestamp(seg.start_ms, ','),
            format_timestamp(seg.end_ms, ','),
            cue_text(&seg.text)
        ));
    }
    srt
}

fn to_vtt(segments: &[TranscriptSegment]) -> String {
    let mut vtt = String::from("WEBVTT\n\n");
    for seg in segments {
        vtt.push_str(&format!(
            "{} --> {}\n{}\n\n",
            format_timestamp(seg.start_ms, '.'),
            format_timestamp(seg.end_ms, '.'),
            // "-->" inside cue text would be read as a timing line.
            cue_text(&seg.text).replace("-->", "->")
        ));
    }
    vtt
}

/// Segments as a spreadsheet, with a BOM so Excel reads the UTF-8 text correctly.
fn to_csv(segments: &[TranscriptSegment]) -> String {
    let mut csv = String::from("\u{FEFF}start,end,text\r\n");
    for seg in segments {
        csv.push_str(&format!(
            "{},{},{}\r\n",
            format_timestamp(seg.start_ms, '.'),
            format_timestamp(seg.end_ms, '.'),
            csv_field(seg.text.trim())
        ));
    }
    csv
}

/// Quote a CSV field, and stop spreadsheets from running text that starts like a formula.
fn csv_field(text: &str) -> String {
    let text = if text.starts_with(['=', '+', '-', '@', '\t', '\r']) { format!("'{}", text) } else { text.to_string() };
    format!("\"{}\"", text.replace('"', "\"\""))
}

fn local_time(ms: i64) -> String {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_millis_opt(ms)
        .single()
        .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default()
}

fn to_markdown(item: &HistoryItem, segments: &[TranscriptSegment]) -> String {
    let kind = match item.kind {
        HistoryKind::Tts => "Speech",
        HistoryKind::Stt => "Transcript",
    };
    let mut meta = vec![kind.to_string(), local_time(item.created_at), item.model_id.clone()];
    meta.extend(item.voice_id.clone());
    meta.extend(item.language.clone().filter(|l| l != "auto"));
    meta.extend(item.duration_ms.map(format_clock));
    let title = item.title.lines().next().unwrap_or_default().trim();
    let title = if title.is_empty() { "Talkr export" } else { title };
    let mut md = format!("# {}\n\n_{}_\n\n", title, meta.join(" · "));
    if segments.is_empty() {
        md.push_str(item.text.trim());
        md.push('\n');
    } else {
        for seg in segments {
            md.push_str(&format!("**[{}]** {}\n\n", format_clock(seg.start_ms), seg.text.trim()));
        }
    }
    md
}

fn to_json(item: &HistoryItem, segments: &[TranscriptSegment]) -> Result<String> {
    let value = serde_json::json!({
        "title": item.title,
        "kind": match item.kind { HistoryKind::Tts => "tts", HistoryKind::Stt => "stt" },
        "createdAt": chrono::DateTime::from_timestamp_millis(item.created_at).map(|t| t.to_rfc3339()),
        "model": item.model_id,
        "voice": item.voice_id,
        "language": item.language,
        "durationMs": item.duration_ms,
        "text": item.text,
        "segments": segments,
        "generator": format!("Talkr {}", env!("CARGO_PKG_VERSION")),
    });
    Ok(serde_json::to_string_pretty(&value)?)
}

/// `hh:mm:ss<sep>mmm`: SRT puts a comma before the milliseconds, WebVTT and CSV a dot.
fn format_timestamp(ms: i64, sep: char) -> String {
    let ms = ms.max(0);
    let hours = ms / 3_600_000;
    let minutes = (ms % 3_600_000) / 60_000;
    let seconds = (ms % 60_000) / 1000;
    let millis = ms % 1000;
    format!("{:02}:{:02}:{:02}{}{:03}", hours, minutes, seconds, sep, millis)
}

/// `m:ss`, or `h:mm:ss` from an hour on.
fn format_clock(ms: i64) -> String {
    let s = ms.max(0) / 1000;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
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

    fn timed(title: &str) -> HistoryItem {
        let mut it = item(title);
        it.segments_json = Some(r#"[{"startMs":0,"endMs":1000,"text":"One"},{"startMs":1000,"endMs":2500,"text":"Two"}]"#.into());
        it
    }

    fn temp_paths() -> (tempfile::TempDir, AppPaths) {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::init_at(dir.path()).unwrap();
        (dir, paths)
    }

    fn text_of(e: Export) -> String {
        let Payload::Text(text) = e.payload else { panic!("text expected") };
        text
    }

    #[test]
    fn timestamps() {
        assert_eq!(format_timestamp(0, ','), "00:00:00,000");
        assert_eq!(format_timestamp(1_234, ','), "00:00:01,234");
        assert_eq!(format_timestamp(61_005, '.'), "00:01:01.005");
        assert_eq!(format_timestamp(3_600_000 + 59 * 60_000 + 59_999, ','), "01:59:59,999");
        assert_eq!(format_timestamp(100 * 3_600_000, ','), "100:00:00,000");
        assert_eq!(format_timestamp(-5, ','), "00:00:00,000");
        assert_eq!(format_clock(0), "0:00");
        assert_eq!(format_clock(83_000), "1:23");
        assert_eq!(format_clock(3_725_000), "1:02:05");
    }

    #[test]
    fn srt_numbering_and_layout() {
        let srt = to_srt(&[seg(0, 1500, "  Hello there. "), seg(1500, 3_723_004, "General Kenobi!")]);
        assert_eq!(
            srt,
            "1\n00:00:00,000 --> 00:00:01,500\nHello there.\n\n2\n00:00:01,500 --> 01:02:03,004\nGeneral Kenobi!\n\n"
        );
        assert_eq!(to_srt(&[]), "");
        // A blank line inside a segment would end the cue early.
        assert_eq!(to_srt(&[seg(0, 1, "a\n\n b ")]), "1\n00:00:00,000 --> 00:00:00,001\na\nb\n\n");
    }

    #[test]
    fn vtt_layout() {
        let vtt = to_vtt(&[seg(0, 1500, "Hi --> there"), seg(1500, 61_000, "Bye")]);
        assert_eq!(vtt, "WEBVTT\n\n00:00:00.000 --> 00:00:01.500\nHi -> there\n\n00:00:01.500 --> 00:01:01.000\nBye\n\n");
    }

    #[test]
    fn csv_quotes_and_defuses_formulas() {
        let csv = to_csv(&[seg(0, 1000, "He said \"hi\", twice"), seg(1000, 2000, "=HYPERLINK(\"x\")"), seg(2000, 3000, "-5 degrees")]);
        assert_eq!(
            csv,
            "\u{FEFF}start,end,text\r\n\
             00:00:00.000,00:00:01.000,\"He said \"\"hi\"\", twice\"\r\n\
             00:00:01.000,00:00:02.000,\"'=HYPERLINK(\"\"x\"\")\"\r\n\
             00:00:02.000,00:00:03.000,\"'-5 degrees\"\r\n"
        );
    }

    #[test]
    fn markdown_and_json() {
        let (_dir, paths) = temp_paths();
        let mut it = timed("Standup notes");
        it.language = Some("en".into());
        it.duration_ms = Some(2500);
        let md = text_of(prepare_export(&paths, it.clone(), "md").unwrap());
        assert!(md.starts_with("# Standup notes\n\n_Transcript · "), "{md}");
        assert!(md.contains(" · m · en · 0:02_"), "{md}");
        assert!(md.contains("**[0:00]** One\n\n**[0:01]** Two\n"), "{md}");

        let plain = text_of(prepare_export(&paths, item(""), "md").unwrap());
        assert!(plain.starts_with("# Talkr export\n"), "{plain}");
        assert!(plain.ends_with("Hello world.\n"), "{plain}");

        let json: serde_json::Value = serde_json::from_str(&text_of(prepare_export(&paths, it, "json").unwrap())).unwrap();
        assert_eq!(json["title"], "Standup notes");
        assert_eq!(json["kind"], "stt");
        assert_eq!(json["createdAt"], "1970-01-01T00:00:00+00:00");
        assert_eq!(json["segments"][1]["text"], "Two");
        assert_eq!(json["segments"][1]["endMs"], 2500);
        assert_eq!(json["durationMs"], 2500);
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
    fn timed_formats_need_segments() {
        let (_dir, paths) = temp_paths();
        for format in ["srt", "vtt", "csv"] {
            let e = prepare_export(&paths, timed("t"), format).unwrap();
            assert_eq!(e.file_name, format!("t.{format}"));
            assert!(text_of(e).contains("00:00:01"), "{format}");

            // No segments, empty segments, or broken JSON cannot be exported with timings.
            assert!(matches!(prepare_export(&paths, item("t"), format), Err(AppError::Validation(_))), "{format}");
            let mut empty = item("t");
            empty.segments_json = Some("[]".into());
            assert!(matches!(prepare_export(&paths, empty, format), Err(AppError::Validation(_))), "{format}");
            let mut broken = item("t");
            broken.segments_json = Some("{".into());
            assert!(matches!(prepare_export(&paths, broken, format), Err(AppError::Json(_))), "{format}");
        }
        let srt = text_of(prepare_export(&paths, timed("t"), "srt").unwrap());
        assert!(srt.starts_with("1\n00:00:00,000 --> 00:00:01,000\nOne\n\n2\n"), "{srt}");
    }

    #[test]
    fn audio_exports_need_an_existing_file() {
        let (_dir, paths) = temp_paths();
        let mut it = item("rec");
        it.audio_path = Some("audio/2026/01/a.wav".into());
        for format in ["wav", "flac"] {
            assert!(matches!(prepare_export(&paths, it.clone(), format), Err(AppError::NotFound(_))), "{format}");
            assert!(matches!(prepare_export(&paths, item("x"), format), Err(AppError::NotFound(_))), "{format}");
        }

        let full = paths.home.join("audio/2026/01/a.wav");
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(&full, b"RIFF").unwrap();
        let e = prepare_export(&paths, it.clone(), "wav").unwrap();
        assert_eq!(e.payload, Payload::Copy(full.clone()));
        assert_eq!(e.file_name, "rec.wav");
        let e = prepare_export(&paths, it, "flac").unwrap();
        assert_eq!(e.payload, Payload::Flac(full));
        assert_eq!(e.file_name, "rec.flac");

        // FLAC is made from WAV only (a transcribed MP3 stays an MP3).
        let mp3 = paths.home.join("audio/2026/01/b.mp3");
        std::fs::write(&mp3, b"ID3").unwrap();
        let mut other = item("song");
        other.audio_path = Some("audio/2026/01/b.mp3".into());
        assert!(matches!(prepare_export(&paths, other, "flac"), Err(AppError::Validation(_))));
    }

    #[test]
    fn export_rejects_unknown_formats() {
        let (_dir, paths) = temp_paths();
        for f in ["mp3", "", "TXT", "../txt", "docx"] {
            assert!(matches!(prepare_export(&paths, item("t"), f), Err(AppError::Validation(_))), "{f}");
        }
    }

    #[test]
    fn write_export_text_copy_and_flac() {
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

        // FLAC: a real WAV converts; a broken one fails without leaving files behind.
        let wav = dir.path().join("speech.wav");
        let spec = hound::WavSpec { channels: 1, sample_rate: 24_000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(&wav, spec).unwrap();
        for i in 0..10_000 {
            w.write_sample(((i as f32 * 0.05).sin() * 10_000.0) as i16).unwrap();
        }
        w.finalize().unwrap();
        let flac = dir.path().join("speech.flac");
        write_export(Payload::Flac(wav.clone()), &flac).unwrap();
        assert!(std::fs::read(&flac).unwrap().starts_with(b"fLaC"));
        assert!(!dir.path().join("speech.flac.part").exists());
        assert!(matches!(write_export(Payload::Flac(wav.clone()), &wav), Err(AppError::Validation(_))));

        let bad = dir.path().join("bad.flac");
        assert!(write_export(Payload::Flac(src), &bad).is_err());
        assert!(!bad.exists());
        assert!(!dir.path().join("bad.flac.part").exists());
    }

    #[test]
    fn byte_exports_are_checked() {
        let mp3 = [0xFF, 0xFB, 0x90, 0x00];
        assert_eq!(check_bytes_export(Some("mp3"), Some("abc"), &mp3).unwrap(), "abc");
        assert_eq!(check_bytes_export(Some("mp3"), Some("abc"), b"ID3\x04").unwrap(), "abc");
        assert!(check_bytes_export(Some("wav"), Some("abc"), &mp3).is_err());
        assert!(check_bytes_export(None, Some("abc"), &mp3).is_err());
        assert!(check_bytes_export(Some("mp3"), None, &mp3).is_err());
        assert!(check_bytes_export(Some("mp3"), Some(""), &mp3).is_err());
        assert!(check_bytes_export(Some("mp3"), Some(&"x".repeat(65)), &mp3).is_err());
        assert!(check_bytes_export(Some("mp3"), Some("abc"), b"").is_err());
        assert!(check_bytes_export(Some("mp3"), Some("abc"), b"<html>").is_err());
    }
}
