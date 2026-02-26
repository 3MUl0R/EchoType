use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// Identifies how the app was matched.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppIdentifierType {
    BundleId,
    ExePath,
    WmClass,
}

impl AppIdentifierType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::BundleId => "bundle_id",
            Self::ExePath => "exe_path",
            Self::WmClass => "wm_class",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "bundle_id" => Self::BundleId,
            "exe_path" => Self::ExePath,
            "wm_class" => Self::WmClass,
            _ => Self::BundleId,
        }
    }
}

/// A per-application profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: i64,
    pub name: String,
    pub app_identifier: String,
    pub app_identifier_type: AppIdentifierType,
    pub created_at: String,
}

/// Parameters for creating a new profile.
pub struct CreateProfileParams<'a> {
    pub name: &'a str,
    pub app_identifier: &'a str,
    pub app_identifier_type: &'a AppIdentifierType,
}

/// Create a new profile. Returns the profile ID.
pub fn create(conn: &Connection, params: &CreateProfileParams) -> Result<i64, String> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO profiles (name, app_identifier, app_identifier_type, created_at) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![params.name, params.app_identifier, params.app_identifier_type.as_str(), now],
    )
    .map_err(|e| format!("Failed to create profile: {e}"))?;
    Ok(conn.last_insert_rowid())
}

/// List all profiles.
pub fn list(conn: &Connection) -> Result<Vec<Profile>, String> {
    let mut stmt = conn
        .prepare("SELECT id, name, app_identifier, app_identifier_type, created_at FROM profiles ORDER BY name")
        .map_err(|e| format!("Failed to query profiles: {e}"))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(Profile {
                id: row.get(0)?,
                name: row.get(1)?,
                app_identifier: row.get(2)?,
                app_identifier_type: AppIdentifierType::from_str(&row.get::<_, String>(3)?),
                created_at: row.get(4)?,
            })
        })
        .map_err(|e| format!("Failed to fetch profiles: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect profiles: {e}"))
}

