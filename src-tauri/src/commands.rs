use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tracing::{error, info};

use crate::audio::{capture, pipeline, AudioDeviceInfo};
use crate::engine::whisper::WhisperEngine;
use crate::engine::{Language, SttEngine, TranscribeRequest, Transcription};
use crate::models::download::DownloadManager;
use crate::models::manifest::ModelEntry;
use crate::AppState;

#[tauri::command]
pub fn list_audio_devices() -> Vec<AudioDeviceInfo> {
    capture::list_devices()
}

#[tauri::command]
pub async fn start_capture(state: State<'_, AppState>) -> Result<(), String> {
    // Check lock first before starting hardware capture
    let mut guard = state.capture_session.lock().await;
    if guard.is_some() {
        return Err("Capture already in progress".to_string());
    }

    // Read mic settings
    let (device, fallback) = {
        let conn = state.db.lock().await;
        let device =
            crate::settings::get_typed::<String>(&conn, crate::settings::keys::SELECTED_MIC_DEVICE)
                .ok();
        let fallback =
            crate::settings::get_typed::<bool>(&conn, crate::settings::keys::MIC_AUTO_FALLBACK)
                .unwrap_or(true);
        (device, fallback)
    };

    let session = capture::start_capture_with_device(device.as_deref(), fallback)
        .map_err(|e| e.to_string())?;
    *guard = Some(session);
    Ok(())
}

#[tauri::command]
pub async fn stop_capture(state: State<'_, AppState>) -> Result<f64, String> {
    let session = {
        let mut guard = state.capture_session.lock().await;
        guard.take().ok_or("No active capture session")?
    };
    let buffer = session.stop();
    let duration = buffer.duration_secs();
    *state.last_audio.lock().await = Some(buffer);
    Ok(duration)
}

#[tauri::command]
pub async fn transcribe_audio(
    state: State<'_, AppState>,
    language: Option<String>,
) -> Result<Transcription, String> {
    let raw = {
        let audio_guard = state.last_audio.lock().await;
        audio_guard
            .as_ref()
            .ok_or("No audio to transcribe. Record something first.")?
            .clone()
    };

    if raw.is_empty() {
        return Err("Audio buffer is empty".to_string());
    }

    info!(
        duration_secs = raw.duration_secs(),
        sample_rate = raw.sample_rate,
        "Processing audio for transcription"
    );

    // Read noise suppression setting
    let suppression_level = {
        let conn = state.db.lock().await;
        let level_str = crate::settings::get_typed::<String>(
            &conn,
            crate::settings::keys::NOISE_SUPPRESSION_LEVEL,
        )
        .unwrap_or_else(|_| "moderate".to_string());
        crate::audio::denoise::SuppressionLevel::from_str(&level_str)
    };

    // Run audio pipeline (denoise + resample) off the async runtime
    let processed = tokio::task::spawn_blocking(move || {
        let config = pipeline::PipelineConfig { suppression_level };
        pipeline::process(&raw, &config)
    })
    .await
    .map_err(|e| format!("Pipeline task failed: {e}"))?
    .map_err(|e| format!("Audio pipeline error: {e}"))?;

    // Transcribe
    let request = TranscribeRequest {
        audio: processed.samples,
        sample_rate: processed.sample_rate,
        language: language.map(Language),
    };

    state.engine_manager.transcribe(request).await.map_err(|e| {
        error!(%e, "Transcription failed");
        e.to_string()
    })
}

#[tauri::command]
pub async fn load_model(state: State<'_, AppState>, path: String) -> Result<String, String> {
    // Model loading is CPU-intensive (parsing GGML file), run off async runtime
    let engine = tokio::task::spawn_blocking(move || {
        let model_path = std::path::PathBuf::from(&path);
        WhisperEngine::new(&model_path)
    })
    .await
    .map_err(|e| format!("Model load task failed: {e}"))?
    .map_err(|e| e.to_string())?;

    let name = engine.name().to_string();
    state.engine_manager.load(Box::new(engine)).await;
    Ok(name)
}

#[tauri::command]
pub async fn get_dictation_state(
    state: State<'_, AppState>,
) -> Result<crate::dictation::DictationState, String> {
    Ok(state.dictation_manager.current_state().await)
}

// --- Model Management Commands ---

/// A model entry with its install/download status for the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    #[serde(flatten)]
    pub entry: ModelEntry,
    pub installed: bool,
    pub downloading: bool,
    pub active: bool,
}

