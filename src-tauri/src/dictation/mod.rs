use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use crate::audio::{capture, pipeline, AudioBuffer};
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
        let db = app_state.db.clone();
        let audio_for_history = buffer.clone();

        tokio::spawn(async move {
            // Read settings before transcription (async-safe)
            let (denoise_enabled, output_method) = {
                let conn = db.lock().await;
                let denoise = crate::settings::get_typed::<bool>(
                    &conn,
                    crate::settings::keys::DENOISE_ENABLED,
                )
                .unwrap_or(true);
                let method = crate::settings::get_typed::<output::OutputMethod>(
                    &conn,
                    crate::settings::keys::OUTPUT_METHOD,
                )
                .unwrap_or_else(|_| output::auto_select_method());
                (denoise, method)
            };

            let result = run_transcription_pipeline(buffer, &engine_manager, denoise_enabled).await;

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

                    // Insert text (blocking: uses thread::sleep + enigo)
                    let text_for_insert = text.clone();
                    let insert_result = tokio::task::spawn_blocking(move || {
                        output::insert_text(&text_for_insert, &output_method)
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

                        // Save to history
                        save_to_history(&db, &app_handle, &text, &audio_for_history, total_latency)
                            .await;

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
    denoise_enabled: bool,
) -> Result<String, String> {
    // Pipeline: denoise + resample (blocking work)
    let processed = tokio::task::spawn_blocking(move || {
        let config = pipeline::PipelineConfig { denoise_enabled };
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

/// Save a completed dictation to history.
async fn save_to_history(
    db: &crate::db::DbHandle,
    app: &AppHandle,
    text: &str,
    audio: &AudioBuffer,
    _latency_ms: u64,
) {
    // Read settings (async-safe)
    let (enabled, engine_id, max_count) = {
        let conn = db.lock().await;
        let enabled =
            crate::settings::get_typed::<bool>(&conn, crate::settings::keys::HISTORY_ENABLED)
                .unwrap_or(true);
        let engine_id =
            crate::settings::get_typed::<String>(&conn, crate::settings::keys::ACTIVE_MODEL_ID)
                .ok();
        let max_count: i64 =
            crate::settings::get_typed(&conn, crate::settings::keys::HISTORY_RETENTION_COUNT)
                .unwrap_or(50);
        (enabled, engine_id, max_count)
    };

    if !enabled {
        return;
    }

    let now = chrono::Utc::now().to_rfc3339();
    let duration_ms = (audio.duration_secs() * 1000.0) as i64;

    // Calculate words per minute
    let word_count = text.split_whitespace().count() as f64;
    let minutes = duration_ms as f64 / 60_000.0;
    let wpm = if minutes > 0.0 {
        Some(word_count / minutes)
    } else {
        None
    };

    // Save audio to WAV file (no DB lock needed)
    let audio_rel_path = save_audio_wav(app, audio, &now);

    let params = crate::db::history::InsertParams {
        text,
        audio_path: audio_rel_path.as_deref(),
        duration_ms: Some(duration_ms),
        engine_id: engine_id.as_deref(),
        language: None,
        words_per_minute: wpm,
        created_at: &now,
    };

    // Insert and enforce retention under a single lock
    let conn = db.lock().await;
    match crate::db::history::insert(&conn, &params) {
        Ok(id) => {
            debug!(history_id = id, "Saved dictation to history");

            if let Ok(paths) = crate::db::history::enforce_retention_count(&conn, max_count) {
                for rel_path in paths {
                    if let Ok(data_dir) = app.path().app_data_dir() {
                        let full_path = data_dir.join(&rel_path);
                        let _ = std::fs::remove_file(&full_path);
                    }
                }
            }
        }
        Err(e) => {
            error!(%e, "Failed to save dictation to history");
        }
    }
}

/// Save audio buffer to a WAV file, returning the relative path.
fn save_audio_wav(app: &AppHandle, audio: &AudioBuffer, timestamp: &str) -> Option<String> {
    let data_dir = app.path().app_data_dir().ok()?;
    let audio_dir = data_dir.join("audio");
    std::fs::create_dir_all(&audio_dir).ok()?;

    // Create filename from timestamp (sanitize for filesystem)
    let safe_ts: String = timestamp
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let filename = format!("{safe_ts}.wav");
    let full_path = audio_dir.join(&filename);
    let rel_path = format!("audio/{filename}");

    // Write WAV using hound
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: audio.sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };

    match hound::WavWriter::create(&full_path, spec) {
        Ok(mut writer) => {
            for &sample in &audio.samples {
                if writer.write_sample(sample).is_err() {
                    break;
                }
            }
            if writer.finalize().is_err() {
                warn!("Failed to finalize WAV file");
            }
            Some(rel_path)
        }
        Err(e) => {
            warn!(%e, "Failed to create WAV file for history");
            None
        }
    }
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
