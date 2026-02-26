use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// Model-specific preferences stored in the database.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPreference {
    pub model_id: String,
    pub language: Option<String>,
    pub last_used_at: Option<String>,
}

/// Get preferences for a model.
#[allow(dead_code)]
pub fn get(conn: &Connection, model_id: &str) -> Result<Option<ModelPreference>, String> {
    let result = conn.query_row(
        "SELECT model_id, language, last_used_at FROM model_preferences WHERE model_id = ?1",
        [model_id],
        |row| {
            Ok(ModelPreference {
                model_id: row.get(0)?,
                language: row.get(1)?,
                last_used_at: row.get(2)?,
            })
        },
    );

    match result {
        Ok(pref) => Ok(Some(pref)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(format!("Failed to get model preferences: {e}")),
    }
}

/// Update or insert model preferences.
#[allow(dead_code)]
pub fn upsert(conn: &Connection, pref: &ModelPreference) -> Result<(), String> {
    conn.execute(
        "INSERT INTO model_preferences (model_id, language, last_used_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(model_id) DO UPDATE SET
           language = COALESCE(excluded.language, model_preferences.language),
           last_used_at = COALESCE(excluded.last_used_at, model_preferences.last_used_at)",
        rusqlite::params![pref.model_id, pref.language, pref.last_used_at],
    )
    .map_err(|e| format!("Failed to upsert model preference: {e}"))?;
    Ok(())
}

/// Update the last_used_at timestamp for a model.
#[allow(dead_code)]
pub fn touch(conn: &Connection, model_id: &str, timestamp: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO model_preferences (model_id, last_used_at) VALUES (?1, ?2)
         ON CONFLICT(model_id) DO UPDATE SET last_used_at = excluded.last_used_at",
        [model_id, timestamp],
    )
    .map_err(|e| format!("Failed to touch model: {e}"))?;
    Ok(())
}

/// Delete model preferences.
#[allow(dead_code)]
pub fn delete(conn: &Connection, model_id: &str) -> Result<(), String> {
    conn.execute(
        "DELETE FROM model_preferences WHERE model_id = ?1",
        [model_id],
    )
    .map_err(|e| format!("Failed to delete model preference: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn get_missing_returns_none() {
        let handle = db::open_memory();
        let conn = handle.blocking_lock();
        assert!(get(&conn, "nonexistent").unwrap().is_none());
    }

    #[test]
    fn upsert_and_get() {
        let handle = db::open_memory();
        let conn = handle.blocking_lock();
        let pref = ModelPreference {
            model_id: "whisper-tiny-en".to_string(),
            language: Some("en".to_string()),
            last_used_at: Some("2026-02-25T12:00:00Z".to_string()),
        };
        upsert(&conn, &pref).unwrap();

        let retrieved = get(&conn, "whisper-tiny-en").unwrap().unwrap();
        assert_eq!(retrieved.language, Some("en".to_string()));
    }

    #[test]
    fn touch_updates_timestamp() {
        let handle = db::open_memory();
        let conn = handle.blocking_lock();
        touch(&conn, "test-model", "2026-02-25T10:00:00Z").unwrap();
        touch(&conn, "test-model", "2026-02-25T12:00:00Z").unwrap();

        let pref = get(&conn, "test-model").unwrap().unwrap();
        assert_eq!(pref.last_used_at, Some("2026-02-25T12:00:00Z".to_string()));
    }

    #[test]
    fn delete_removes() {
        let handle = db::open_memory();
        let conn = handle.blocking_lock();
        touch(&conn, "model-1", "2026-02-25T12:00:00Z").unwrap();
        delete(&conn, "model-1").unwrap();
        assert!(get(&conn, "model-1").unwrap().is_none());
    }
}
