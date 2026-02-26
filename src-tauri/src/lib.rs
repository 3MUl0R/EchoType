mod audio;
pub mod cli;
mod commands;
mod db;
pub mod diagnostic;
mod dictation;
mod engine;
mod hotkey;
mod logging;
mod models;
mod output;
mod platform;
mod security;
mod settings;
mod tray;
#[allow(dead_code)]
mod updater;

use std::sync::Arc;

use tauri::{Listener, Manager};
use tokio::sync::Mutex;
use tracing::{debug, error, info};

use dictation::DictationManager;
use engine::manager::EngineManager;
use engine::SttEngine;
use models::download::DownloadManager;
use models::manifest::Manifest;

/// Shared application state accessible from Tauri commands.
pub struct AppState {
    pub engine_manager: EngineManager,
    pub capture_session: Arc<Mutex<Option<audio::capture::CaptureSession>>>,
    pub last_audio: Arc<Mutex<Option<audio::AudioBuffer>>>,
    pub dictation_manager: DictationManager,
    pub focus_target: Arc<Mutex<Option<platform::focus::FocusTarget>>>,
    pub manifest: Arc<Mutex<Manifest>>,
    pub download_manager: DownloadManager,
    pub active_model_id: Arc<Mutex<Option<String>>>,
    pub db: db::DbHandle,
    pub selection_state: Arc<Mutex<output::selection::SelectionState>>,
    pub streaming_handle: Arc<Mutex<Option<dictation::streaming::StreamingHandle>>>,
    pub pending_edit: Arc<Mutex<Option<dictation::PendingEdit>>>,
    pub active_profile_id: Arc<Mutex<Option<i64>>>,
    pub mute_guard: Arc<Mutex<Option<audio::mute::MuteGuard>>>,
}

#[tauri::command]
fn ping() -> String {
    "pong".to_string()
}

