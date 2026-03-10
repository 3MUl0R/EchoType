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
    pub const OVERLAY_ENABLED: &str = "overlay_enabled";
    pub const AUDIO_FEEDBACK_ENABLED: &str = "audio_feedback_enabled";
    pub const AUDIO_FEEDBACK_VOLUME: &str = "audio_feedback_volume";
    pub const SELECTED_MIC_DEVICE: &str = "selected_mic_device";
    pub const MIC_AUTO_FALLBACK: &str = "mic_auto_fallback";
    pub const ACTIVATION_MODE: &str = "activation_mode";
    pub const NOISE_SUPPRESSION_LEVEL: &str = "noise_suppression_level";
    pub const AUTO_PUNCTUATE: &str = "auto_punctuate";
    pub const DICTATION_MODE: &str = "dictation_mode";
    pub const SILENCE_CUTOFF_SECONDS: &str = "silence_cutoff_seconds";
    pub const TOGGLE_AUTO_STOP_ENABLED: &str = "toggle_auto_stop_enabled";
    pub const AUTO_SUBMIT_ENABLED: &str = "auto_submit_enabled";
    pub const AUTO_SUBMIT_KEY: &str = "auto_submit_key";
    pub const AUTO_SUBMIT_DELAY_MS: &str = "auto_submit_delay_ms";
    pub const STREAMING_ENABLED: &str = "streaming_enabled";
    pub const EDIT_BUFFER_ENABLED: &str = "edit_buffer_enabled";
    pub const CUSTOM_WORDS: &str = "custom_words";
    pub const PRIVATE_MODE_ENABLED: &str = "private_mode_enabled";
    pub const MUTE_SYSTEM_AUDIO: &str = "mute_system_audio";
    pub const ENGINE_TYPE: &str = "engine_type";
    pub const CLOUD_PROVIDER: &str = "cloud_provider";
    pub const CLOUD_OPT_IN_CONFIRMED: &str = "cloud_opt_in_confirmed";
    pub const CLOUD_FALLBACK_LOCAL: &str = "cloud_fallback_local";
    pub const OPENAI_MODEL: &str = "openai_model";
    pub const GROQ_MODEL: &str = "groq_model";
    pub const DEEPGRAM_MODEL: &str = "deepgram_model";
    pub const TYPING_BASELINE_WPM: &str = "typing_baseline_wpm";
    pub const THEME: &str = "theme";
    pub const WIZARD_COMPLETED: &str = "wizard_completed";
    pub const UPDATE_BEHAVIOR: &str = "update_behavior";
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
    pub overlay_enabled: bool,
    pub audio_feedback_enabled: bool,
    pub audio_feedback_volume: f64,
    pub selected_mic_device: Option<String>,
    pub mic_auto_fallback: bool,
    pub activation_mode: String,
    pub noise_suppression_level: String,
    pub auto_punctuate: bool,
    pub dictation_mode: String,
    pub silence_cutoff_seconds: f64,
    pub toggle_auto_stop_enabled: bool,
    pub auto_submit_enabled: bool,
    pub auto_submit_key: String,
    pub auto_submit_delay_ms: u64,
    pub streaming_enabled: bool,
    pub edit_buffer_enabled: bool,
    pub custom_words: Vec<String>,
    pub private_mode_enabled: bool,
    pub mute_system_audio: bool,
    pub engine_type: String,
    pub cloud_provider: Option<String>,
    pub cloud_opt_in_confirmed: bool,
    pub cloud_fallback_local: bool,
    pub openai_model: String,
    pub groq_model: String,
    pub deepgram_model: String,
    pub typing_baseline_wpm: Option<f64>,
    pub theme: String,
    pub wizard_completed: bool,
    pub update_behavior: String,
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
        keys::OVERLAY_ENABLED => "true",
        keys::AUDIO_FEEDBACK_ENABLED => "true",
        keys::AUDIO_FEEDBACK_VOLUME => "0.5",
        keys::MIC_AUTO_FALLBACK => "true",
        keys::ACTIVATION_MODE => "\"hold\"",
        keys::NOISE_SUPPRESSION_LEVEL => "\"moderate\"",
        keys::AUTO_PUNCTUATE => "true",
        keys::DICTATION_MODE => "\"formatted\"",
        keys::SILENCE_CUTOFF_SECONDS => "1.5",
        keys::TOGGLE_AUTO_STOP_ENABLED => "false",
        keys::AUTO_SUBMIT_ENABLED => "false",
        keys::AUTO_SUBMIT_KEY => "\"enter\"",
        keys::AUTO_SUBMIT_DELAY_MS => "100",
        keys::STREAMING_ENABLED => "true",
        keys::EDIT_BUFFER_ENABLED => "false",
        keys::CUSTOM_WORDS => "[]",
        keys::PRIVATE_MODE_ENABLED => "false",
        keys::MUTE_SYSTEM_AUDIO => "false",
        keys::ENGINE_TYPE => "\"local\"",
        keys::CLOUD_PROVIDER => "null",
        keys::CLOUD_OPT_IN_CONFIRMED => "false",
        keys::CLOUD_FALLBACK_LOCAL => "true",
        keys::OPENAI_MODEL => "\"whisper-1\"",
        keys::GROQ_MODEL => "\"whisper-large-v3\"",
        keys::DEEPGRAM_MODEL => "\"nova-2\"",
        keys::TYPING_BASELINE_WPM => "null",
        keys::THEME => "\"dark\"",
        keys::WIZARD_COMPLETED => "false",
        keys::UPDATE_BEHAVIOR => "\"download_and_prompt\"",
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

