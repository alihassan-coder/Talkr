//! History database (SQLite with an FTS5 index over titles and text).
//!
//! Every call opens its own short-lived connection; SQLite in WAL mode handles the concurrency
//! and there is no pool to poison. The schema is created lazily: the first connection to a
//! database file runs the migrations, so a failure at startup (disk full, locked file) is retried
//! by the next history call instead of breaking history until a restart.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use crate::error::{AppError, Result};

const SCHEMA_VERSION: i32 = 1;

/// Database files whose schema this process has already brought up to date.
fn migrated() -> &'static Mutex<HashSet<PathBuf>> {
    static MIGRATED: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
    MIGRATED.get_or_init(Default::default)
}

/// Create the database (if needed) and run migrations. Safe to call more than once.
pub fn init(paths: &crate::paths::AppPaths) -> Result<()> {
    get_connection(paths).map(drop)
}

/// Open a connection, running the migrations first if this process has not yet done so for
/// this file.
pub fn get_connection(paths: &crate::paths::AppPaths) -> Result<Connection> {
    open(&paths.db_file).map_err(|e| match e {
        AppError::Database(e) => AppError::Other(format!(
            "The history database ({}) could not be opened: {}",
            paths.db_file.display(),
            e
        )),
        other => other,
    })
}

fn open(db_file: &Path) -> Result<Connection> {
    let mut conn = Connection::open(db_file)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;

    let mut done = migrated().lock().unwrap_or_else(|e| e.into_inner());
    if !done.contains(db_file) {
        migrate(&mut conn)?;
        done.insert(db_file.to_path_buf());
    }
    Ok(conn)
}

/// Bring the schema to `SCHEMA_VERSION` in one IMMEDIATE transaction: a crash or error midway
/// leaves the previous version intact, and two processes cannot migrate at the same time.
/// `PRAGMA user_version` is part of the transaction, so it moves only if everything else did.
fn migrate(conn: &mut Connection) -> Result<()> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let from_version: i32 = tx.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if from_version > SCHEMA_VERSION {
        log::warn!(
            "History database schema v{} is newer than this version of Talkr (v{}); using it as is",
            from_version,
            SCHEMA_VERSION
        );
        return Ok(());
    }
    if from_version < 1 {
        tx.execute_batch(include_str!("schema.sql"))?;
    }
    // Future migrations: `if from_version < 2 { ... }`
    if from_version != SCHEMA_VERSION {
        tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    }
    tx.commit()?;
    Ok(())
}

/// What a search box query turns into.
#[derive(Debug, PartialEq, Eq)]
enum Search {
    /// No query: everything matches.
    All,
    /// A query with nothing searchable in it (only punctuation): nothing matches.
    Nothing,
    /// An FTS5 MATCH expression.
    Match(String),
}

/// Longest search query used, in characters; the rest is ignored.
const MAX_SEARCH_CHARS: usize = 1_000;

