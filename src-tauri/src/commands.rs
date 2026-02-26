use serde::Serialize;
use tauri::{AppHandle, State};
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
    let session = capture::start_capture().map_err(|e| e.to_string())?;
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

    // Run audio pipeline (denoise + resample) off the async runtime
    let processed = tokio::task::spawn_blocking(move || {
        let config = pipeline::PipelineConfig::default();
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

    // Update active model ID
    *state.active_model_id.lock().await = Some(model_id);

    info!(model = %name, "Active model switched");
    Ok(name)
}

#[tauri::command]
pub async fn get_active_model(state: State<'_, AppState>) -> Result<Option<String>, String> {
    Ok(state.active_model_id.lock().await.clone())
}
