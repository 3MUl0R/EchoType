pub mod history;
pub mod latency;
pub mod metrics;
pub mod migrations;
pub mod models;
pub mod profiles;
pub mod settings;
pub mod vocabulary;

use std::path::Path;
use std::sync::Arc;

use rusqlite::Connection;
use tokio::sync::Mutex;
use tracing::info;

/// Thread-safe database handle.
pub type DbHandle = Arc<Mutex<Connection>>;

/// Open the SQLite database at the given path, run migrations, and return a handle.
pub fn open(path: &Path) -> Result<DbHandle, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Cannot create database directory: {e}"))?;
    }

    let conn = Connection::open(path)
        .map_err(|e| format!("Cannot open database at {}: {e}", path.display()))?;

    // Enable WAL mode for concurrent reads
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(|e| format!("Cannot set WAL mode: {e}"))?;

    // Enable foreign key enforcement (required for CASCADE deletes)
    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(|e| format!("Cannot enable foreign keys: {e}"))?;

    // Run migrations
    migrations::run(&conn)?;

    info!(path = %path.display(), "Database opened");
    Ok(Arc::new(Mutex::new(conn)))
}

/// Open an in-memory database for testing.
#[cfg(test)]
pub fn open_memory() -> DbHandle {
    let conn = Connection::open_in_memory().expect("in-memory DB");
    conn.pragma_update(None, "journal_mode", "WAL").ok();
    migrations::run(&conn).expect("migrations");
    Arc::new(Mutex::new(conn))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_memory_works() {
        let db = open_memory();
        let conn = db.blocking_lock();
        let version: u32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert!(version >= 1);
    }
}
