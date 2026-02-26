use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// A dictation history entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: i64,
    pub text: String,
    pub audio_path: Option<String>,
    pub duration_ms: Option<i64>,
    pub engine_id: Option<String>,
    pub language: Option<String>,
    pub words_per_minute: Option<f64>,
    pub created_at: String,
    pub is_private: bool,
}

/// Parameters for inserting a new history entry.
pub struct InsertParams<'a> {
    pub text: &'a str,
    pub audio_path: Option<&'a str>,
    pub duration_ms: Option<i64>,
    pub engine_id: Option<&'a str>,
    pub language: Option<&'a str>,
    pub words_per_minute: Option<f64>,
    pub created_at: &'a str,
}

/// Insert a new dictation history entry. Returns the row ID.
pub fn insert(conn: &Connection, params: &InsertParams) -> Result<i64, String> {
    conn.execute(
        "INSERT INTO dictation_history (text, audio_path, duration_ms, engine_id, language, words_per_minute, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            params.text,
            params.audio_path,
            params.duration_ms,
            params.engine_id,
            params.language,
            params.words_per_minute,
            params.created_at,
        ],
    )
    .map_err(|e| format!("Failed to insert history entry: {e}"))?;

    Ok(conn.last_insert_rowid())
}

/// Get a history entry by ID.
pub fn get_by_id(conn: &Connection, id: i64) -> Result<Option<HistoryEntry>, String> {
    let result = conn.query_row(
        "SELECT id, text, audio_path, duration_ms, engine_id, language, words_per_minute, created_at, is_private
         FROM dictation_history WHERE id = ?1",
        [id],
        row_to_entry,
    );

    match result {
        Ok(entry) => Ok(Some(entry)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(format!("Failed to get history entry: {e}")),
    }
}

/// Get history entries ordered by most recent, with pagination.
pub fn list(conn: &Connection, limit: i64, offset: i64) -> Result<Vec<HistoryEntry>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, text, audio_path, duration_ms, engine_id, language, words_per_minute, created_at, is_private
             FROM dictation_history
             ORDER BY created_at DESC
             LIMIT ?1 OFFSET ?2",
        )
        .map_err(|e| format!("Failed to prepare history query: {e}"))?;

    let entries = stmt
        .query_map([limit, offset], row_to_entry)
        .map_err(|e| format!("Failed to query history: {e}"))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(entries)
}

/// Delete a history entry by ID. Returns the audio path if one existed.
pub fn delete(conn: &Connection, id: i64) -> Result<Option<String>, String> {
    let audio_path: Option<String> = conn
        .query_row(
            "SELECT audio_path FROM dictation_history WHERE id = ?1",
            [id],
            |row| row.get::<_, Option<String>>(0),
        )
        .ok()
        .flatten();

    conn.execute("DELETE FROM dictation_history WHERE id = ?1", [id])
        .map_err(|e| format!("Failed to delete history entry: {e}"))?;

    Ok(audio_path)
}

/// Delete all history entries. Returns audio paths for cleanup.
pub fn clear(conn: &Connection) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare("SELECT audio_path FROM dictation_history WHERE audio_path IS NOT NULL")
        .map_err(|e| format!("Failed to query audio paths: {e}"))?;

    let paths: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .map_err(|e| format!("Failed to read audio paths: {e}"))?
        .filter_map(|r| r.ok())
        .collect();

    conn.execute("DELETE FROM dictation_history", [])
        .map_err(|e| format!("Failed to clear history: {e}"))?;

    Ok(paths)
}

/// Get total history entry count.
#[allow(dead_code)]
pub fn count(conn: &Connection) -> Result<i64, String> {
    conn.query_row("SELECT COUNT(*) FROM dictation_history", [], |row| {
        row.get(0)
    })
    .map_err(|e| format!("Failed to count history: {e}"))
}

/// Enforce retention by keeping only the most recent `max_count` entries.
/// Returns audio paths of deleted entries for cleanup.
pub fn enforce_retention_count(conn: &Connection, max_count: i64) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT audio_path FROM dictation_history
             WHERE audio_path IS NOT NULL
               AND id NOT IN (
                 SELECT id FROM dictation_history ORDER BY created_at DESC LIMIT ?1
               )",
        )
        .map_err(|e| format!("Failed to query retention overflow: {e}"))?;

    let paths: Vec<String> = stmt
        .query_map([max_count], |row| row.get(0))
        .map_err(|e| format!("Failed to read overflow paths: {e}"))?
        .filter_map(|r| r.ok())
        .collect();

    conn.execute(
        "DELETE FROM dictation_history
         WHERE id NOT IN (
           SELECT id FROM dictation_history ORDER BY created_at DESC LIMIT ?1
         )",
        [max_count],
    )
    .map_err(|e| format!("Failed to enforce retention: {e}"))?;

    Ok(paths)
}

