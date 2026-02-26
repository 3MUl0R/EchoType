use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use crate::audio::{capture, pipeline};
use crate::engine::TranscribeRequest;
use crate::output;
use crate::platform::focus;
use crate::AppState;

/// Minimum recording duration in seconds to be worth transcribing.
const MIN_RECORDING_SECS: f64 = 0.3;

/// States of the dictation flow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DictationState {
    Idle,
    Recording,
    Transcribing,
    Inserting,
}

/// Events emitted to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DictationEvent {
    pub state: DictationState,
    pub text: Option<String>,
    pub error: Option<String>,
    pub latency_ms: Option<u64>,
}

/// Manages the dictation lifecycle.
pub struct DictationManager {
    state: Arc<Mutex<DictationState>>,
}

impl DictationManager {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(DictationState::Idle)),
        }
    }

    /// Get the current dictation state.
    pub async fn current_state(&self) -> DictationState {
        self.state.lock().await.clone()
    }

    /// Handle dictation start (hotkey press).
    pub async fn on_start(&self, app: &AppHandle) {
        // Check state without holding lock across blocking work
        {
            let state = self.state.lock().await;
            if *state != DictationState::Idle {
                debug!(
                    current = ?*state,
                    "Ignoring dictation start: not idle"
                );
                return;
            }
        }

        // Capture the currently focused app before we do anything
        let focus_target = focus::capture_focus();

        // Start audio capture (blocking I/O, but short-lived)
        let app_state: tauri::State<'_, AppState> = app.state();
        match capture::start_capture() {
            Ok(session) => {
                let mut capture_guard = app_state.capture_session.lock().await;
                *capture_guard = Some(session);

                // Store focus target for later restore
                *app_state.focus_target.lock().await = focus_target;

                *self.state.lock().await = DictationState::Recording;
                emit_state(
                    app,
                    &DictationEvent {
                        state: DictationState::Recording,
                        text: None,
                        error: None,
                        latency_ms: None,
                    },
                );
                info!("Dictation recording started");
            }
            Err(e) => {
                error!(%e, "Failed to start capture for dictation");
                emit_state(
                    app,
                    &DictationEvent {
                        state: DictationState::Idle,
                        text: None,
                        error: Some(e.to_string()),
                        latency_ms: None,
                    },
                );
            }
        }
    }

    /// Handle dictation stop (hotkey release).
    pub async fn on_stop(&self, app: &AppHandle) {
        let release_time = Instant::now();

        {
            let current = self.state.lock().await;
            if *current != DictationState::Recording {
                debug!(
                    current = ?*current,
                    "Ignoring dictation stop: not recording"
                );
                return;
            }
        }

        // Take the capture session
        let app_state: tauri::State<'_, AppState> = app.state();
        let session = {
            let mut guard = app_state.capture_session.lock().await;
            guard.take()
        };

        let session = match session {
            Some(s) => s,
            None => {
                warn!("No capture session found on dictation stop");
                *self.state.lock().await = DictationState::Idle;
                emit_state(
                    app,
                    &DictationEvent {
                        state: DictationState::Idle,
                        text: None,
                        error: Some("No capture session".to_string()),
                        latency_ms: None,
                    },
                );
                return;
            }
        };

        let buffer = session.stop();

        // Skip very short recordings
        if buffer.duration_secs() < MIN_RECORDING_SECS {
            info!(
                duration = buffer.duration_secs(),
                "Recording too short, skipping transcription"
            );
            *self.state.lock().await = DictationState::Idle;
            emit_state(
                app,
                &DictationEvent {
                    state: DictationState::Idle,
                    text: None,
                    error: None,
                    latency_ms: None,
                },
            );
            return;
        }

        // Transition to transcribing
        *self.state.lock().await = DictationState::Transcribing;
        emit_state(
            app,
            &DictationEvent {
                state: DictationState::Transcribing,
                text: None,
                error: None,
                latency_ms: None,
            },
        );

        // Store last audio
        *app_state.last_audio.lock().await = Some(buffer.clone());

        // Run pipeline + transcription in background
        let state_ref = self.state.clone();
        let app_handle = app.clone();
        let engine_manager = app_state.engine_manager.clone();
        let focus_target = app_state.focus_target.lock().await.clone();

        tokio::spawn(async move {
            let result = run_transcription_pipeline(buffer, &engine_manager).await;

            match result {
                Ok(text) => {
                    let latency = release_time.elapsed().as_millis() as u64;
                    info!(
                        latency_ms = latency,
                        text_len = text.len(),
                        "Dictation transcription complete"
                    );

                    // Transition to inserting
                    *state_ref.lock().await = DictationState::Inserting;
                    emit_state(
                        &app_handle,
                        &DictationEvent {
                            state: DictationState::Inserting,
                            text: Some(text.clone()),
                            error: None,
                            latency_ms: Some(latency),
                        },
                    );

                    // Restore focus to the original app before inserting
                    if let Some(ref target) = focus_target {
                        focus::restore_focus(target);
                    }

                    // Select output method based on permissions/platform
                    let method = output::auto_select_method();

                    // Insert text (blocking: uses thread::sleep + enigo)
                    let text_for_insert = text.clone();
                    let insert_result = tokio::task::spawn_blocking(move || {
                        output::insert_text(&text_for_insert, &method)
                    })
                    .await
                    .unwrap_or_else(|e| Err(format!("Insert task panicked: {e}")));
                    if let Err(e) = insert_result {
                        error!(%e, "Text insertion failed");
                        emit_state(
                            &app_handle,
                            &DictationEvent {
                                state: DictationState::Idle,
                                text: Some(text),
                                error: Some(format!("Insertion failed: {e}")),
                                latency_ms: Some(latency),
                            },
                        );
                    } else {
                        let total_latency = release_time.elapsed().as_millis() as u64;
                        info!(total_latency_ms = total_latency, "Text inserted");
                        emit_state(
                            &app_handle,
                            &DictationEvent {
                                state: DictationState::Idle,
                                text: Some(text),
                                error: None,
                                latency_ms: Some(total_latency),
                            },
                        );
                    }

                    *state_ref.lock().await = DictationState::Idle;
                }
                Err(e) => {
                    error!(%e, "Dictation transcription failed");
                    *state_ref.lock().await = DictationState::Idle;
                    emit_state(
                        &app_handle,
                        &DictationEvent {
                            state: DictationState::Idle,
                            text: None,
                            error: Some(e),
                            latency_ms: None,
                        },
                    );
                }
            }
        });
    }
}

