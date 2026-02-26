use rusqlite::Connection;

/// Get a setting value by key.
pub fn get(conn: &Connection, key: &str) -> Result<Option<String>, String> {
    conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
        row.get(0)
    })
    .optional()
    .map_err(|e| format!("Failed to read setting '{key}': {e}"))
}

/// Set a setting value (insert or update).
pub fn set(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [key, value],
    )
    .map_err(|e| format!("Failed to write setting '{key}': {e}"))?;
    Ok(())
}

/// Delete a setting (reset to default).
pub fn delete(conn: &Connection, key: &str) -> Result<(), String> {
    conn.execute("DELETE FROM settings WHERE key = ?1", [key])
        .map_err(|e| format!("Failed to delete setting '{key}': {e}"))?;
    Ok(())
}

/// Get all settings as key-value pairs.
pub fn get_all(conn: &Connection) -> Result<Vec<(String, String)>, String> {
    let mut stmt = conn
        .prepare("SELECT key, value FROM settings ORDER BY key")
        .map_err(|e| format!("Failed to query settings: {e}"))?;

    let rows = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|e| format!("Failed to read settings: {e}"))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(rows)
}

/// Trait to allow `query_row` to return `Option<T>`.
trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn get_missing_key_returns_none() {
        let handle = db::open_memory();
        let conn = handle.blocking_lock();
        assert_eq!(get(&conn, "nonexistent").unwrap(), None);
    }

    #[test]
    fn set_and_get() {
        let handle = db::open_memory();
        let conn = handle.blocking_lock();
        set(&conn, "hotkey", "\"CmdOrCtrl+Shift+Space\"").unwrap();
        assert_eq!(
            get(&conn, "hotkey").unwrap(),
            Some("\"CmdOrCtrl+Shift+Space\"".to_string())
        );
    }

    #[test]
    fn set_overwrites() {
        let handle = db::open_memory();
        let conn = handle.blocking_lock();
        set(&conn, "lang", "\"en\"").unwrap();
        set(&conn, "lang", "\"fr\"").unwrap();
        assert_eq!(get(&conn, "lang").unwrap(), Some("\"fr\"".to_string()));
    }

    #[test]
    fn delete_removes() {
        let handle = db::open_memory();
        let conn = handle.blocking_lock();
        set(&conn, "key", "\"val\"").unwrap();
        delete(&conn, "key").unwrap();
        assert_eq!(get(&conn, "key").unwrap(), None);
    }

    #[test]
    fn get_all_returns_pairs() {
        let handle = db::open_memory();
        let conn = handle.blocking_lock();
        set(&conn, "a", "1").unwrap();
        set(&conn, "b", "2").unwrap();
        let all = get_all(&conn).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0], ("a".to_string(), "1".to_string()));
        assert_eq!(all[1], ("b".to_string(), "2".to_string()));
    }
}
