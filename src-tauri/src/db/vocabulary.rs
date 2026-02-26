use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// A vocabulary collection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VocabularyCollection {
    pub id: i64,
    pub name: String,
    pub created_at: String,
}

/// A vocabulary entry: one correction with one or more aliases (misheard variants).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VocabularyEntry {
    pub id: i64,
    pub collection_id: i64,
    pub correction: String,
    /// JSON array of alias strings.
    pub aliases: Vec<String>,
}

/// Create a new vocabulary collection. Returns the collection ID.
pub fn create_collection(conn: &Connection, name: &str) -> Result<i64, String> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO vocabulary_collections (name, created_at) VALUES (?1, ?2)",
        rusqlite::params![name, now],
    )
    .map_err(|e| format!("Failed to create vocabulary collection: {e}"))?;
    Ok(conn.last_insert_rowid())
}

/// List all vocabulary collections.
pub fn list_collections(conn: &Connection) -> Result<Vec<VocabularyCollection>, String> {
    let mut stmt = conn
        .prepare("SELECT id, name, created_at FROM vocabulary_collections ORDER BY name")
        .map_err(|e| format!("Failed to query vocabulary collections: {e}"))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(VocabularyCollection {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at: row.get(2)?,
            })
        })
        .map_err(|e| format!("Failed to fetch vocabulary collections: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect vocabulary collections: {e}"))
}

/// Delete a vocabulary collection and all its entries (CASCADE).
pub fn delete_collection(conn: &Connection, id: i64) -> Result<(), String> {
    conn.execute("DELETE FROM vocabulary_collections WHERE id = ?1", [id])
        .map_err(|e| format!("Failed to delete vocabulary collection: {e}"))?;
    Ok(())
}

/// Rename a vocabulary collection.
pub fn rename_collection(conn: &Connection, id: i64, name: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE vocabulary_collections SET name = ?1 WHERE id = ?2",
        rusqlite::params![name, id],
    )
    .map_err(|e| format!("Failed to rename vocabulary collection: {e}"))?;
    Ok(())
}

/// Add an entry to a vocabulary collection. Returns the entry ID.
pub fn add_entry(
    conn: &Connection,
    collection_id: i64,
    correction: &str,
    aliases: &[String],
) -> Result<i64, String> {
    let aliases_json =
        serde_json::to_string(aliases).map_err(|e| format!("Failed to serialize aliases: {e}"))?;
    conn.execute(
        "INSERT INTO vocabulary_entries (collection_id, correction, aliases) VALUES (?1, ?2, ?3)",
        rusqlite::params![collection_id, correction, aliases_json],
    )
    .map_err(|e| format!("Failed to add vocabulary entry: {e}"))?;
    Ok(conn.last_insert_rowid())
}

/// Update an existing vocabulary entry.
pub fn update_entry(
    conn: &Connection,
    id: i64,
    correction: &str,
    aliases: &[String],
) -> Result<(), String> {
    let aliases_json =
        serde_json::to_string(aliases).map_err(|e| format!("Failed to serialize aliases: {e}"))?;
    conn.execute(
        "UPDATE vocabulary_entries SET correction = ?1, aliases = ?2 WHERE id = ?3",
        rusqlite::params![correction, aliases_json, id],
    )
    .map_err(|e| format!("Failed to update vocabulary entry: {e}"))?;
    Ok(())
}

/// Delete a vocabulary entry.
pub fn delete_entry(conn: &Connection, id: i64) -> Result<(), String> {
    conn.execute("DELETE FROM vocabulary_entries WHERE id = ?1", [id])
        .map_err(|e| format!("Failed to delete vocabulary entry: {e}"))?;
    Ok(())
}

/// List all entries in a vocabulary collection.
pub fn list_entries(conn: &Connection, collection_id: i64) -> Result<Vec<VocabularyEntry>, String> {
    let mut stmt = conn
        .prepare("SELECT id, collection_id, correction, aliases FROM vocabulary_entries WHERE collection_id = ?1 ORDER BY correction")
        .map_err(|e| format!("Failed to query vocabulary entries: {e}"))?;

    let rows = stmt
        .query_map([collection_id], |row| {
            let aliases_str: String = row.get(3)?;
            let aliases: Vec<String> = serde_json::from_str(&aliases_str).unwrap_or_default();
            Ok(VocabularyEntry {
                id: row.get(0)?,
                collection_id: row.get(1)?,
                correction: row.get(2)?,
                aliases,
            })
        })
        .map_err(|e| format!("Failed to fetch vocabulary entries: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect vocabulary entries: {e}"))
}

/// Get the entry count for a collection.
pub fn entry_count(conn: &Connection, collection_id: i64) -> Result<i64, String> {
    conn.query_row(
        "SELECT COUNT(*) FROM vocabulary_entries WHERE collection_id = ?1",
        [collection_id],
        |row| row.get(0),
    )
    .map_err(|e| format!("Failed to count vocabulary entries: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn
    }

    #[test]
    fn collection_crud() {
        let conn = setup();
        let id = create_collection(&conn, "Medical Terms").unwrap();
        assert!(id > 0);

        let collections = list_collections(&conn).unwrap();
        assert_eq!(collections.len(), 1);
        assert_eq!(collections[0].name, "Medical Terms");

        rename_collection(&conn, id, "Medical").unwrap();
        let collections = list_collections(&conn).unwrap();
        assert_eq!(collections[0].name, "Medical");

        delete_collection(&conn, id).unwrap();
        let collections = list_collections(&conn).unwrap();
        assert!(collections.is_empty());
    }

    #[test]
    fn entry_crud() {
        let conn = setup();
        let cid = create_collection(&conn, "Tech").unwrap();

        let eid = add_entry(
            &conn,
            cid,
            "EchoType",
            &["echo type".to_string(), "eco type".to_string()],
        )
        .unwrap();
        assert!(eid > 0);

        let entries = list_entries(&conn, cid).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].correction, "EchoType");
        assert_eq!(entries[0].aliases.len(), 2);

        update_entry(
            &conn,
            eid,
            "EchoType",
            &[
                "echo type".to_string(),
                "eco type".to_string(),
                "ekko type".to_string(),
            ],
        )
        .unwrap();
        let entries = list_entries(&conn, cid).unwrap();
        assert_eq!(entries[0].aliases.len(), 3);

        assert_eq!(entry_count(&conn, cid).unwrap(), 1);

        delete_entry(&conn, eid).unwrap();
        assert_eq!(entry_count(&conn, cid).unwrap(), 0);
    }

    #[test]
    fn delete_collection_cascades_entries() {
        let conn = setup();
        let cid = create_collection(&conn, "Test").unwrap();
        add_entry(&conn, cid, "word", &["wrd".to_string()]).unwrap();
        assert_eq!(entry_count(&conn, cid).unwrap(), 1);

        delete_collection(&conn, cid).unwrap();
        // Entries should be deleted via CASCADE
        assert_eq!(entry_count(&conn, cid).unwrap(), 0);
    }
}
