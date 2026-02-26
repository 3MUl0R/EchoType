use rusqlite::Connection;
use tracing::info;

/// Current schema version.
const CURRENT_VERSION: u32 = 1;

/// Run all pending migrations.
pub fn run(conn: &Connection) -> Result<(), String> {
    let version: u32 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|e| format!("Cannot read schema version: {e}"))?;

    if version >= CURRENT_VERSION {
        return Ok(());
    }

    info!(
        from = version,
        to = CURRENT_VERSION,
        "Running database migrations"
    );

    if version < 1 {
        migrate_v1(conn)?;
    }

    conn.pragma_update(None, "user_version", CURRENT_VERSION)
        .map_err(|e| format!("Cannot update schema version: {e}"))?;

    Ok(())
}

/// V1: Initial schema — settings, dictation history, model preferences.
fn migrate_v1(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS settings (
            key   TEXT PRIMARY KEY NOT NULL,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS dictation_history (
            id               INTEGER PRIMARY KEY AUTOINCREMENT,
            text             TEXT NOT NULL,
            audio_path       TEXT,
            duration_ms      INTEGER,
            engine_id        TEXT,
            language         TEXT,
            words_per_minute REAL,
            created_at       TEXT NOT NULL,
            is_private       INTEGER NOT NULL DEFAULT 0
        );

        CREATE INDEX IF NOT EXISTS idx_history_created_at
            ON dictation_history(created_at);

        CREATE TABLE IF NOT EXISTS model_preferences (
            model_id     TEXT PRIMARY KEY NOT NULL,
            language     TEXT,
            last_used_at TEXT
        );
        ",
    )
    .map_err(|e| format!("Migration v1 failed: {e}"))?;

    info!("Applied migration v1");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        run(&conn).unwrap();
        run(&conn).unwrap(); // Second run should be a no-op

        let version: u32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, CURRENT_VERSION);
    }

    #[test]
    fn v1_creates_tables() {
        let conn = Connection::open_in_memory().unwrap();
        run(&conn).unwrap();

        // Verify tables exist by querying sqlite_master
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        assert!(tables.contains(&"settings".to_string()));
        assert!(tables.contains(&"dictation_history".to_string()));
        assert!(tables.contains(&"model_preferences".to_string()));
    }
}