/// Get a typed setting with profile override: profile → global → default.
pub fn get_typed_with_profile<T: serde::de::DeserializeOwned>(
    conn: &Connection,
    key: &str,
    profile_id: Option<i64>,
) -> Result<T, String> {
    // Check profile override first
    if let Some(pid) = profile_id {
        if let Ok(Some(val)) = crate::db::profiles::get_setting(conn, pid, key) {
            return serde_json::from_str(&val)
                .map_err(|e| format!("Invalid profile setting for '{key}': {e}"));
        }
    }
    // Fall back to global
    get_typed(conn, key)
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
        overlay_enabled: get_typed(conn, keys::OVERLAY_ENABLED)?,
        audio_feedback_enabled: get_typed(conn, keys::AUDIO_FEEDBACK_ENABLED)?,
        audio_feedback_volume: get_typed(conn, keys::AUDIO_FEEDBACK_VOLUME)?,
        selected_mic_device: get_typed(conn, keys::SELECTED_MIC_DEVICE).ok(),
        mic_auto_fallback: get_typed(conn, keys::MIC_AUTO_FALLBACK)?,
        activation_mode: get_typed(conn, keys::ACTIVATION_MODE)?,
        noise_suppression_level: get_typed(conn, keys::NOISE_SUPPRESSION_LEVEL)?,
        auto_punctuate: get_typed(conn, keys::AUTO_PUNCTUATE)?,
        dictation_mode: get_typed(conn, keys::DICTATION_MODE)?,
        silence_cutoff_seconds: get_typed(conn, keys::SILENCE_CUTOFF_SECONDS)?,
        toggle_auto_stop_enabled: get_typed(conn, keys::TOGGLE_AUTO_STOP_ENABLED)?,
        auto_submit_enabled: get_typed(conn, keys::AUTO_SUBMIT_ENABLED)?,
        auto_submit_key: get_typed(conn, keys::AUTO_SUBMIT_KEY)?,
        auto_submit_delay_ms: get_typed(conn, keys::AUTO_SUBMIT_DELAY_MS)?,
        streaming_enabled: get_typed(conn, keys::STREAMING_ENABLED)?,
        edit_buffer_enabled: get_typed(conn, keys::EDIT_BUFFER_ENABLED)?,
        custom_words: get_typed(conn, keys::CUSTOM_WORDS).unwrap_or_default(),
        private_mode_enabled: get_typed(conn, keys::PRIVATE_MODE_ENABLED)?,
        mute_system_audio: get_typed(conn, keys::MUTE_SYSTEM_AUDIO)?,
        engine_type: get_typed(conn, keys::ENGINE_TYPE)?,
        cloud_provider: get_typed(conn, keys::CLOUD_PROVIDER).ok().flatten(),
        cloud_opt_in_confirmed: get_typed(conn, keys::CLOUD_OPT_IN_CONFIRMED)?,
        cloud_fallback_local: get_typed(conn, keys::CLOUD_FALLBACK_LOCAL)?,
        openai_model: get_typed(conn, keys::OPENAI_MODEL)?,
        groq_model: get_typed(conn, keys::GROQ_MODEL)?,
        deepgram_model: get_typed(conn, keys::DEEPGRAM_MODEL)?,
        typing_baseline_wpm: get_typed(conn, keys::TYPING_BASELINE_WPM).ok().flatten(),
        theme: get_typed(conn, keys::THEME)?,
        wizard_completed: get_typed(conn, keys::WIZARD_COMPLETED)?,
        update_behavior: get_typed(conn, keys::UPDATE_BEHAVIOR)?,
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
        if default_for(key).is_some()
            || key == keys::ACTIVE_MODEL_ID
            || key == keys::SELECTED_MIC_DEVICE
        {
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
            keys::OVERLAY_ENABLED,
            keys::AUDIO_FEEDBACK_ENABLED,
            keys::AUDIO_FEEDBACK_VOLUME,
            keys::MIC_AUTO_FALLBACK,
            keys::ACTIVATION_MODE,
            keys::NOISE_SUPPRESSION_LEVEL,
            keys::AUTO_PUNCTUATE,
            keys::DICTATION_MODE,
            keys::SILENCE_CUTOFF_SECONDS,
            keys::TOGGLE_AUTO_STOP_ENABLED,
            keys::AUTO_SUBMIT_ENABLED,
            keys::AUTO_SUBMIT_KEY,
            keys::AUTO_SUBMIT_DELAY_MS,
            keys::STREAMING_ENABLED,
            keys::EDIT_BUFFER_ENABLED,
            keys::CUSTOM_WORDS,
            keys::PRIVATE_MODE_ENABLED,
            keys::MUTE_SYSTEM_AUDIO,
            keys::ENGINE_TYPE,
            keys::CLOUD_PROVIDER,
            keys::CLOUD_OPT_IN_CONFIRMED,
            keys::CLOUD_FALLBACK_LOCAL,
            keys::OPENAI_MODEL,
            keys::GROQ_MODEL,
            keys::DEEPGRAM_MODEL,
            keys::TYPING_BASELINE_WPM,
            keys::THEME,
            keys::WIZARD_COMPLETED,
            keys::UPDATE_BEHAVIOR,
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
        assert!(all.overlay_enabled);
        assert!(all.audio_feedback_enabled);
        assert!((all.audio_feedback_volume - 0.5).abs() < f64::EPSILON);
        assert!(all.selected_mic_device.is_none());
        assert!(all.mic_auto_fallback);
        assert_eq!(all.activation_mode, "hold");
        assert_eq!(all.noise_suppression_level, "moderate");
        assert!(all.auto_punctuate);
        assert_eq!(all.dictation_mode, "formatted");
        assert!((all.silence_cutoff_seconds - 1.5).abs() < f64::EPSILON);
        assert!(!all.toggle_auto_stop_enabled);
        assert!(!all.auto_submit_enabled);
        assert_eq!(all.auto_submit_key, "enter");
        assert_eq!(all.auto_submit_delay_ms, 100);
        assert!(all.streaming_enabled);
        assert!(!all.edit_buffer_enabled);
        assert!(all.custom_words.is_empty());
        assert!(!all.private_mode_enabled);
        assert!(!all.mute_system_audio);
        assert_eq!(all.engine_type, "local");
        assert!(all.cloud_provider.is_none());
        assert!(!all.cloud_opt_in_confirmed);
        assert!(all.cloud_fallback_local);
        assert_eq!(all.openai_model, "whisper-1");
        assert_eq!(all.groq_model, "whisper-large-v3");
        assert_eq!(all.deepgram_model, "nova-2");
        assert!(all.typing_baseline_wpm.is_none());
        assert_eq!(all.theme, "dark");
        assert!(!all.wizard_completed);
        assert_eq!(all.update_behavior, "download_and_prompt");
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