pub fn run() {
    let _guard = logging::init();

    info!(
        version = env!("CARGO_PKG_VERSION"),
        platform = std::env::consts::OS,
        arch = std::env::consts::ARCH,
        "EchoType starting"
    );

    // Check platform permissions at startup
    let permissions = platform::permissions::check_permissions();
    info!(
        accessibility = permissions.accessibility,
        "Platform permissions checked"
    );

    // Load model manifest
    let manifest = models::manifest::load_bundled().expect("Bundled manifest must be valid");
    info!(model_count = manifest.models.len(), "Model manifest loaded");

    // Open database
    let db_dir = dirs::data_dir()
        .expect("Cannot determine app data directory")
        .join("com.echotype.app");
    let db_path = db_dir.join("echotype.db");
    let db_handle = db::open(&db_path).expect("Cannot open database");

    // Restore active model from settings
    let active_model_id = {
        let conn = db_handle.blocking_lock();
        match db::settings::get(&conn, "active_model_id") {
            Ok(Some(v)) => serde_json::from_str::<String>(&v).ok(),
            _ => None,
        }
    };
    if let Some(ref id) = active_model_id {
        info!(model_id = %id, "Restored active model from settings");
    }

    let state = AppState {
        engine_manager: EngineManager::new(),
        capture_session: Arc::new(Mutex::new(None)),
        last_audio: Arc::new(Mutex::new(None)),
        dictation_manager: DictationManager::new(),
        focus_target: Arc::new(Mutex::new(None)),
        manifest: Arc::new(Mutex::new(manifest)),
        download_manager: DownloadManager::new(),
        active_model_id: Arc::new(Mutex::new(active_model_id)),
        db: db_handle,
        selection_state: Arc::new(Mutex::new(output::selection::SelectionState::NoSelection)),
        streaming_handle: Arc::new(Mutex::new(None)),
        pending_edit: Arc::new(Mutex::new(None)),
        active_profile_id: Arc::new(Mutex::new(None)),
        mute_guard: Arc::new(Mutex::new(None)),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            ping,
            commands::list_audio_devices,
            commands::start_capture,
            commands::stop_capture,
            commands::transcribe_audio,
            commands::load_model,
            commands::get_dictation_state,
            commands::list_available_models,
            commands::list_installed_models,
            commands::download_model,
            commands::cancel_download,
            commands::delete_model,
            commands::set_active_model,
            commands::get_active_model,
            commands::check_permissions,
            commands::open_permission_settings,
            commands::get_setting,
            commands::set_setting,
            commands::get_all_settings,
            commands::reset_setting,
            commands::export_settings,
            commands::import_settings,
            commands::get_history,
            commands::get_history_entry,
            commands::delete_history_entry,
            commands::clear_history,
            commands::copy_history_text,
            commands::get_edit_buffer_text,
            commands::edit_buffer_insert,
            commands::edit_buffer_discard,
            commands::edit_buffer_copy,
            commands::list_profiles,
            commands::get_profile,
            commands::create_profile,
            commands::update_profile,
            commands::delete_profile,
            commands::get_profile_settings,
            commands::set_profile_setting,
            commands::remove_profile_setting,
            commands::list_vocabulary_collections,
            commands::create_vocabulary_collection,
            commands::rename_vocabulary_collection,
            commands::delete_vocabulary_collection,
            commands::list_vocabulary_entries,
            commands::add_vocabulary_entry,
            commands::update_vocabulary_entry,
            commands::delete_vocabulary_entry,
            commands::vocabulary_entry_count,
            commands::import_vocabulary_json,
            commands::toggle_private_mode,
            commands::set_api_key,
            commands::get_api_key_status,
            commands::delete_api_key,
            commands::validate_api_key,
            commands::list_cloud_providers,
            commands::activate_cloud_engine,
            commands::activate_local_engine,
            commands::get_metrics_today,
            commands::get_metrics_range,
            commands::get_engine_breakdown,
            commands::get_lifetime_metrics,
            commands::set_typing_baseline,
            commands::check_for_update,
            commands::get_build_info,
            commands::get_diagnostic_info,
        ])
        .setup(|app| {
            // Register the dictation hotkey
            if let Err(e) = hotkey::register_dictation_hotkey(app.handle()) {
                error!(%e, "Failed to register dictation hotkey at startup");
            }

            // Set up dictation event listeners
            setup_dictation_listeners(app.handle());

            // Clean up stale partial downloads
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Ok(models_dir) = DownloadManager::models_dir(&handle) {
                    DownloadManager::cleanup_stale_partials(&models_dir).await;
                }
            });

            // Set up system tray
            if let Err(e) = tray::setup(app.handle()) {
                error!(%e, "Failed to set up system tray");
                // Tray failed — show the main window so the app isn't invisible
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                }
            } else {
                // Only hide-on-close when tray is available
                tray::setup_window_close_behavior(app.handle());
            }

            // Load the restored active engine in background
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let state: tauri::State<'_, AppState> = handle.state();

                // Check if we should restore a cloud engine or local
                let engine_type = {
                    let conn = state.db.lock().await;
                    settings::get_typed::<String>(&conn, settings::keys::ENGINE_TYPE)
                        .unwrap_or_else(|_| "local".to_string())
                };

                let mut cloud_restored = false;
                if engine_type == "cloud" {
                    // Attempt to restore cloud engine
                    let provider_str = {
                        let conn = state.db.lock().await;
                        settings::get_typed::<String>(&conn, settings::keys::CLOUD_PROVIDER).ok()
                    };
                    if let Some(ref p) = provider_str {
                        let provider = match p.as_str() {
                            "groq" => Some(engine::cloud::CloudProvider::Groq),
                            "openai" => Some(engine::cloud::CloudProvider::OpenAi),
                            "deepgram" => Some(engine::cloud::CloudProvider::Deepgram),
                            _ => None,
                        };
                        if let Some(provider) = provider {
                            if let Ok(Some(key)) = security::keyring_store::get_api_key(provider) {
                                let result: Result<Box<dyn SttEngine>, String> = match provider {
                                    engine::cloud::CloudProvider::Groq => {
                                        engine::cloud::groq::GroqEngine::new(key)
                                            .map(|e| Box::new(e) as Box<dyn SttEngine>)
                                            .map_err(|e| e.to_string())
                                    }
                                    engine::cloud::CloudProvider::OpenAi => {
                                        let model = {
                                            let conn = state.db.lock().await;
                                            settings::get_typed::<String>(
                                                &conn,
                                                settings::keys::OPENAI_MODEL,
                                            )
                                            .unwrap_or_else(|_| "whisper-1".to_string())
                                        };
                                        engine::cloud::openai::OpenAiEngine::new(key)
                                            .map(|e| {
                                                Box::new(e.with_model(model)) as Box<dyn SttEngine>
                                            })
                                            .map_err(|e| e.to_string())
                                    }
                                    engine::cloud::CloudProvider::Deepgram => {
                                        engine::cloud::deepgram::DeepgramEngine::new(key)
                                            .map(|e| Box::new(e) as Box<dyn SttEngine>)
                                            .map_err(|e| e.to_string())
                                    }
                                };
                                match result {
                                    Ok(eng) => {
                                        info!(provider = p.as_str(), "Restored cloud engine");
                                        state.engine_manager.load(eng).await;
                                        cloud_restored = true;
                                    }
                                    Err(e) => {
                                        error!(%e, provider = p.as_str(), "Failed to restore cloud engine, falling back to local");
                                    }
                                }
                            } else {
                                error!(provider = p.as_str(), "No API key in keychain, falling back to local engine");
                            }
                        }
                    }
                }

                if !cloud_restored {
                    // Restore local Whisper model
                    let model_id = state.active_model_id.lock().await.clone();
                    if let Some(model_id) = model_id {
                        let file = {
                            let manifest = state.manifest.lock().await;
                            manifest
                                .models
                                .iter()
                                .find(|m| m.id == model_id)
                                .map(|e| e.file.clone())
                        };
                        if let Some(file) = file {
                            if let Ok(models_dir) = DownloadManager::models_dir(&handle) {
                                let model_path = models_dir.join(&file);
                                if model_path.exists() {
                                    match tokio::task::spawn_blocking(move || {
                                        engine::whisper::WhisperEngine::new(&model_path)
                                    })
                                    .await
                                    {
                                        Ok(Ok(engine)) => {
                                            let name = engine.name().to_string();
                                            state.engine_manager.load(Box::new(engine)).await;
                                            info!(model = %name, "Restored active model engine");
                                        }
                                        Ok(Err(e)) => {
                                            error!(%e, "Failed to load restored model");
                                        }
                                        Err(e) => {
                                            error!(%e, "Model load task panicked");
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Read the current activation mode from settings.
async fn get_activation_mode(state: &AppState) -> String {
    let conn = state.db.lock().await;
    settings::get_typed::<String>(&conn, settings::keys::ACTIVATION_MODE)
        .unwrap_or_else(|_| "hold".to_string())
}

/// Set up listeners for dictation hotkey events.
fn setup_dictation_listeners(app: &tauri::AppHandle) {
    // On key press: start recording (hold mode) or toggle (toggle mode)
    let handle = app.clone();
    app.listen("dictation:start", move |_event| {
        let app = handle.clone();
        tauri::async_runtime::spawn(async move {
            let state: tauri::State<'_, AppState> = app.state();
            let mode = get_activation_mode(&state).await;

            match mode.as_str() {
                "toggle" => {
                    // Toggle: if idle → start, if recording → stop
                    let current = state.dictation_manager.current_state().await;
                    if current == dictation::DictationState::Recording {
                        state.dictation_manager.on_stop(&app).await;
                    } else {
                        state.dictation_manager.on_start(&app).await;
                    }
                }
                _ => {
                    // Hold mode (default): press → start
                    state.dictation_manager.on_start(&app).await;
                }
            }
        });
    });

    // On key release: stop recording (hold mode) or ignore (toggle mode)
    let handle = app.clone();
    app.listen("dictation:stop", move |_event| {
        let app = handle.clone();
        tauri::async_runtime::spawn(async move {
            let state: tauri::State<'_, AppState> = app.state();
            let mode = get_activation_mode(&state).await;

            match mode.as_str() {
                "toggle" => {
                    // Toggle mode: release is ignored
                    debug!("Toggle mode: ignoring key release");
                }
                _ => {
                    // Hold mode: release → stop
                    state.dictation_manager.on_stop(&app).await;
                }
            }
        });
    });

    info!("Dictation event listeners registered");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_returns_pong() {
        assert_eq!(ping(), "pong");
    }
}