/// Turn free-form user input into a safe FTS5 query: each word becomes a quoted prefix term, so
/// FTS5 syntax in the input (`AND`, `NEAR(`, `col:`, `-`, `*`, quotes) is searched for as text
/// instead of being interpreted. Words without a letter or digit are dropped, since the
/// tokenizer would reduce them to an empty phrase.
fn fts_query(q: &str) -> Search {
    // Only the start of an over-long query is used: every word becomes an FTS5 term that must
    // match, so a pasted page of text would cost a lot and could never match anyway.
    let q = match q.char_indices().nth(MAX_SEARCH_CHARS) {
        Some((end, _)) => &q[..end],
        None => q,
    };
    if q.trim().is_empty() {
        return Search::All;
    }
    let terms: Vec<String> = q
        .split_whitespace()
        .map(|t| t.replace('"', ""))
        .filter(|t| t.chars().any(char::is_alphanumeric))
        .map(|t| format!("\"{}\"*", t))
        .collect();
    if terms.is_empty() {
        Search::Nothing
    } else {
        Search::Match(terms.join(" "))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

impl HistoryKind {
    fn as_str(self) -> &'static str {
        match self {
            HistoryKind::Tts => "tts",
            HistoryKind::Stt => "stt",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryListResult {
    pub items: Vec<HistoryItem>,
    pub next_cursor: Option<i64>,
}

const COLUMNS: &str = "id, kind, created_at, title, text, audio_path, duration_ms, model_id, voice_id, language, device, processing_ms, favorite, segments_json";

fn row_to_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<HistoryItem> {
    Ok(HistoryItem {
        id: row.get(0)?,
        kind: match row.get::<_, String>(1)?.as_str() {
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
}

pub fn insert_history(paths: &crate::paths::AppPaths, item: &HistoryItem) -> Result<()> {
    let conn = get_connection(paths)?;
    conn.execute(
        &format!("INSERT INTO history ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)"),
        params![
            item.id,
            item.kind.as_str(),
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

/// List history newest first, `limit` per page. `cursor` is the `next_cursor` of the previous
/// page (a `created_at` value); items strictly older than it are returned.
pub fn list_history(
    paths: &crate::paths::AppPaths,
    cursor: Option<i64>,
    query: Option<String>,
    kind: Option<HistoryKind>,
    favorites_only: bool,
    limit: usize,
) -> Result<HistoryListResult> {
    let empty = HistoryListResult { items: Vec::new(), next_cursor: None };
    let search = query.as_deref().map(fts_query).unwrap_or(Search::All);
    if search == Search::Nothing || limit == 0 {
        return Ok(empty);
    }

    let conn = get_connection(paths)?;

    let mut where_clauses = Vec::new();
    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(cursor_ts) = cursor {
        where_clauses.push("created_at < ?");
        params_vec.push(Box::new(cursor_ts));
    }

    if let Search::Match(q) = search {
        where_clauses.push("rowid IN (SELECT rowid FROM history_fts WHERE history_fts MATCH ?)");
        params_vec.push(Box::new(q));
    }

    if let Some(k) = kind {
        where_clauses.push("kind = ?");
        params_vec.push(Box::new(k.as_str()));
    }

    if favorites_only {
        where_clauses.push("favorite = 1");
    }

    let where_sql = if where_clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", where_clauses.join(" AND "))
    };

    let sql = format!("SELECT {COLUMNS} FROM history {where_sql} ORDER BY created_at DESC, rowid DESC LIMIT ?");

    let mut stmt = conn.prepare(&sql)?;
    params_vec.push(Box::new(i64::try_from(limit).unwrap_or(i64::MAX - 1) + 1));

    let mut items = stmt
        .query_map(rusqlite::params_from_iter(params_vec), row_to_item)?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let has_more = items.len() > limit;
    items.truncate(limit);
    let next_cursor = if has_more { items.last().map(|i| i.created_at) } else { None };

    Ok(HistoryListResult { items, next_cursor })
}

pub fn get_history(paths: &crate::paths::AppPaths, id: &str) -> Result<Option<HistoryItem>> {
    let conn = get_connection(paths)?;
    let item = conn
        .query_row(&format!("SELECT {COLUMNS} FROM history WHERE id = ?1"), params![id], row_to_item)
        .optional()?;
    Ok(item)
}

pub fn delete_history(paths: &crate::paths::AppPaths, id: &str) -> Result<()> {
    let conn = get_connection(paths)?;
    conn.execute("DELETE FROM history WHERE id = ?1", params![id])?;
    Ok(())
}

/// Toggle favorite; returns the new value. A single UPDATE, so two quick clicks cannot race.
pub fn toggle_favorite(paths: &crate::paths::AppPaths, id: &str) -> Result<bool> {
    let conn = get_connection(paths)?;
    let new: Option<i32> = conn
        .query_row(
            "UPDATE history SET favorite = 1 - favorite WHERE id = ?1 RETURNING favorite",
            params![id],
            |r| r.get(0),
        )
        .optional()?;
    match new {
        Some(v) => Ok(v != 0),
        None => Err(AppError::NotFound("History item not found".into())),
    }
}

/// Result of a bulk delete: number of rows removed and the audio paths they referenced
/// (so callers can remove the files).
#[derive(Debug)]
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
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let audio_paths = {
        let mut stmt = tx.prepare(&format!("SELECT audio_path FROM history WHERE {} AND audio_path IS NOT NULL", where_sql))?;
        let rows = stmt.query_map(args, |r| r.get::<_, String>(0))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()?
    };
    let count = tx.execute(&format!("DELETE FROM history WHERE {}", where_sql), args)?;
    tx.commit()?;
    Ok(DeletedRows { count, audio_paths })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::AppPaths;

    fn temp_paths() -> (tempfile::TempDir, AppPaths) {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::init_at(dir.path()).unwrap();
        (dir, paths)
    }

    fn item(id: &str, kind: HistoryKind, created_at: i64, title: &str, text: &str) -> HistoryItem {
        HistoryItem {
            id: id.into(),
            kind,
            created_at,
            title: title.into(),
            text: text.into(),
            audio_path: Some(format!("audio/2026/01/{id}.wav")),
            duration_ms: Some(1200),
            model_id: "model".into(),
            voice_id: (kind == HistoryKind::Tts).then(|| "voice".into()),
            language: Some("en".into()),
            device: "cpu".into(),
            processing_ms: 42,
            favorite: false,
            segments_json: None,
        }
    }

    fn ids(r: &HistoryListResult) -> Vec<&str> {
        r.items.iter().map(|i| i.id.as_str()).collect()
    }

    fn search(paths: &AppPaths, q: &str) -> Vec<String> {
        list_history(paths, None, Some(q.to_string()), None, false, 100)
            .unwrap_or_else(|e| panic!("search {q:?} failed: {e}"))
            .items
            .into_iter()
            .map(|i| i.id)
            .collect()
    }

    #[test]
    fn insert_and_get_round_trip() {
        let (_dir, paths) = temp_paths();
        init(&paths).unwrap();
        let mut a = item("a", HistoryKind::Stt, 1_000, "Title", "Some text");
        a.segments_json = Some(r#"[{"startMs":0,"endMs":10,"text":"x"}]"#.into());
        a.favorite = true;
        a.voice_id = None;
        a.duration_ms = None;
        insert_history(&paths, &a).unwrap();
        assert_eq!(get_history(&paths, "a").unwrap(), Some(a));
        assert_eq!(get_history(&paths, "missing").unwrap(), None);
    }

    #[test]
    fn duplicate_id_is_rejected() {
        let (_dir, paths) = temp_paths();
        let a = item("a", HistoryKind::Tts, 1, "t", "x");
        insert_history(&paths, &a).unwrap();
        assert!(insert_history(&paths, &a).is_err());
    }

    #[test]
    fn list_is_newest_first_and_paginates_with_cursor() {
        let (_dir, paths) = temp_paths();
        for i in 0..7 {
            insert_history(&paths, &item(&format!("i{i}"), HistoryKind::Tts, 1_000 + i, "t", "x")).unwrap();
        }
        let p1 = list_history(&paths, None, None, None, false, 3).unwrap();
        assert_eq!(ids(&p1), ["i6", "i5", "i4"]);
        assert_eq!(p1.next_cursor, Some(1_004));
        let p2 = list_history(&paths, p1.next_cursor, None, None, false, 3).unwrap();
        assert_eq!(ids(&p2), ["i3", "i2", "i1"]);
        assert_eq!(p2.next_cursor, Some(1_001));
        let p3 = list_history(&paths, p2.next_cursor, None, None, false, 3).unwrap();
        assert_eq!(ids(&p3), ["i0"]);
        assert_eq!(p3.next_cursor, None);

        // An exactly full last page has no next cursor.
        let all = list_history(&paths, None, None, None, false, 7).unwrap();
        assert_eq!(all.items.len(), 7);
        assert_eq!(all.next_cursor, None);

        assert!(list_history(&paths, None, None, None, false, 0).unwrap().items.is_empty());
    }

    #[test]
    fn list_filters_by_kind_and_favorites() {
        let (_dir, paths) = temp_paths();
        insert_history(&paths, &item("t1", HistoryKind::Tts, 1, "a", "x")).unwrap();
        insert_history(&paths, &item("s1", HistoryKind::Stt, 2, "b", "x")).unwrap();
        let mut fav = item("s2", HistoryKind::Stt, 3, "c", "x");
        fav.favorite = true;
        insert_history(&paths, &fav).unwrap();

        assert_eq!(ids(&list_history(&paths, None, None, Some(HistoryKind::Tts), false, 10).unwrap()), ["t1"]);
        assert_eq!(ids(&list_history(&paths, None, None, Some(HistoryKind::Stt), false, 10).unwrap()), ["s2", "s1"]);
        assert_eq!(ids(&list_history(&paths, None, None, None, true, 10).unwrap()), ["s2"]);
        assert!(list_history(&paths, None, None, Some(HistoryKind::Tts), true, 10).unwrap().items.is_empty());
    }

    #[test]
    fn search_matches_title_and_text_by_prefix() {
        let (_dir, paths) = temp_paths();
        insert_history(&paths, &item("a", HistoryKind::Tts, 1, "Meeting notes", "quarterly budget review")).unwrap();
        insert_history(&paths, &item("b", HistoryKind::Stt, 2, "Grocery list", "apples and oranges")).unwrap();
        insert_history(&paths, &item("c", HistoryKind::Stt, 3, "Café visit", "Naïve résumé")).unwrap();

        assert_eq!(search(&paths, "meeting"), ["a"]);
        assert_eq!(search(&paths, "budg"), ["a"]);
        assert_eq!(search(&paths, "APPLES"), ["b"]);
        // All words must match.
        assert_eq!(search(&paths, "apples budget"), Vec::<String>::new());
        assert_eq!(search(&paths, "  apples   oranges "), ["b"]);
        assert_eq!(search(&paths, "café"), ["c"]);
        // Blank query means no filter.
        assert_eq!(search(&paths, "   ").len(), 3);
        // Combined with other filters.
        assert_eq!(ids(&list_history(&paths, None, Some("list".into()), Some(HistoryKind::Tts), false, 10).unwrap()), Vec::<&str>::new());
    }

    #[test]
    fn search_treats_fts_syntax_and_quotes_as_text() {
        let (_dir, paths) = temp_paths();
        insert_history(&paths, &item("a", HistoryKind::Tts, 1, "It's \"quoted\"", "rock AND roll; NEAR the end: col:x")).unwrap();
        insert_history(&paths, &item("b", HistoryKind::Tts, 2, "other", "nothing here")).unwrap();

        for q in [
            "\"", "\"\"", "'", "*", "-", "(", ")", "^", ":", "+", "!!!", "\"unterminated", "a\"b", "NOT", "AND", "OR",
            "NEAR(", "NEAR(x y)", "title:x", "col:x", "x*", "-rock", "rock OR", "{a b}", "@#$%",
            "'; DROP TABLE history; --", "\\", "%", "_",
        ] {
            // None of these may error; the table must survive.
            let _ = search(&paths, q);
        }
        assert_eq!(search(&paths, "\"quoted\""), ["a"]);
        assert_eq!(search(&paths, "it's"), ["a"]);
        assert_eq!(search(&paths, "AND"), ["a"]);
        assert_eq!(search(&paths, "NEAR"), ["a"]);
        assert_eq!(search(&paths, "col:x"), ["a"]);
        assert_eq!(search(&paths, "-rock"), ["a"]);
        // Only punctuation: nothing searchable, so nothing matches (rather than everything).
        assert_eq!(search(&paths, "!!! ***"), Vec::<String>::new());
        assert_eq!(list_history(&paths, None, None, None, false, 10).unwrap().items.len(), 2);
    }

    #[test]
    fn fts_query_building() {
        assert_eq!(fts_query(""), Search::All);
        assert_eq!(fts_query(" \t"), Search::All);
        assert_eq!(fts_query("*** --"), Search::Nothing);
        assert_eq!(fts_query("hello"), Search::Match("\"hello\"*".into()));
        assert_eq!(fts_query("a \"b\" c"), Search::Match("\"a\"* \"b\"* \"c\"*".into()));
        assert_eq!(fts_query("say\"what"), Search::Match("\"saywhat\"*".into()));
    }

    #[test]
    fn long_search_queries_are_cut() {
        // 2 000 one-letter words: only the first 1 000 characters (500 words) are searched.
        let long = "é ".repeat(1_000);
        let Search::Match(q) = fts_query(&long) else { panic!("expected a match query") };
        assert_eq!(q.matches("\"é\"*").count(), 500);
        // Exactly at the limit nothing is cut.
        let exact = format!("{}b", "a".repeat(MAX_SEARCH_CHARS - 1));
        assert_eq!(fts_query(&exact), Search::Match(format!("\"{}\"*", exact)));

        let (_dir, paths) = temp_paths();
        insert_history(&paths, &item("a", HistoryKind::Tts, 1, "alpha", "x")).unwrap();
        let huge = format!("alpha {}", "zz ".repeat(100_000));
        // Still a valid query, and the words beyond the cut do not take part.
        assert!(search(&paths, &huge).is_empty(), "the kept words include ones that do not match");
        assert_eq!(search(&paths, &format!("alpha{}", " ".repeat(5_000))), ["a"]);
    }

    #[test]
    fn search_index_follows_updates_and_deletes() {
        let (_dir, paths) = temp_paths();
        insert_history(&paths, &item("a", HistoryKind::Tts, 1, "alpha", "x")).unwrap();
        toggle_favorite(&paths, "a").unwrap();
        assert_eq!(search(&paths, "alpha"), ["a"]);
        delete_history(&paths, "a").unwrap();
        assert!(search(&paths, "alpha").is_empty());
    }

    #[test]
    fn toggle_favorite_flips_and_reports() {
        let (_dir, paths) = temp_paths();
        insert_history(&paths, &item("a", HistoryKind::Tts, 1, "t", "x")).unwrap();
        assert!(toggle_favorite(&paths, "a").unwrap());
        assert!(get_history(&paths, "a").unwrap().unwrap().favorite);
        assert!(!toggle_favorite(&paths, "a").unwrap());
        assert!(!get_history(&paths, "a").unwrap().unwrap().favorite);
        assert!(matches!(toggle_favorite(&paths, "missing"), Err(AppError::NotFound(_))));
    }

    #[test]
    fn delete_removes_only_that_item() {
        let (_dir, paths) = temp_paths();
        insert_history(&paths, &item("a", HistoryKind::Tts, 1, "t", "x")).unwrap();
        insert_history(&paths, &item("b", HistoryKind::Tts, 2, "t", "x")).unwrap();
        delete_history(&paths, "a").unwrap();
        delete_history(&paths, "missing").unwrap();
        assert_eq!(ids(&list_history(&paths, None, None, None, false, 10).unwrap()), ["b"]);
    }

    #[test]
    fn clear_keeps_favorites_and_returns_audio_paths() {
        let (_dir, paths) = temp_paths();
        insert_history(&paths, &item("a", HistoryKind::Tts, 1, "t", "x")).unwrap();
        let mut no_audio = item("b", HistoryKind::Stt, 2, "t", "x");
        no_audio.audio_path = None;
        insert_history(&paths, &no_audio).unwrap();
        let mut fav = item("c", HistoryKind::Tts, 3, "t", "x");
        fav.favorite = true;
        insert_history(&paths, &fav).unwrap();

        let deleted = clear_history(&paths).unwrap();
        assert_eq!(deleted.count, 2);
        assert_eq!(deleted.audio_paths, ["audio/2026/01/a.wav"]);
        assert_eq!(ids(&list_history(&paths, None, None, None, false, 10).unwrap()), ["c"]);
        assert_eq!(clear_history(&paths).unwrap().count, 0);
    }

    #[test]
    fn retention_prunes_old_non_favorites() {
        let (_dir, paths) = temp_paths();
        let now = chrono::Utc::now().timestamp_millis();
        let day = 86_400_000;
        insert_history(&paths, &item("new", HistoryKind::Tts, now - day, "t", "x")).unwrap();
        insert_history(&paths, &item("old", HistoryKind::Tts, now - 10 * day, "t", "x")).unwrap();
        let mut old_fav = item("old-fav", HistoryKind::Tts, now - 10 * day, "t", "x");
        old_fav.favorite = true;
        insert_history(&paths, &old_fav).unwrap();

        // 0 keeps everything.
        assert_eq!(prune_old_history(&paths, 0).unwrap().count, 0);
        let deleted = prune_old_history(&paths, 7).unwrap();
        assert_eq!(deleted.count, 1);
        assert_eq!(deleted.audio_paths, ["audio/2026/01/old.wav"]);
        let mut left: Vec<String> = list_history(&paths, None, None, None, false, 10).unwrap().items.into_iter().map(|i| i.id).collect();
        left.sort();
        assert_eq!(left, ["new", "old-fav"]);
        assert_eq!(prune_old_history(&paths, 7).unwrap().count, 0);
    }

    #[test]
    fn migrations_are_idempotent_and_versioned() {
        let (_dir, paths) = temp_paths();
        init(&paths).unwrap();
        insert_history(&paths, &item("a", HistoryKind::Tts, 1, "keep me", "x")).unwrap();

        // Force the migration to run again on the same file, as a fresh process would.
        for _ in 0..2 {
            let mut conn = Connection::open(&paths.db_file).unwrap();
            migrate(&mut conn).unwrap();
            let v: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
            assert_eq!(v, SCHEMA_VERSION);
        }
        // Even from version 0 on an existing schema (IF NOT EXISTS everywhere).
        {
            let mut conn = Connection::open(&paths.db_file).unwrap();
            conn.pragma_update(None, "user_version", 0).unwrap();
            migrate(&mut conn).unwrap();
        }
        assert_eq!(get_history(&paths, "a").unwrap().unwrap().title, "keep me");
        assert_eq!(search(&paths, "keep"), ["a"]);
    }

    #[test]
    fn failed_migration_rolls_back() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("t.db");
        let mut conn = Connection::open(&file).unwrap();
        // A conflicting object named like one the schema creates makes the batch fail midway.
        conn.execute_batch("CREATE VIEW idx_history_kind AS SELECT 1;").unwrap();
        assert!(migrate(&mut conn).is_err());
        let v: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, 0, "the version must not move when the migration fails");
        let history_exists: bool = conn
            .query_row("SELECT count(*) FROM sqlite_master WHERE name = 'history'", [], |r| r.get::<_, i32>(0))
            .map(|n| n > 0)
            .unwrap();
        assert!(!history_exists, "the partial schema must be rolled back");
    }

    #[test]
    fn newer_schema_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = Connection::open(dir.path().join("t.db")).unwrap();
        conn.pragma_update(None, "user_version", SCHEMA_VERSION + 5).unwrap();
        migrate(&mut conn).unwrap();
        let v: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, SCHEMA_VERSION + 5);
    }

    #[test]
    fn schema_is_created_lazily_without_init() {
        let (_dir, paths) = temp_paths();
        // No init(): the first call brings the schema up.
        assert!(list_history(&paths, None, None, None, false, 10).unwrap().items.is_empty());
        insert_history(&paths, &item("a", HistoryKind::Tts, 1, "t", "x")).unwrap();
        assert!(get_history(&paths, "a").unwrap().is_some());
    }

    #[test]
    fn unopenable_database_is_a_clear_error_and_retried() {
        let (_dir, paths) = temp_paths();
        // A folder where the database file should be.
        std::fs::create_dir(&paths.db_file).unwrap();
        let err = init(&paths).unwrap_err().to_string();
        assert!(err.contains("history database"), "{err}");
        // Once the problem is gone, the next call succeeds without a restart.
        std::fs::remove_dir(&paths.db_file).unwrap();
        init(&paths).unwrap();
        insert_history(&paths, &item("a", HistoryKind::Tts, 1, "t", "x")).unwrap();
    }
}
