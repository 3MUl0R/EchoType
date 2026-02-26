use tauri::State;
use tracing::{error, info};

use crate::audio::{capture, pipeline, AudioDeviceInfo};
use crate::engine::whisper::WhisperEngine;
use crate::engine::{Language, SttEngine, TranscribeRequest, Transcription};
use crate::AppState;

#[tauri::command]
pub fn list_audio_devices() -> Vec<AudioDeviceInfo> {
    capture::list_devices()
}

#[tauri::command]
pub async fn start_capture(state: State<'_, AppState>) -> Result<(), String> {
    let session = capture::start_capture().map_err(|e| e.to_string())?;
    let mut guard = state.capture_session.lock().await;
    if guard.is_some() {
        return Err("Capture already in progress".to_string());
    }
    *guard = Some(session);
    Ok(())
}

#[tauri::command]
pub async fn stop_capture(state: State<'_, AppState>) -> Result<f64, String> {
    let mut guard = state.capture_session.lock().await;
    let session = guard.take().ok_or("No active capture session")?;
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
    let audio_guard = state.last_audio.lock().await;
    let raw = audio_guard
        .as_ref()
        .ok_or("No audio to transcribe. Record something first.")?
        .clone();
    drop(audio_guard);

    if raw.is_empty() {
        return Err("Audio buffer is empty".to_string());
    }

    info!(
        duration_secs = raw.duration_secs(),
        sample_rate = raw.sample_rate,
        "Processing audio for transcription"
    );

    // Run audio pipeline (denoise + resample to 16kHz)
    let config = pipeline::PipelineConfig::default();
    let processed = pipeline::process(&raw, &config)?;

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
    let model_path = std::path::PathBuf::from(&path);
    let engine = WhisperEngine::new(&model_path).map_err(|e| e.to_string())?;
    let name = engine.name().to_string();
    state.engine_manager.load(Box::new(engine)).await;
    Ok(name)
}
