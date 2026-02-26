use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::db;

/// All known setting keys.
pub mod keys {
    pub const HOTKEY: &str = "hotkey";
    pub const OUTPUT_METHOD: &str = "output_method";
    pub const ACTIVE_MODEL_ID: &str = "active_model_id";
    pub const LANGUAGE: &str = "language";
    pub const DENOISE_ENABLED: &str = "denoise_enabled";
    pub const LOG_LEVEL: &str = "log_level";
    pub const HISTORY_RETENTION_COUNT: &str = "history_retention_count";
    pub const HISTORY_RETENTION_DAYS: &str = "history_retention_days";
    pub const HISTORY_ENABLED: &str = "history_enabled";
}

/// All user-facing settings with their current values.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllSettings {
    pub hotkey: String,
    pub output_method: String,
    pub active_model_id: Option<String>,
    pub language: String,
    pub denoise_enabled: bool,
    pub log_level: String,
    pub history_retention_count: i64,
    pub history_retention_days: Option<i64>,
    pub history_enabled: bool,
}

/// Default values for all settings.
fn default_for(key: &str) -> Option<String> {
    let val = match key {
        keys::HOTKEY => {
            if cfg!(target_os = "macos") {
                "\"Cmd+Shift+Space\""
            } else {
                "\"Ctrl+Shift+Space\""
            }
        }
        keys::OUTPUT_METHOD => "\"direct_input\"",
        keys::LANGUAGE => "\"en\"",
        keys::DENOISE_ENABLED => "true",
        keys::LOG_LEVEL => "\"info\"",
        keys::HISTORY_RETENTION_COUNT => "50",
        keys::HISTORY_RETENTION_DAYS => "null",
        keys::HISTORY_ENABLED => "true",
        _ => return None,
    };
    Some(val.to_string())
}

/// Get a setting, falling back to the default if not stored.
pub fn get(conn: &Connection, key: &str) -> Result<String, String> {
    match db::settings::get(conn, key)? {
        Some(val) => Ok(val),
        None => default_for(key).ok_or_else(|| format!("Unknown setting: {key}")),
    }
}

/// Get a typed setting value.
pub fn get_typed<T: serde::de::DeserializeOwned>(
    conn: &Connection,
    key: &str,
) -> Result<T, String> {
    let raw = get(conn, key)?;
    serde_json::from_str(&raw).map_err(|e| format!("Invalid setting value for '{key}': {e}"))
}

/// Set a setting value (JSON-encoded).
pub fn set(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    // Validate that the value is valid JSON
    serde_json::from_str::<serde_json::Value>(value)
        .map_err(|e| format!("Invalid JSON for setting '{key}': {e}"))?;

    db::settings::set(conn, key, value)
}

/// Reset a setting to its default by removing the stored override.
pub fn reset(conn: &Connection, key: &str) -> Result<(), String> {
    db::settings::delete(conn, key)
}

/// Get all settings with their current values (stored + defaults).
pub fn get_all(conn: &Connection) -> Result<AllSettings, String> {
    Ok(AllSettings {
        hotkey: get_typed(conn, keys::HOTKEY)?,
        output_method: get_typed(conn, keys::OUTPUT_METHOD)?,
        active_model_id: get_typed(conn, keys::ACTIVE_MODEL_ID).ok(),
        language: get_typed(conn, keys::LANGUAGE)?,
        denoise_enabled: get_typed(conn, keys::DENOISE_ENABLED)?,
        log_level: get_typed(conn, keys::LOG_LEVEL)?,
        history_retention_count: get_typed(conn, keys::HISTORY_RETENTION_COUNT)?,
        history_retention_days: get_typed(conn, keys::HISTORY_RETENTION_DAYS).ok().flatten(),
        history_enabled: get_typed(conn, keys::HISTORY_ENABLED)?,
    })
}

/// Schema version for export/import.
const SETTINGS_SCHEMA_VERSION: u32 = 1;

/// Export format.
#[derive(Serialize, Deserialize)]
pub struct ExportedSettings {
    pub schema_version: u32,
    pub settings: std::collections::HashMap<String, serde_json::Value>,
}