/// Get a profile by ID.
pub fn get_by_id(conn: &Connection, id: i64) -> Result<Option<Profile>, String> {
    let result = conn.query_row(
        "SELECT id, name, app_identifier, app_identifier_type, created_at FROM profiles WHERE id = ?1",
        [id],
        |row| {
            Ok(Profile {
                id: row.get(0)?,
                name: row.get(1)?,
                app_identifier: row.get(2)?,
                app_identifier_type: AppIdentifierType::from_str(
                    &row.get::<_, String>(3)?,
                ),
                created_at: row.get(4)?,
            })
        },
    );

    match result {
        Ok(p) => Ok(Some(p)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(format!("Failed to get profile: {e}")),
    }
}

/// Find a profile matching the given app identifier.
pub fn find_by_app(
    conn: &Connection,
    app_identifier: &str,
    id_type: &AppIdentifierType,
) -> Result<Option<Profile>, String> {
    let result = conn.query_row(
        "SELECT id, name, app_identifier, app_identifier_type, created_at FROM profiles WHERE app_identifier = ?1 AND app_identifier_type = ?2",
        rusqlite::params![app_identifier, id_type.as_str()],
        |row| {
            Ok(Profile {
                id: row.get(0)?,
                name: row.get(1)?,
                app_identifier: row.get(2)?,
                app_identifier_type: AppIdentifierType::from_str(
                    &row.get::<_, String>(3)?,
                ),
                created_at: row.get(4)?,
            })
        },
    );

    match result {
        Ok(p) => Ok(Some(p)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(format!("Failed to find profile: {e}")),
    }
}

/// Delete a profile and its settings.
pub fn delete(conn: &Connection, id: i64) -> Result<(), String> {
    // CASCADE handles profile_settings
    conn.execute("DELETE FROM profiles WHERE id = ?1", [id])
        .map_err(|e| format!("Failed to delete profile: {e}"))?;
    Ok(())
}

/// Update a profile's name and app identifier.
pub fn update(
    conn: &Connection,
    id: i64,
    name: &str,
    app_identifier: &str,
    id_type: &AppIdentifierType,
) -> Result<(), String> {
    conn.execute(
        "UPDATE profiles SET name = ?1, app_identifier = ?2, app_identifier_type = ?3 WHERE id = ?4",
        rusqlite::params![name, app_identifier, id_type.as_str(), id],
    )
    .map_err(|e| format!("Failed to update profile: {e}"))?;
    Ok(())
}

/// Get a setting override for a specific profile.
pub fn get_setting(
    conn: &Connection,
    profile_id: i64,
    key: &str,
) -> Result<Option<String>, String> {
    let result = conn.query_row(
        "SELECT value FROM profile_settings WHERE profile_id = ?1 AND key = ?2",
        rusqlite::params![profile_id, key],
        |row| row.get(0),
    );

    match result {
        Ok(v) => Ok(Some(v)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(format!("Failed to get profile setting: {e}")),
    }
}

/// Set a setting override for a specific profile.
pub fn set_setting(
    conn: &Connection,
    profile_id: i64,
    key: &str,
    value: &str,
) -> Result<(), String> {
    conn.execute(
        "INSERT OR REPLACE INTO profile_settings (profile_id, key, value) VALUES (?1, ?2, ?3)",
        rusqlite::params![profile_id, key, value],
    )
    .map_err(|e| format!("Failed to set profile setting: {e}"))?;
    Ok(())
}

/// Remove a setting override for a profile (falls back to global default).
pub fn remove_setting(conn: &Connection, profile_id: i64, key: &str) -> Result<(), String> {
    conn.execute(
        "DELETE FROM profile_settings WHERE profile_id = ?1 AND key = ?2",
        rusqlite::params![profile_id, key],
    )
    .map_err(|e| format!("Failed to remove profile setting: {e}"))?;
    Ok(())
}

/// Get all setting overrides for a profile.
pub fn get_all_settings(
    conn: &Connection,
    profile_id: i64,
) -> Result<Vec<(String, String)>, String> {
    let mut stmt = conn
        .prepare("SELECT key, value FROM profile_settings WHERE profile_id = ?1 ORDER BY key")
        .map_err(|e| format!("Failed to query profile settings: {e}"))?;

    let rows = stmt
        .query_map([profile_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| format!("Failed to fetch profile settings: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect profile settings: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        // Enable foreign keys
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn
    }

    #[test]
    fn create_and_list() {
        let conn = setup();
        let id = create(
            &conn,
            &CreateProfileParams {
                name: "Safari",
                app_identifier: "com.apple.Safari",
                app_identifier_type: &AppIdentifierType::BundleId,
            },
        )
        .unwrap();
        assert!(id > 0);

        let profiles = list(&conn).unwrap();
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].name, "Safari");
        assert_eq!(profiles[0].app_identifier, "com.apple.Safari");
    }

    #[test]
    fn find_by_app_works() {
        let conn = setup();
        create(
            &conn,
            &CreateProfileParams {
                name: "VS Code",
                app_identifier: "com.microsoft.VSCode",
                app_identifier_type: &AppIdentifierType::BundleId,
            },
        )
        .unwrap();

        let found =
            find_by_app(&conn, "com.microsoft.VSCode", &AppIdentifierType::BundleId).unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "VS Code");

        let not_found = find_by_app(&conn, "com.other.App", &AppIdentifierType::BundleId).unwrap();
        assert!(not_found.is_none());
    }

    #[test]
    fn delete_cascades_settings() {
        let conn = setup();
        let id = create(
            &conn,
            &CreateProfileParams {
                name: "Test",
                app_identifier: "com.test.app",
                app_identifier_type: &AppIdentifierType::BundleId,
            },
        )
        .unwrap();

        set_setting(&conn, id, "output_method", "\"clipboard_only\"").unwrap();
        assert!(get_setting(&conn, id, "output_method").unwrap().is_some());

        delete(&conn, id).unwrap();
        // Settings should be gone too
        assert!(get_setting(&conn, id, "output_method").unwrap().is_none());
    }

    #[test]
    fn profile_settings_crud() {
        let conn = setup();
        let id = create(
            &conn,
            &CreateProfileParams {
                name: "Test",
                app_identifier: "com.test.app",
                app_identifier_type: &AppIdentifierType::BundleId,
            },
        )
        .unwrap();

        // Set
        set_setting(&conn, id, "output_method", "\"direct_input\"").unwrap();
        set_setting(&conn, id, "auto_punctuate", "true").unwrap();

        // Get
        let val = get_setting(&conn, id, "output_method").unwrap().unwrap();
        assert_eq!(val, "\"direct_input\"");

        // Get all
        let all = get_all_settings(&conn, id).unwrap();
        assert_eq!(all.len(), 2);

        // Remove
        remove_setting(&conn, id, "auto_punctuate").unwrap();
        let all = get_all_settings(&conn, id).unwrap();
        assert_eq!(all.len(), 1);
    }

    #[test]
    fn identifier_type_roundtrip() {
        for t in [
            AppIdentifierType::BundleId,
            AppIdentifierType::ExePath,
            AppIdentifierType::WmClass,
        ] {
            assert_eq!(AppIdentifierType::from_str(t.as_str()), t);
        }
    }
}
