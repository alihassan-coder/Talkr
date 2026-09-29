use rusqlite::{Connection, params, OptionalExtension};
use serde::{Deserialize, Serialize};
use crate::error::Result;

const SCHEMA_VERSION: i32 = 1;

/// Create the database (if needed) and run migrations.
pub fn init(paths: &crate::paths::AppPaths) -> Result<()> {
    let conn = get_connection(paths)?;

    let user_version: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if user_version < SCHEMA_VERSION {
        run_migrations(&conn, user_version)?;
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    }

    Ok(())
}

fn run_migrations(conn: &Connection, from_version: i32) -> Result<()> {
    if from_version < 1 {
        conn.execute_batch(include_str!("schema.sql"))?;
    }
    Ok(())
}

pub fn get_connection(paths: &crate::paths::AppPaths) -> Result<Connection> {
    let conn = Connection::open(&paths.db_file)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(conn)
}

/// Turn free-form user input into a safe FTS5 query: each word becomes a quoted prefix term.
fn fts_query(q: &str) -> Option<String> {
    let terms: Vec<String> = q
        .split_whitespace()
        .map(|t| t.replace('"', ""))
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{}\"*", t))
        .collect();
    (!terms.is_empty()).then(|| terms.join(" "))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryItem {
    pub id: String,
    pub kind: HistoryKind,
    pub created_at: i64,
    pub title: String,
    pub text: String,
    pub audio_path: Option<String>,
    pub duration_ms: Option<i64>,
    pub model_id: String,
    pub voice_id: Option<String>,
    pub language: Option<String>,
    pub device: String,
    pub processing_ms: i64,
    pub favorite: bool,
    pub segments_json: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HistoryKind {
    Tts,
    Stt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryListResult {
    pub items: Vec<HistoryItem>,
    pub next_cursor: Option<i64>,
}

pub fn insert_history(paths: &crate::paths::AppPaths, item: &HistoryItem) -> Result<()> {
    let conn = get_connection(paths)?;
    conn.execute(
        "INSERT INTO history (id, kind, created_at, title, text, audio_path, duration_ms, model_id, voice_id, language, device, processing_ms, favorite, segments_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        params![
            item.id,
            format!("{:?}", item.kind).to_lowercase(),
            item.created_at,
            item.title,
            item.text,
            item.audio_path,
            item.duration_ms,
            item.model_id,
            item.voice_id,
            item.language,
            item.device,
            item.processing_ms,
            item.favorite as i32,
            item.segments_json,
        ],
    )?;
    Ok(())
}

pub fn list_history(
    paths: &crate::paths::AppPaths,
    cursor: Option<i64>,
    query: Option<String>,
    kind: Option<HistoryKind>,
    favorites_only: bool,
    limit: usize,
) -> Result<HistoryListResult> {
    let conn = get_connection(paths)?;

    let mut where_clauses = Vec::new();
    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(cursor_ts) = cursor {
        where_clauses.push("created_at < ?");
        params_vec.push(Box::new(cursor_ts));
    }

    if let Some(q) = query.as_deref().and_then(fts_query) {
        where_clauses.push("rowid IN (SELECT rowid FROM history_fts WHERE history_fts MATCH ?)");
        params_vec.push(Box::new(q));
    }

    if let Some(k) = kind {
        where_clauses.push("kind = ?");
        params_vec.push(Box::new(format!("{:?}", k).to_lowercase()));
    }

    if favorites_only {
        where_clauses.push("favorite = 1");
    }

    let where_sql = if where_clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", where_clauses.join(" AND "))
    };

    let sql = format!(
        "SELECT id, kind, created_at, title, text, audio_path, duration_ms, model_id, voice_id, language, device, processing_ms, favorite, segments_json
         FROM history
         {} ORDER BY created_at DESC LIMIT ?",
        where_sql
    );

    let mut stmt = conn.prepare(&sql)?;
    let mut params_with_limit = params_vec;
    params_with_limit.push(Box::new(limit as i64 + 1));

    let items = stmt.query_map(rusqlite::params_from_iter(params_with_limit), |row| {
        Ok(HistoryItem {
            id: row.get(0)?,
            kind: match row.get::<_, String>(1)?.as_str() {
                "tts" => HistoryKind::Tts,
                "stt" => HistoryKind::Stt,
                _ => HistoryKind::Tts,
            },
            created_at: row.get(2)?,
            title: row.get(3)?,
            text: row.get(4)?,
            audio_path: row.get(5)?,
            duration_ms: row.get(6)?,
            model_id: row.get(7)?,
            voice_id: row.get(8)?,
            language: row.get(9)?,
            device: row.get(10)?,
            processing_ms: row.get(11)?,
            favorite: row.get::<_, i32>(12)? != 0,
            segments_json: row.get(13)?,
        })
    })?.collect::<std::result::Result<Vec<_>, _>>()?;

    let mut items = items;
    let has_more = items.len() > limit;
    items.truncate(limit);
    let next_cursor = if has_more { items.last().map(|i| i.created_at) } else { None };

    Ok(HistoryListResult { items, next_cursor })
}