/// Export all settings to a serializable structure.
pub fn export(conn: &Connection) -> Result<ExportedSettings, String> {
    let pairs = db::settings::get_all(conn)?;
    let mut map = std::collections::HashMap::new();
    for (key, value) in pairs {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&value) {
            map.insert(key, parsed);
        }
    }
    Ok(ExportedSettings {
        schema_version: SETTINGS_SCHEMA_VERSION,
        settings: map,
    })
}

/// Import settings from an exported structure.
pub fn import(conn: &Connection, exported: &ExportedSettings) -> Result<u32, String> {
    if exported.schema_version > SETTINGS_SCHEMA_VERSION {
        return Err(format!(
            "Settings file version {} is newer than supported version {}",
            exported.schema_version, SETTINGS_SCHEMA_VERSION
        ));
    }

    let mut applied = 0u32;

    for (key, value) in &exported.settings {
        // Only import known keys
        if default_for(key).is_some() || key == keys::ACTIVE_MODEL_ID {
            let json = serde_json::to_string(value)
                .map_err(|e| format!("Failed to serialize setting '{key}': {e}"))?;
            db::settings::set(conn, key, &json)?;
            applied += 1;
        } else {
            warn!(key = %key, "Skipping unknown setting during import");
        }
    }

    Ok(applied)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid_json() {
        for key in [
            keys::HOTKEY,
            keys::OUTPUT_METHOD,
            keys::LANGUAGE,
            keys::DENOISE_ENABLED,
            keys::LOG_LEVEL,
            keys::HISTORY_RETENTION_COUNT,
            keys::HISTORY_RETENTION_DAYS,
            keys::HISTORY_ENABLED,
        ] {
            let val = default_for(key).unwrap();
            assert!(
                serde_json::from_str::<serde_json::Value>(&val).is_ok(),
                "Default for '{key}' is not valid JSON: {val}"
            );
        }
    }

    #[test]
    fn get_returns_default() {
        let handle = db::open_memory();
        let conn = handle.blocking_lock();
        let result = get(&conn, keys::LANGUAGE).unwrap();
        assert_eq!(result, "\"en\"");
    }

    #[test]
    fn set_and_get_override() {
        let handle = db::open_memory();
        let conn = handle.blocking_lock();
        set(&conn, keys::LANGUAGE, "\"fr\"").unwrap();
        let result = get(&conn, keys::LANGUAGE).unwrap();
        assert_eq!(result, "\"fr\"");
    }

    #[test]
    fn reset_restores_default() {
        let handle = db::open_memory();
        let conn = handle.blocking_lock();
        set(&conn, keys::LANGUAGE, "\"fr\"").unwrap();
        reset(&conn, keys::LANGUAGE).unwrap();
        let result = get(&conn, keys::LANGUAGE).unwrap();
        assert_eq!(result, "\"en\"");
    }

    #[test]
    fn set_rejects_invalid_json() {
        let handle = db::open_memory();
        let conn = handle.blocking_lock();
        assert!(set(&conn, keys::LANGUAGE, "not json").is_err());
    }

    #[test]
    fn get_all_returns_defaults() {
        let handle = db::open_memory();
        let conn = handle.blocking_lock();
        let all = get_all(&conn).unwrap();
        assert_eq!(all.language, "en");
        assert!(all.denoise_enabled);
        assert_eq!(all.history_retention_count, 50);
    }

    #[test]
    fn export_import_roundtrip() {
        let handle = db::open_memory();
        let conn = handle.blocking_lock();
        set(&conn, keys::LANGUAGE, "\"de\"").unwrap();
        set(&conn, keys::DENOISE_ENABLED, "false").unwrap();

        let exported = export(&conn).unwrap();
        assert_eq!(exported.schema_version, 1);
        assert_eq!(exported.settings.len(), 2);

        // Import into a fresh DB
        let handle2 = db::open_memory();
        let conn2 = handle2.blocking_lock();
        let applied = import(&conn2, &exported).unwrap();
        assert_eq!(applied, 2);

        let lang: String = get_typed(&conn2, keys::LANGUAGE).unwrap();
        assert_eq!(lang, "de");
        let denoise: bool = get_typed(&conn2, keys::DENOISE_ENABLED).unwrap();
        assert!(!denoise);
    }
}