/// Run the audio pipeline and transcription.
async fn run_transcription_pipeline(
    buffer: crate::audio::AudioBuffer,
    engine_manager: &crate::engine::manager::EngineManager,
) -> Result<String, String> {
    // Pipeline: denoise + resample (blocking work)
    let processed = tokio::task::spawn_blocking(move || {
        let config = pipeline::PipelineConfig::default();
        pipeline::process(&buffer, &config)
    })
    .await
    .map_err(|e| format!("Pipeline task failed: {e}"))?
    .map_err(|e| format!("Audio pipeline error: {e}"))?;

    // Transcribe
    let request = TranscribeRequest {
        audio: processed.samples,
        sample_rate: processed.sample_rate,
        language: None,
    };

    let transcription = engine_manager
        .transcribe(request)
        .await
        .map_err(|e| e.to_string())?;

    Ok(transcription.text)
}

fn emit_state(app: &AppHandle, event: &DictationEvent) {
    if let Err(e) = app.emit("dictation:state", event) {
        error!(%e, "Failed to emit dictation state");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn starts_idle() {
        let mgr = DictationManager::new();
        assert_eq!(mgr.current_state().await, DictationState::Idle);
    }

    #[test]
    fn dictation_state_serializes() {
        let event = DictationEvent {
            state: DictationState::Recording,
            text: None,
            error: None,
            latency_ms: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"recording\""));
    }
}