pub fn get_history(paths: &crate::paths::AppPaths, id: &str) -> Result<Option<HistoryItem>> {
    let conn = get_connection(paths)?;
    let item = conn.query_row(
        "SELECT id, kind, created_at, title, text, audio_path, duration_ms, model_id, voice_id, language, device, processing_ms, favorite, segments_json
         FROM history WHERE id = ?1",
        params![id],
        |row| {
            Ok(HistoryItem {
                id: row.get(0)?,
                kind: match row.get::<_, String>(1)?.as_str() {
                    "tts" => HistoryKind::Tts,
                    "stt" => HistoryKind::Stt,
                    _ => HistoryKind::Tts,
                },
                created_at: row.get(2)?,
                title: row.get(3)?,
                text: row.get(4)?,
                audio_path: row.get(5)?,
                duration_ms: row.get(6)?,
                model_id: row.get(7)?,
                voice_id: row.get(8)?,
                language: row.get(9)?,
                device: row.get(10)?,
                processing_ms: row.get(11)?,
                favorite: row.get::<_, i32>(12)? != 0,
                segments_json: row.get(13)?,
            })
        },
    ).optional()?;
    Ok(item)
}

pub fn delete_history(paths: &crate::paths::AppPaths, id: &str) -> Result<()> {
    let conn = get_connection(paths)?;
    conn.execute("DELETE FROM history WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn toggle_favorite(paths: &crate::paths::AppPaths, id: &str) -> Result<bool> {
    let conn = get_connection(paths)?;
    let current: i32 = conn.query_row("SELECT favorite FROM history WHERE id = ?1", params![id], |r| r.get(0))?;
    let new = 1 - current;
    conn.execute("UPDATE history SET favorite = ?1 WHERE id = ?2", params![new, id])?;
    Ok(new != 0)
}

/// Result of a bulk delete: number of rows removed and the audio paths they referenced
/// (so callers can remove the files).
pub struct DeletedRows {
    pub count: usize,
    pub audio_paths: Vec<String>,
}

/// Delete all non-favorite items.
pub fn clear_history(paths: &crate::paths::AppPaths) -> Result<DeletedRows> {
    delete_where(paths, "favorite = 0", params![])
}

/// Delete non-favorite items older than `retention_days` (0 = keep forever).
pub fn prune_old_history(paths: &crate::paths::AppPaths, retention_days: u32) -> Result<DeletedRows> {
    if retention_days == 0 {
        return Ok(DeletedRows { count: 0, audio_paths: Vec::new() });
    }
    let cutoff = chrono::Utc::now() - chrono::Duration::days(retention_days as i64);
    delete_where(paths, "created_at < ?1 AND favorite = 0", params![cutoff.timestamp_millis()])
}

fn delete_where(paths: &crate::paths::AppPaths, where_sql: &str, args: &[&dyn rusqlite::ToSql]) -> Result<DeletedRows> {
    let mut conn = get_connection(paths)?;
    let tx = conn.transaction()?;
    let audio_paths = {
        let mut stmt = tx.prepare(&format!("SELECT audio_path FROM history WHERE {} AND audio_path IS NOT NULL", where_sql))?;
        let rows = stmt.query_map(args, |r| r.get::<_, String>(0))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()?
    };
    let count = tx.execute(&format!("DELETE FROM history WHERE {}", where_sql), args)?;
    tx.commit()?;
    Ok(DeletedRows { count, audio_paths })
}