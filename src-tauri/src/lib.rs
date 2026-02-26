mod audio;
mod commands;
mod dictation;
mod engine;
mod hotkey;
mod logging;
mod models;
mod output;
mod platform;

use std::sync::Arc;

use tauri::{Listener, Manager};
use tokio::sync::Mutex;
use tracing::{error, info};

use dictation::DictationManager;
use engine::manager::EngineManager;
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

    let state = AppState {
        engine_manager: EngineManager::new(),
        capture_session: Arc::new(Mutex::new(None)),
        last_audio: Arc::new(Mutex::new(None)),
        dictation_manager: DictationManager::new(),
        focus_target: Arc::new(Mutex::new(None)),
        manifest: Arc::new(Mutex::new(manifest)),
        download_manager: DownloadManager::new(),
        active_model_id: Arc::new(Mutex::new(None)),
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

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Set up listeners for dictation hotkey events.
fn setup_dictation_listeners(app: &tauri::AppHandle) {
    let handle = app.clone();
    app.listen("dictation:start", move |_event| {
        let app = handle.clone();
        tauri::async_runtime::spawn(async move {
            let state: tauri::State<'_, AppState> = app.state();
            state.dictation_manager.on_start(&app).await;
        });
    });

    let handle = app.clone();
    app.listen("dictation:stop", move |_event| {
        let app = handle.clone();
        tauri::async_runtime::spawn(async move {
            let state: tauri::State<'_, AppState> = app.state();
            state.dictation_manager.on_stop(&app).await;
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