#[tauri::command]
pub async fn list_available_models(
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Vec<ModelInfo>, String> {
    let manifest = state.manifest.lock().await;
    let active_id = state.active_model_id.lock().await.clone();
    let models_dir = DownloadManager::models_dir(&app)?;

    let mut result = Vec::new();
    for entry in &manifest.models {
        let installed = models_dir.join(&entry.file).exists();
        let downloading = state.download_manager.is_downloading(&entry.id).await;
        let active = active_id.as_deref() == Some(&entry.id);

        result.push(ModelInfo {
            entry: entry.clone(),
            installed,
            downloading,
            active,
        });
    }

    Ok(result)
}

#[tauri::command]
pub async fn list_installed_models(
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Vec<ModelInfo>, String> {
    let manifest = state.manifest.lock().await;
    let active_id = state.active_model_id.lock().await.clone();
    let models_dir = DownloadManager::models_dir(&app)?;

    let mut result = Vec::new();
    for entry in &manifest.models {
        let path = models_dir.join(&entry.file);
        if path.exists() {
            let active = active_id.as_deref() == Some(&entry.id);
            result.push(ModelInfo {
                entry: entry.clone(),
                installed: true,
                downloading: false,
                active,
            });
        }
    }

    Ok(result)
}

#[tauri::command]
pub async fn download_model(
    state: State<'_, AppState>,
    app: AppHandle,
    model_id: String,
) -> Result<(), String> {
    let manifest = state.manifest.lock().await;
    let entry = manifest
        .models
        .iter()
        .find(|m| m.id == model_id)
        .ok_or_else(|| format!("Model not found: {model_id}"))?
        .clone();
    drop(manifest);

    state.download_manager.start_download(&app, entry).await
}

#[tauri::command]
pub async fn cancel_download(state: State<'_, AppState>, model_id: String) -> Result<(), String> {
    state.download_manager.cancel_download(&model_id).await
}

#[tauri::command]
pub async fn delete_model(
    state: State<'_, AppState>,
    app: AppHandle,
    model_id: String,
) -> Result<(), String> {
    // Hold the active_model_id lock across the entire operation to prevent
    // a race where set_active_model activates this model between our check and delete.
    let active_id = state.active_model_id.lock().await;
    if active_id.as_deref() == Some(model_id.as_str()) {
        return Err("Cannot delete the active model. Switch to another model first.".to_string());
    }

    let manifest = state.manifest.lock().await;
    let entry = manifest
        .models
        .iter()
        .find(|m| m.id == model_id)
        .ok_or_else(|| format!("Model not found: {model_id}"))?
        .clone();
    drop(manifest);

    let result = DownloadManager::delete_model(&app, &entry).await;
    drop(active_id);
    result
}

#[tauri::command]
pub async fn set_active_model(
    state: State<'_, AppState>,
    app: AppHandle,
    model_id: String,
) -> Result<String, String> {
    // Check if dictation is active
    let dictation_state = state.dictation_manager.current_state().await;
    if dictation_state != crate::dictation::DictationState::Idle {
        return Err("Cannot switch models during active dictation".to_string());
    }

    // Find the model in manifest
    let manifest = state.manifest.lock().await;
    let entry = manifest
        .models
        .iter()
        .find(|m| m.id == model_id)
        .ok_or_else(|| format!("Model not found: {model_id}"))?
        .clone();
    drop(manifest);

    // Verify model file exists
    let models_dir = DownloadManager::models_dir(&app)?;
    let model_path = models_dir.join(&entry.file);
    if !model_path.exists() {
        return Err(format!(
            "Model file not downloaded: {}. Download it first.",
            entry.file
        ));
    }

    // Load the model (CPU-intensive)
    let path = model_path.clone();
    let engine = tokio::task::spawn_blocking(move || WhisperEngine::new(&path))
        .await
        .map_err(|e| format!("Model load task failed: {e}"))?
        .map_err(|e| e.to_string())?;

    let name = engine.name().to_string();
    state.engine_manager.load(Box::new(engine)).await;

    // Update active model ID (in memory and database)
    *state.active_model_id.lock().await = Some(model_id.clone());
    {
        let conn = state.db.lock().await;
        crate::settings::set(
            &conn,
            crate::settings::keys::ACTIVE_MODEL_ID,
            &serde_json::to_string(&model_id).unwrap(),
        )
        .ok();
    }

    info!(model = %name, "Active model switched");
    Ok(name)
}

#[tauri::command]
pub async fn get_active_model(state: State<'_, AppState>) -> Result<Option<String>, String> {
    Ok(state.active_model_id.lock().await.clone())
}

// --- Edit Buffer Commands ---

#[tauri::command]
pub async fn get_edit_buffer_text(state: State<'_, AppState>) -> Result<String, String> {
    let pending = state.pending_edit.lock().await;
    match pending.as_ref() {
        Some(edit) => Ok(edit.text.clone()),
        None => Err("No pending edit".to_string()),
    }
}

#[tauri::command]
pub async fn edit_buffer_insert(app: AppHandle, text: String) -> Result<(), String> {
    crate::dictation::complete_edit_insert(&app, text).await;
    Ok(())
}

#[tauri::command]
pub async fn edit_buffer_discard(app: AppHandle) -> Result<(), String> {
    crate::dictation::complete_edit_discard(&app).await;
    Ok(())
}

#[tauri::command]
pub async fn edit_buffer_copy(text: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        use arboard::Clipboard;
        let mut clipboard = Clipboard::new().map_err(|e| format!("Clipboard error: {e}"))?;
        clipboard
            .set_text(&text)
            .map_err(|e| format!("Failed to copy: {e}"))?;
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| format!("Task failed: {e}"))??;
    Ok(())
}

// --- Hotkey Commands ---

#[tauri::command]
pub async fn change_hotkey(
    app: AppHandle,
    state: State<'_, AppState>,
    shortcut: String,
) -> Result<(), String> {
    let shortcut = shortcut.trim().to_string();
    if shortcut.is_empty() {
        return Err("Shortcut cannot be empty".to_string());
    }

    // Unregister the old hotkey
    crate::hotkey::unregister_dictation_hotkey(&app)?;

    // Try to register the new one
    if let Err(e) = crate::hotkey::register_dictation_hotkey(&app, Some(&shortcut)) {
        // Registration failed — try to restore the old hotkey
        let old_hotkey = {
            let conn = state.db.lock().await;
            crate::settings::get_typed::<String>(&conn, crate::settings::keys::HOTKEY).ok()
        };
        let _ = crate::hotkey::register_dictation_hotkey(&app, old_hotkey.as_deref());
        return Err(format!("Invalid shortcut: {e}"));
    }

    // Persist the new hotkey
    {
        let conn = state.db.lock().await;
        let json = serde_json::to_string(&shortcut)
            .map_err(|e| format!("Failed to serialize hotkey: {e}"))?;
        crate::settings::set(&conn, crate::settings::keys::HOTKEY, &json)?;
    }

    info!(shortcut = %shortcut, "Hotkey changed");
    Ok(())
}

// --- Settings Commands ---

#[tauri::command]
pub async fn get_setting(state: State<'_, AppState>, key: String) -> Result<String, String> {
    let conn = state.db.lock().await;
    crate::settings::get(&conn, &key)
}

#[tauri::command]
pub async fn set_setting(
    state: State<'_, AppState>,
    key: String,
    value: String,
) -> Result<(), String> {
    let conn = state.db.lock().await;
    crate::settings::set(&conn, &key, &value)
}

#[tauri::command]
pub async fn get_all_settings(
    state: State<'_, AppState>,
) -> Result<crate::settings::AllSettings, String> {
    let conn = state.db.lock().await;
    crate::settings::get_all(&conn)
}

#[tauri::command]
pub async fn reset_setting(state: State<'_, AppState>, key: String) -> Result<(), String> {
    let conn = state.db.lock().await;
    crate::settings::reset(&conn, &key)
}

#[tauri::command]
pub async fn export_settings(
    state: State<'_, AppState>,
) -> Result<crate::settings::ExportedSettings, String> {
    let conn = state.db.lock().await;
    crate::settings::export(&conn)
}

#[tauri::command]
pub async fn import_settings(
    state: State<'_, AppState>,
    data: crate::settings::ExportedSettings,
) -> Result<u32, String> {
    let conn = state.db.lock().await;
    crate::settings::import(&conn, &data)
}

// --- Permission Commands ---

#[tauri::command]
pub fn check_permissions() -> crate::platform::permissions::PermissionStatus {
    crate::platform::permissions::check_permissions()
}

#[tauri::command]
pub fn open_permission_settings(permission: String) -> Result<(), String> {
    crate::platform::permissions::open_permission_settings(&permission)
}

// --- History Commands ---

#[tauri::command]
pub async fn get_history(
    state: State<'_, AppState>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<crate::db::history::HistoryEntry>, String> {
    let conn = state.db.lock().await;
    crate::db::history::list(&conn, limit.unwrap_or(20), offset.unwrap_or(0))
}

#[tauri::command]
pub async fn get_history_entry(
    state: State<'_, AppState>,
    id: i64,
) -> Result<Option<crate::db::history::HistoryEntry>, String> {
    let conn = state.db.lock().await;
    crate::db::history::get_by_id(&conn, id)
}

#[tauri::command]
pub async fn delete_history_entry(
    state: State<'_, AppState>,
    app: AppHandle,
    id: i64,
) -> Result<(), String> {
    let audio_path = {
        let conn = state.db.lock().await;
        crate::db::history::delete(&conn, id)?
    };

    // Clean up audio file if it existed
    if let Some(rel_path) = audio_path {
        let data_dir = app
            .path()
            .app_data_dir()
            .map_err(|e| format!("Cannot resolve data dir: {e}"))?;
        let full_path = data_dir.join(&rel_path);
        if full_path.exists() {
            let _ = tokio::fs::remove_file(&full_path).await;
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn clear_history(state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    let audio_paths = {
        let conn = state.db.lock().await;
        crate::db::history::clear(&conn)?
    };

    // Clean up audio files
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Cannot resolve data dir: {e}"))?;
    for rel_path in audio_paths {
        let full_path = data_dir.join(&rel_path);
        if full_path.exists() {
            let _ = tokio::fs::remove_file(&full_path).await;
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn copy_history_text(state: State<'_, AppState>, id: i64) -> Result<String, String> {
    let conn = state.db.lock().await;
    let entry = crate::db::history::get_by_id(&conn, id)?.ok_or("History entry not found")?;

    // Copy to clipboard
    let text = entry.text.clone();
    tokio::task::spawn_blocking(move || {
        use arboard::Clipboard;
        let mut clipboard = Clipboard::new().map_err(|e| format!("Clipboard error: {e}"))?;
        clipboard
            .set_text(&text)
            .map_err(|e| format!("Failed to copy: {e}"))?;
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| format!("Task failed: {e}"))??;

    Ok(entry.text)
}

// --- Profile Commands ---

#[tauri::command]
pub async fn list_profiles(
    state: State<'_, AppState>,
) -> Result<Vec<crate::db::profiles::Profile>, String> {
    let conn = state.db.lock().await;
    crate::db::profiles::list(&conn)
}

#[tauri::command]
pub async fn get_profile(
    state: State<'_, AppState>,
    id: i64,
) -> Result<Option<crate::db::profiles::Profile>, String> {
    let conn = state.db.lock().await;
    crate::db::profiles::get_by_id(&conn, id)
}

#[tauri::command]
pub async fn create_profile(
    state: State<'_, AppState>,
    name: String,
    app_identifier: String,
    app_identifier_type: String,
) -> Result<i64, String> {
    let id_type = crate::db::profiles::AppIdentifierType::from_str(&app_identifier_type);
    let conn = state.db.lock().await;
    crate::db::profiles::create(
        &conn,
        &crate::db::profiles::CreateProfileParams {
            name: &name,
            app_identifier: &app_identifier,
            app_identifier_type: &id_type,
        },
    )
}

#[tauri::command]
pub async fn update_profile(
    state: State<'_, AppState>,
    id: i64,
    name: String,
    app_identifier: String,
    app_identifier_type: String,
) -> Result<(), String> {
    let id_type = crate::db::profiles::AppIdentifierType::from_str(&app_identifier_type);
    let conn = state.db.lock().await;
    crate::db::profiles::update(&conn, id, &name, &app_identifier, &id_type)
}

#[tauri::command]
pub async fn delete_profile(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let conn = state.db.lock().await;
    crate::db::profiles::delete(&conn, id)
}

#[tauri::command]
pub async fn get_profile_settings(
    state: State<'_, AppState>,
    profile_id: i64,
) -> Result<Vec<(String, String)>, String> {
    let conn = state.db.lock().await;
    crate::db::profiles::get_all_settings(&conn, profile_id)
}

#[tauri::command]
pub async fn set_profile_setting(
    state: State<'_, AppState>,
    profile_id: i64,
    key: String,
    value: String,
) -> Result<(), String> {
    let conn = state.db.lock().await;
    crate::db::profiles::set_setting(&conn, profile_id, &key, &value)
}

#[tauri::command]
pub async fn remove_profile_setting(
    state: State<'_, AppState>,
    profile_id: i64,
    key: String,
) -> Result<(), String> {
    let conn = state.db.lock().await;
    crate::db::profiles::remove_setting(&conn, profile_id, &key)
}

// --- Vocabulary Commands ---

#[tauri::command]
pub async fn list_vocabulary_collections(
    state: State<'_, AppState>,
) -> Result<Vec<crate::db::vocabulary::VocabularyCollection>, String> {
    let conn = state.db.lock().await;
    crate::db::vocabulary::list_collections(&conn)
}

#[tauri::command]
pub async fn create_vocabulary_collection(
    state: State<'_, AppState>,
    name: String,
) -> Result<i64, String> {
    let conn = state.db.lock().await;
    crate::db::vocabulary::create_collection(&conn, &name)
}

#[tauri::command]
pub async fn rename_vocabulary_collection(
    state: State<'_, AppState>,
    id: i64,
    name: String,
) -> Result<(), String> {
    let conn = state.db.lock().await;
    crate::db::vocabulary::rename_collection(&conn, id, &name)
}

#[tauri::command]
pub async fn delete_vocabulary_collection(
    state: State<'_, AppState>,
    id: i64,
) -> Result<(), String> {
    let conn = state.db.lock().await;
    crate::db::vocabulary::delete_collection(&conn, id)
}

#[tauri::command]
pub async fn list_vocabulary_entries(
    state: State<'_, AppState>,
    collection_id: i64,
) -> Result<Vec<crate::db::vocabulary::VocabularyEntry>, String> {
    let conn = state.db.lock().await;
    crate::db::vocabulary::list_entries(&conn, collection_id)
}

#[tauri::command]
pub async fn add_vocabulary_entry(
    state: State<'_, AppState>,
    collection_id: i64,
    correction: String,
    aliases: Vec<String>,
) -> Result<i64, String> {
    let conn = state.db.lock().await;
    crate::db::vocabulary::add_entry(&conn, collection_id, &correction, &aliases)
}

#[tauri::command]
pub async fn update_vocabulary_entry(
    state: State<'_, AppState>,
    id: i64,
    correction: String,
    aliases: Vec<String>,
) -> Result<(), String> {
    let conn = state.db.lock().await;
    crate::db::vocabulary::update_entry(&conn, id, &correction, &aliases)
}

#[tauri::command]
pub async fn delete_vocabulary_entry(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let conn = state.db.lock().await;
    crate::db::vocabulary::delete_entry(&conn, id)
}

#[tauri::command]
pub async fn vocabulary_entry_count(
    state: State<'_, AppState>,
    collection_id: i64,
) -> Result<i64, String> {
    let conn = state.db.lock().await;
    crate::db::vocabulary::entry_count(&conn, collection_id)
}

#[tauri::command]
pub async fn import_vocabulary_json(
    state: State<'_, AppState>,
    collection_id: i64,
    entries: Vec<crate::db::vocabulary::VocabularyEntry>,
) -> Result<u32, String> {
    let conn = state.db.lock().await;
    let mut count = 0u32;
    for entry in &entries {
        crate::db::vocabulary::add_entry(&conn, collection_id, &entry.correction, &entry.aliases)?;
        count += 1;
    }
    Ok(count)
}

// --- Private Mode Commands ---

#[tauri::command]
pub async fn toggle_private_mode(state: State<'_, AppState>) -> Result<bool, String> {
    let conn = state.db.lock().await;
    let current =
        crate::settings::get_typed::<bool>(&conn, crate::settings::keys::PRIVATE_MODE_ENABLED)
            .unwrap_or(false);
    let new_value = !current;
    crate::settings::set(
        &conn,
        crate::settings::keys::PRIVATE_MODE_ENABLED,
        &serde_json::to_string(&new_value).unwrap(),
    )?;
    info!(private_mode = new_value, "Private mode toggled");
    Ok(new_value)
}

// ── Cloud API Key Commands ──────────────────────────────────────────

#[tauri::command]
pub async fn set_api_key(
    provider: crate::engine::cloud::CloudProvider,
    key: String,
) -> Result<(), String> {
    let key = key.trim().to_string();
    if key.is_empty() {
        return Err("API key cannot be empty".to_string());
    }
    if key.len() > 256 {
        return Err("API key is too long".to_string());
    }
    crate::security::keyring_store::set_api_key(provider, &key)
}

#[tauri::command]
pub async fn get_api_key_status(
    provider: crate::engine::cloud::CloudProvider,
) -> Result<crate::security::keyring_store::ApiKeyStatus, String> {
    crate::security::keyring_store::get_api_key_status(provider)
}

#[tauri::command]
pub async fn delete_api_key(provider: crate::engine::cloud::CloudProvider) -> Result<(), String> {
    crate::security::keyring_store::delete_api_key(provider)
}

#[tauri::command]
pub async fn validate_api_key(
    provider: crate::engine::cloud::CloudProvider,
) -> Result<bool, String> {
    crate::security::keyring_store::validate_api_key(provider).await
}

#[tauri::command]
pub async fn list_cloud_providers() -> Result<Vec<serde_json::Value>, String> {
    use crate::engine::cloud::CloudProvider;
    use crate::security::keyring_store;

    let providers = [
        CloudProvider::Groq,
        CloudProvider::OpenAi,
        CloudProvider::Deepgram,
    ];
    let mut result = Vec::new();

    for provider in providers {
        let status = keyring_store::get_api_key_status(provider)?;
        result.push(serde_json::json!({
            "id": provider.as_str(),
            "name": provider.display_name(),
            "has_key": status.has_key,
            "masked_last4": status.masked_last4,
        }));
    }

    Ok(result)
}

/// Switch to a cloud engine for transcription.
#[tauri::command]
pub async fn activate_cloud_engine(
    state: State<'_, AppState>,
    provider: crate::engine::cloud::CloudProvider,
) -> Result<(), String> {
    use crate::engine::cloud::{deepgram::DeepgramEngine, groq::GroqEngine, openai::OpenAiEngine};
    use crate::security::keyring_store;

    // Enforce cloud opt-in
    {
        let conn = state.db.lock().await;
        let opted_in = crate::settings::get_typed::<bool>(
            &conn,
            crate::settings::keys::CLOUD_OPT_IN_CONFIRMED,
        )
        .unwrap_or(false);
        if !opted_in {
            return Err("Cloud usage not confirmed by user".to_string());
        }
    }

    // Check dictation is idle before switching
    let dictation_state = state.dictation_manager.current_state().await;
    if dictation_state != crate::dictation::DictationState::Idle {
        return Err("Cannot switch engines during active dictation".to_string());
    }

    let api_key = keyring_store::get_api_key(provider)?
        .ok_or_else(|| format!("No API key configured for {provider}"))?;

    let engine: Box<dyn SttEngine> = match provider {
        crate::engine::cloud::CloudProvider::Groq => {
            Box::new(GroqEngine::new(api_key).map_err(|e| e.to_string())?)
        }
        crate::engine::cloud::CloudProvider::OpenAi => {
            let conn = state.db.lock().await;
            let model =
                crate::settings::get_typed::<String>(&conn, crate::settings::keys::OPENAI_MODEL)
                    .unwrap_or_else(|_| "whisper-1".to_string());
            drop(conn);
            Box::new(
                OpenAiEngine::new(api_key)
                    .map_err(|e| e.to_string())?
                    .with_model(model),
            )
        }
        crate::engine::cloud::CloudProvider::Deepgram => {
            Box::new(DeepgramEngine::new(api_key).map_err(|e| e.to_string())?)
        }
    };

    state.engine_manager.load(engine).await;

    // Save the engine type and provider in settings
    {
        let conn = state.db.lock().await;
        crate::settings::set(&conn, crate::settings::keys::ENGINE_TYPE, "\"cloud\"")?;
        let provider_json = serde_json::to_string(&provider.as_str())
            .map_err(|e| format!("JSON serialize: {e}"))?;
        crate::settings::set(&conn, crate::settings::keys::CLOUD_PROVIDER, &provider_json)?;
    }

    info!(provider = provider.as_str(), "Cloud engine activated");
    Ok(())
}

/// Switch back to a local model engine.
#[tauri::command]
pub async fn activate_local_engine(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Check dictation is idle before switching
    let dictation_state = state.dictation_manager.current_state().await;
    if dictation_state != crate::dictation::DictationState::Idle {
        return Err("Cannot switch engines during active dictation".to_string());
    }

    let model_id = state.active_model_id.lock().await.clone();
    let model_id = model_id.ok_or("No local model selected")?;

    let file = {
        let manifest = state.manifest.lock().await;
        manifest
            .models
            .iter()
            .find(|m| m.id == model_id)
            .map(|e| e.file.clone())
            .ok_or_else(|| format!("Model {model_id} not in manifest"))?
    };

    let models_dir = DownloadManager::models_dir(&app)?;
    let model_path = models_dir.join(&file);
    if !model_path.exists() {
        return Err(format!("Model file not found: {}", model_path.display()));
    }

    let engine = tokio::task::spawn_blocking(move || WhisperEngine::new(&model_path))
        .await
        .map_err(|e| format!("Task error: {e}"))?
        .map_err(|e| e.to_string())?;

    state.engine_manager.load(Box::new(engine)).await;

    // Save engine type in settings
    {
        let conn = state.db.lock().await;
        crate::settings::set(&conn, crate::settings::keys::ENGINE_TYPE, "\"local\"")?;
    }

    info!(model = %model_id, "Local engine activated");
    Ok(())
}

// ── Metrics Commands ──────────────────────────────────────────────

#[tauri::command]
pub async fn get_metrics_today(
    state: State<'_, AppState>,
) -> Result<Option<crate::db::metrics::DailyMetrics>, String> {
    let date_local = chrono::Local::now().format("%Y-%m-%d").to_string();
    let conn = state.db.lock().await;
    crate::db::metrics::get_daily(&conn, &date_local)
}

#[tauri::command]
pub async fn get_metrics_range(
    state: State<'_, AppState>,
    from: String,
    to: String,
) -> Result<Vec<crate::db::metrics::DailyMetrics>, String> {
    let conn = state.db.lock().await;
    crate::db::metrics::get_daily_range(&conn, &from, &to)
}

#[tauri::command]
pub async fn get_engine_breakdown(
    state: State<'_, AppState>,
    from: String,
    to: String,
) -> Result<Vec<crate::db::metrics::DailyEngineMetrics>, String> {
    let conn = state.db.lock().await;
    crate::db::metrics::get_engine_breakdown(&conn, &from, &to)
}

#[tauri::command]
pub async fn get_lifetime_metrics(
    state: State<'_, AppState>,
) -> Result<crate::db::metrics::LifetimeMetrics, String> {
    let conn = state.db.lock().await;
    crate::db::metrics::get_lifetime(&conn)
}

#[tauri::command]
pub async fn set_typing_baseline(state: State<'_, AppState>, wpm: f64) -> Result<(), String> {
    if wpm.is_nan() || wpm <= 0.0 || wpm > 500.0 {
        return Err("Typing baseline must be between 1 and 500 WPM".to_string());
    }
    let conn = state.db.lock().await;
    crate::db::metrics::set_typing_baseline(&conn, wpm)
}

// ── Updater Commands ──────────────────────────────────────────────

#[tauri::command]
pub async fn check_for_update() -> Result<Option<crate::updater::UpdateInfo>, String> {
    Ok(crate::updater::check_for_update().await)
}

#[tauri::command]
pub async fn get_build_info() -> Result<serde_json::Value, String> {
    let meta = crate::updater::build_meta();
    Ok(serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "platform": meta.platform,
        "arch": meta.arch,
        "flavor": meta.flavor,
    }))
}

// ── Diagnostic Commands ─────────────────────────────────────────

#[tauri::command]
pub async fn get_diagnostic_info(state: State<'_, AppState>) -> Result<String, String> {
    let mut info = crate::diagnostic::collect_basic();

    // Fill in app-state-dependent fields
    let model_id = state.active_model_id.lock().await.clone();
    info.active_model = model_id;

    let engine_type = {
        let conn = state.db.lock().await;
        crate::settings::get_typed::<String>(&conn, crate::settings::keys::ENGINE_TYPE)
            .unwrap_or_else(|_| "local".to_string())
    };
    info.engine_type = engine_type;

    Ok(crate::diagnostic::format_markdown(&info))
}
