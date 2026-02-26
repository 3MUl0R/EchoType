mod audio;
mod commands;
mod engine;
mod logging;

use std::sync::Arc;

use tokio::sync::Mutex;
use tracing::info;

use engine::manager::EngineManager;

/// Shared application state accessible from Tauri commands.
pub struct AppState {
    pub engine_manager: EngineManager,
    pub capture_session: Arc<Mutex<Option<audio::capture::CaptureSession>>>,
    pub last_audio: Arc<Mutex<Option<audio::AudioBuffer>>>,
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

    let state = AppState {
        engine_manager: EngineManager::new(),
        capture_session: Arc::new(Mutex::new(None)),
        last_audio: Arc::new(Mutex::new(None)),
    };

    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            ping,
            commands::list_audio_devices,
            commands::start_capture,
            commands::stop_capture,
            commands::transcribe_audio,
            commands::load_model,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_returns_pong() {
        assert_eq!(ping(), "pong");
    }
}