/// Enforce retention by deleting entries older than the given ISO 8601 cutoff.
/// Returns audio paths of deleted entries for cleanup.
#[allow(dead_code)]
pub fn enforce_retention_age(conn: &Connection, cutoff_utc: &str) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT audio_path FROM dictation_history
             WHERE audio_path IS NOT NULL AND created_at < ?1",
        )
        .map_err(|e| format!("Failed to query age retention: {e}"))?;

    let paths: Vec<String> = stmt
        .query_map([cutoff_utc], |row| row.get(0))
        .map_err(|e| format!("Failed to read aged paths: {e}"))?
        .filter_map(|r| r.ok())
        .collect();

    conn.execute(
        "DELETE FROM dictation_history WHERE created_at < ?1",
        [cutoff_utc],
    )
    .map_err(|e| format!("Failed to enforce age retention: {e}"))?;

    Ok(paths)
}

fn row_to_entry(row: &rusqlite::Row) -> rusqlite::Result<HistoryEntry> {
    Ok(HistoryEntry {
        id: row.get(0)?,
        text: row.get(1)?,
        audio_path: row.get(2)?,
        duration_ms: row.get(3)?,
        engine_id: row.get(4)?,
        language: row.get(5)?,
        words_per_minute: row.get(6)?,
        created_at: row.get(7)?,
        is_private: row.get::<_, i32>(8)? != 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn sample_params() -> InsertParams<'static> {
        InsertParams {
            text: "Hello world",
            audio_path: Some("audio/test.wav"),
            duration_ms: Some(1500),
            engine_id: Some("whisper-tiny-en"),
            language: Some("en"),
            words_per_minute: Some(120.0),
            created_at: "2026-02-25T12:00:00Z",
        }
    }

    #[test]
    fn insert_and_get() {
        let handle = db::open_memory();
        let conn = handle.blocking_lock();
        let id = insert(&conn, &sample_params()).unwrap();
        let entry = get_by_id(&conn, id).unwrap().unwrap();
        assert_eq!(entry.text, "Hello world");
        assert_eq!(entry.audio_path, Some("audio/test.wav".to_string()));
        assert_eq!(entry.duration_ms, Some(1500));
    }

    #[test]
    fn list_returns_recent_first() {
        let handle = db::open_memory();
        let conn = handle.blocking_lock();
        let mut p = sample_params();
        p.created_at = "2026-02-25T10:00:00Z";
        p.text = "First";
        insert(&conn, &p).unwrap();
        p.created_at = "2026-02-25T12:00:00Z";
        p.text = "Second";
        insert(&conn, &p).unwrap();

        let entries = list(&conn, 10, 0).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].text, "Second"); // Most recent first
        assert_eq!(entries[1].text, "First");
    }

    #[test]
    fn delete_returns_audio_path() {
        let handle = db::open_memory();
        let conn = handle.blocking_lock();
        let id = insert(&conn, &sample_params()).unwrap();
        let audio = delete(&conn, id).unwrap();
        assert_eq!(audio, Some("audio/test.wav".to_string()));
        assert!(get_by_id(&conn, id).unwrap().is_none());
    }

    #[test]
    fn enforce_retention_keeps_recent() {
        let handle = db::open_memory();
        let conn = handle.blocking_lock();
        let timestamps = [
            "2026-02-25T10:00:00Z",
            "2026-02-25T11:00:00Z",
            "2026-02-25T12:00:00Z",
            "2026-02-25T13:00:00Z",
            "2026-02-25T14:00:00Z",
        ];
        let texts = ["Entry 0", "Entry 1", "Entry 2", "Entry 3", "Entry 4"];
        for i in 0..5 {
            let p = InsertParams {
                text: texts[i],
                audio_path: Some("audio/test.wav"),
                duration_ms: Some(1500),
                engine_id: Some("whisper-tiny-en"),
                language: Some("en"),
                words_per_minute: Some(120.0),
                created_at: timestamps[i],
            };
            insert(&conn, &p).unwrap();
        }

        let deleted_paths = enforce_retention_count(&conn, 2).unwrap();
        assert_eq!(deleted_paths.len(), 3); // 5 - 2 = 3 deleted
        assert_eq!(count(&conn).unwrap(), 2);

        let remaining = list(&conn, 10, 0).unwrap();
        assert_eq!(remaining[0].text, "Entry 4");
        assert_eq!(remaining[1].text, "Entry 3");
    }

    #[test]
    fn test_enforce_retention_age() {
        let handle = db::open_memory();
        let conn = handle.blocking_lock();
        let mut p = sample_params();
        p.created_at = "2026-02-20T12:00:00Z";
        p.text = "Old";
        insert(&conn, &p).unwrap();
        p.created_at = "2026-02-25T12:00:00Z";
        p.text = "Recent";
        insert(&conn, &p).unwrap();

        let deleted = super::enforce_retention_age(&conn, "2026-02-24T00:00:00Z").unwrap();
        assert_eq!(deleted.len(), 1);
        assert_eq!(count(&conn).unwrap(), 1);
        let remaining = list(&conn, 10, 0).unwrap();
        assert_eq!(remaining[0].text, "Recent");
    }

    #[test]
    fn clear_returns_all_audio_paths() {
        let handle = db::open_memory();
        let conn = handle.blocking_lock();
        insert(&conn, &sample_params()).unwrap();
        insert(&conn, &sample_params()).unwrap();

        let paths = clear(&conn).unwrap();
        assert_eq!(paths.len(), 2);
        assert_eq!(count(&conn).unwrap(), 0);
    }
}
