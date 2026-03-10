pub mod streaming;
pub mod vocabulary;

use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use crate::audio::denoise::SuppressionLevel;
use crate::audio::{capture, feedback, pipeline, AudioBuffer};
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
    Editing,
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

/// Timing breakdown for a single dictation event.
#[derive(Debug, Clone, Default)]
pub struct LatencyBreakdown {
    /// Time for audio pipeline (denoise + resample).
    pub processing_ms: u64,
    /// Time for the transcription engine (includes network for cloud).
    pub transcription_ms: u64,
    /// Network portion (cloud only, estimated).
    pub network_ms: u64,
}

/// State stored when the edit buffer window is open.
pub struct PendingEdit {
    pub text: String,
    pub focus_target: Option<focus::FocusTarget>,
    pub selection: output::selection::SelectionState,
    pub output_method: output::OutputMethod,
    pub auto_submit_enabled: bool,
    pub auto_submit_key: output::AutoSubmitKey,
    pub auto_submit_delay_ms: u64,
    pub audio: AudioBuffer,
    pub release_time: Instant,
    pub latency_breakdown: LatencyBreakdown,
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

        // Start suppressing hotkey key repeats so the letter key doesn't
        // leak into the focused app while the user holds the hotkey combo.
        crate::hotkey::start_key_suppression();

        // Start audio capture (blocking I/O, but short-lived)
        let app_state: tauri::State<'_, AppState> = app.state();

        // Match focused app to a profile
        if let Some(ref target) = focus_target {
            let profile = {
                let conn = app_state.db.lock().await;
                crate::db::profiles::find_by_app(&conn, &target.app_id, &target.id_type)
                    .unwrap_or(None)
            };
            if let Some(p) = &profile {
                info!(profile_id = p.id, profile_name = %p.name, "Matched app profile");
            }
            *app_state.active_profile_id.lock().await = profile.map(|p| p.id);
        } else {
            *app_state.active_profile_id.lock().await = None;
        }

        // Skip selection detection — the Ctrl+C simulation it uses causes
        // side effects in terminals (SIGINT) and editors (triggers keybindings).
        // TODO: revisit with a safer detection method (e.g. accessibility APIs).
        *app_state.selection_state.lock().await = output::selection::SelectionState::NoSelection;

        // Read mic, feedback, and streaming settings together
        let (
            selected_device,
            auto_fallback,
            fb_enabled,
            fb_volume,
            streaming_enabled,
            suppression_level,
            mute_audio,
        ) = {
            let conn = app_state.db.lock().await;
            let device = crate::settings::get_typed::<String>(
                &conn,
                crate::settings::keys::SELECTED_MIC_DEVICE,
            )
            .ok();
            let fallback =
                crate::settings::get_typed::<bool>(&conn, crate::settings::keys::MIC_AUTO_FALLBACK)
                    .unwrap_or(true);
            let enabled = crate::settings::get_typed::<bool>(
                &conn,
                crate::settings::keys::AUDIO_FEEDBACK_ENABLED,
            )
            .unwrap_or(true);
            let volume = crate::settings::get_typed::<f64>(
                &conn,
                crate::settings::keys::AUDIO_FEEDBACK_VOLUME,
            )
            .unwrap_or(0.5);
            let stream =
                crate::settings::get_typed::<bool>(&conn, crate::settings::keys::STREAMING_ENABLED)
                    .unwrap_or(true);
            let level_str = crate::settings::get_typed::<String>(
                &conn,
                crate::settings::keys::NOISE_SUPPRESSION_LEVEL,
            )
            .unwrap_or_else(|_| "moderate".to_string());
            let mute =
                crate::settings::get_typed::<bool>(&conn, crate::settings::keys::MUTE_SYSTEM_AUDIO)
                    .unwrap_or(false);
            (
                device,
                fallback,
                enabled,
                volume,
                stream,
                SuppressionLevel::from_str(&level_str),
                mute,
            )
        };

        // Play start chime BEFORE muting/opening mic so it's audible
        if fb_enabled {
            feedback::play_chime(feedback::Chime::Start, fb_volume as f32);
        }

        // Mute system audio if enabled (after chime, before capture)
        if mute_audio {
            let guard = crate::audio::mute::mute_system_audio();
            *app_state.mute_guard.lock().await = guard;
        }

        match capture::start_capture_with_device(selected_device.as_deref(), auto_fallback) {
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

                // Start streaming if enabled
                if streaming_enabled {
                    let handle = streaming::start_streaming(
                        app.clone(),
                        app_state.capture_session.clone(),
                        app_state.engine_manager.clone(),
                        suppression_level,
                    );
                    *app_state.streaming_handle.lock().await = Some(handle);
                }

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

        // Stop suppressing hotkey key repeats now that the user released the keys.
        crate::hotkey::stop_key_suppression();

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

        // Cancel streaming
        let app_state: tauri::State<'_, AppState> = app.state();
        if let Some(handle) = app_state.streaming_handle.lock().await.take() {
            handle.cancel();
        }

        // Drop mute guard to restore system audio
        app_state.mute_guard.lock().await.take();

        // Take the capture session
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

        // Play stop chime (non-blocking)
        play_feedback_chime(&app_state, feedback::Chime::Stop).await;

        // Store last audio
        *app_state.last_audio.lock().await = Some(buffer.clone());

        // Run pipeline + transcription in background
        let state_ref = self.state.clone();
        let app_handle = app.clone();
        let engine_manager = app_state.engine_manager.clone();
        let focus_target = app_state.focus_target.lock().await.clone();
        let selection = app_state.selection_state.lock().await.clone();
        let db = app_state.db.clone();
        let audio_for_history = buffer.clone();
        let profile_id = *app_state.active_profile_id.lock().await;

        let pending_edit_arc = app_state.pending_edit.clone();

        tokio::spawn(async move {
            // Read settings before transcription (profile-aware)
            let pid = profile_id;
            let (
                suppression_level,
                output_method,
                dictation_mode,
                auto_punctuate,
                auto_submit_enabled,
                auto_submit_key,
                auto_submit_delay_ms,
                edit_buffer_enabled,
                custom_words,
            ) = {
                let conn = db.lock().await;
                let level_str = crate::settings::get_typed_with_profile::<String>(
                    &conn,
                    crate::settings::keys::NOISE_SUPPRESSION_LEVEL,
                    pid,
                )
                .unwrap_or_else(|_| "moderate".to_string());
                let method = crate::settings::get_typed_with_profile::<output::OutputMethod>(
                    &conn,
                    crate::settings::keys::OUTPUT_METHOD,
                    pid,
                )
                .unwrap_or_else(|_| output::auto_select_method());
                let mode = crate::settings::get_typed_with_profile::<String>(
                    &conn,
                    crate::settings::keys::DICTATION_MODE,
                    pid,
                )
                .unwrap_or_else(|_| "formatted".to_string());
                let punctuate = crate::settings::get_typed_with_profile::<bool>(
                    &conn,
                    crate::settings::keys::AUTO_PUNCTUATE,
                    pid,
                )
                .unwrap_or(true);
                let submit_enabled = crate::settings::get_typed_with_profile::<bool>(
                    &conn,
                    crate::settings::keys::AUTO_SUBMIT_ENABLED,
                    pid,
                )
                .unwrap_or(false);
                let submit_key_str = crate::settings::get_typed_with_profile::<String>(
                    &conn,
                    crate::settings::keys::AUTO_SUBMIT_KEY,
                    pid,
                )
                .unwrap_or_else(|_| "enter".to_string());
                let submit_delay = crate::settings::get_typed_with_profile::<u64>(
                    &conn,
                    crate::settings::keys::AUTO_SUBMIT_DELAY_MS,
                    pid,
                )
                .unwrap_or(100);
                let edit_buf = crate::settings::get_typed_with_profile::<bool>(
                    &conn,
                    crate::settings::keys::EDIT_BUFFER_ENABLED,
                    pid,
                )
                .unwrap_or(false);
                let custom_words: Vec<String> =
                    crate::settings::get_typed_with_profile(
                        &conn,
                        crate::settings::keys::CUSTOM_WORDS,
                        pid,
                    )
                    .unwrap_or_default();
                (
                    SuppressionLevel::from_str(&level_str),
                    method,
                    mode,
                    punctuate,
                    submit_enabled,
                    output::AutoSubmitKey::from_str(&submit_key_str),
                    submit_delay,
                    edit_buf,
                    custom_words,
                )
            };

            let result =
                run_transcription_pipeline(buffer, &engine_manager, suppression_level).await;

            match result {
                Ok(pipeline_result) => {
                    let latency_breakdown = pipeline_result.latency;
                    let raw_text = pipeline_result.text;

                    // Apply custom word corrections before postprocessing
                    let corrected = if !custom_words.is_empty() {
                        vocabulary::apply_custom_words(
                            &raw_text,
                            &custom_words,
                            vocabulary::DEFAULT_THRESHOLD,
                        )
                    } else {
                        raw_text
                    };
                    let text = postprocess_text(&corrected, &dictation_mode, auto_punctuate);
                    let latency = release_time.elapsed().as_millis() as u64;
                    info!(
                        latency_ms = latency,
                        text_len = text.len(),
                        "Dictation transcription complete"
                    );

                    // Edit buffer path: open edit window instead of inserting directly
                    if edit_buffer_enabled {
                        *pending_edit_arc.lock().await = Some(PendingEdit {
                            text: text.clone(),
                            focus_target,
                            selection,
                            output_method,
                            auto_submit_enabled,
                            auto_submit_key,
                            auto_submit_delay_ms,
                            audio: audio_for_history,
                            release_time,
                            latency_breakdown: latency_breakdown.clone(),
                        });

                        *state_ref.lock().await = DictationState::Editing;
                        emit_state(
                            &app_handle,
                            &DictationEvent {
                                state: DictationState::Editing,
                                text: Some(text),
                                error: None,
                                latency_ms: Some(latency),
                            },
                        );

                        // Open the edit buffer window
                        if let Err(e) = open_edit_buffer_window(&app_handle) {
                            error!(%e, "Failed to open edit buffer window");
                            *pending_edit_arc.lock().await = None;
                            *state_ref.lock().await = DictationState::Idle;
                            emit_state(
                                &app_handle,
                                &DictationEvent {
                                    state: DictationState::Idle,
                                    text: None,
                                    error: Some(format!("Edit buffer failed: {e}")),
                                    latency_ms: None,
                                },
                            );
                        }

                        return;
                    }

                    // Direct insertion path (edit buffer disabled)
                    do_insert(
                        &app_handle,
                        &state_ref,
                        &db,
                        &text,
                        &focus_target,
                        &selection,
                        &output_method,
                        auto_submit_enabled,
                        &auto_submit_key,
                        auto_submit_delay_ms,
                        &audio_for_history,
                        release_time,
                        &latency_breakdown,
                    )
                    .await;
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

/// Perform text insertion, auto-submit, history save, and state transition.
#[allow(clippy::too_many_arguments)]
async fn do_insert(
    app: &AppHandle,
    state_ref: &Arc<Mutex<DictationState>>,
    db: &crate::db::DbHandle,
    text: &str,
    focus_target: &Option<focus::FocusTarget>,
    selection: &output::selection::SelectionState,
    output_method: &output::OutputMethod,
    auto_submit_enabled: bool,
    auto_submit_key: &output::AutoSubmitKey,
    auto_submit_delay_ms: u64,
    audio: &AudioBuffer,
    release_time: Instant,
    latency_breakdown: &LatencyBreakdown,
) {
    // Transition to inserting
    *state_ref.lock().await = DictationState::Inserting;
    emit_state(
        app,
        &DictationEvent {
            state: DictationState::Inserting,
            text: Some(text.to_string()),
            error: None,
            latency_ms: Some(release_time.elapsed().as_millis() as u64),
        },
    );

    // Restore focus to the original app before inserting
    if let Some(ref target) = focus_target {
        focus::restore_focus(target);
    }

    // Log selection-aware replacement
    if let output::selection::SelectionState::Selected(sel) = selection {
        info!(
            selected_len = sel.len(),
            "Replacing selected text with transcription"
        );
    }

    // Insert text (blocking: uses thread::sleep + enigo)
    let insert_start = Instant::now();
    let text_owned = text.to_string();
    let method = output_method.clone();
    let insert_result =
        tokio::task::spawn_blocking(move || output::insert_text(&text_owned, &method))
            .await
            .unwrap_or_else(|e| Err(format!("Insert task panicked: {e}")));
    let insertion_ms = insert_start.elapsed().as_millis() as u64;

    if let Err(e) = insert_result {
        error!(%e, "Text insertion failed");
        *state_ref.lock().await = DictationState::Idle;
        emit_state(
            app,
            &DictationEvent {
                state: DictationState::Idle,
                text: Some(text.to_string()),
                error: Some(format!("Insertion failed: {e}")),
                latency_ms: Some(release_time.elapsed().as_millis() as u64),
            },
        );
        return;
    }

    // Auto-submit after successful insertion (skip for clipboard-only mode)
    if auto_submit_enabled && *output_method != output::OutputMethod::ClipboardOnly {
        let key = auto_submit_key.clone();
        let delay = auto_submit_delay_ms;
        let submit_result = tokio::task::spawn_blocking(move || {
            std::thread::sleep(std::time::Duration::from_millis(delay));
            output::auto_submit(&key)
        })
        .await
        .unwrap_or_else(|e| Err(format!("Submit task panicked: {e}")));
        if let Err(e) = submit_result {
            warn!(%e, "Auto-submit failed");
        }
    }

    let total_latency = release_time.elapsed().as_millis() as u64;
    info!(total_latency_ms = total_latency, "Text inserted");

    // Save to history and latency metrics
    save_to_history(db, app, text, audio, total_latency, latency_breakdown, insertion_ms).await;

    *state_ref.lock().await = DictationState::Idle;
    emit_state(
        app,
        &DictationEvent {
            state: DictationState::Idle,
            text: Some(text.to_string()),
            error: None,
            latency_ms: Some(total_latency),
        },
    );
}

/// Open the edit buffer window.
fn open_edit_buffer_window(app: &AppHandle) -> Result<(), String> {
    use tauri::WebviewWindowBuilder;

    // Close existing edit buffer window if any
    if let Some(w) = app.get_webview_window("edit-buffer") {
        let _ = w.close();
    }

    let url = tauri::WebviewUrl::App("edit-buffer.html".into());
    let window = WebviewWindowBuilder::new(app, "edit-buffer", url)
        .title("EchoType — Edit")
        .inner_size(480.0, 320.0)
        .resizable(true)
        .center()
        .focused(true)
        .build()
        .map_err(|e| format!("Failed to create edit buffer window: {e}"))?;

    // Handle external close (X button, Cmd+W) — clean up state
    let handle = app.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Destroyed = event {
            let app = handle.clone();
            tauri::async_runtime::spawn(async move {
                let state: tauri::State<'_, AppState> = app.state();
                // Only clean up if still in Editing state (Insert/Discard already handled)
                let current = state.dictation_manager.state.lock().await.clone();
                if current == DictationState::Editing {
                    state.pending_edit.lock().await.take();
                    *state.dictation_manager.state.lock().await = DictationState::Idle;
                    emit_state(
                        &app,
                        &DictationEvent {
                            state: DictationState::Idle,
                            text: None,
                            error: None,
                            latency_ms: None,
                        },
                    );
                    info!("Edit buffer window closed externally, returning to idle");
                }
            });
        }
    });

    info!("Edit buffer window opened");
    Ok(())
}

/// Called by the edit buffer window's Insert button.
pub async fn complete_edit_insert(app: &AppHandle, edited_text: String) {
    let state: tauri::State<'_, AppState> = app.state();
    let pending = state.pending_edit.lock().await.take();

    let Some(pending) = pending else {
        warn!("No pending edit found for insert");
        return;
    };

    // Close edit buffer window
    if let Some(w) = app.get_webview_window("edit-buffer") {
        let _ = w.close();
    }

    let state_ref = state.dictation_manager.state.clone();
    let db = state.db.clone();

    do_insert(
        app,
        &state_ref,
        &db,
        &edited_text,
        &pending.focus_target,
        &pending.selection,
        &pending.output_method,
        pending.auto_submit_enabled,
        &pending.auto_submit_key,
        pending.auto_submit_delay_ms,
        &pending.audio,
        pending.release_time,
        &pending.latency_breakdown,
    )
    .await;
}

/// Called by the edit buffer window's Discard button.
pub async fn complete_edit_discard(app: &AppHandle) {
    let state: tauri::State<'_, AppState> = app.state();
    state.pending_edit.lock().await.take();

    // Close edit buffer window
    if let Some(w) = app.get_webview_window("edit-buffer") {
        let _ = w.close();
    }

    *state.dictation_manager.state.lock().await = DictationState::Idle;
    emit_state(
        app,
        &DictationEvent {
            state: DictationState::Idle,
            text: None,
            error: None,
            latency_ms: None,
        },
    );

    info!("Edit buffer discarded");
}

/// Result of a transcription pipeline run, including timing.
struct PipelineResult {
    text: String,
    latency: LatencyBreakdown,
}

/// Run the audio pipeline and transcription.
async fn run_transcription_pipeline(
    buffer: crate::audio::AudioBuffer,
    engine_manager: &crate::engine::manager::EngineManager,
    suppression_level: SuppressionLevel,
) -> Result<PipelineResult, String> {
    // Pipeline: denoise + resample (blocking work)
    let pipeline_start = Instant::now();
    let processed = tokio::task::spawn_blocking(move || {
        let config = pipeline::PipelineConfig { suppression_level };
        pipeline::process(&buffer, &config)
    })
    .await
    .map_err(|e| format!("Pipeline task failed: {e}"))?
    .map_err(|e| format!("Audio pipeline error: {e}"))?;
    let processing_ms = pipeline_start.elapsed().as_millis() as u64;

    // Transcribe
    let request = TranscribeRequest {
        audio: processed.samples,
        sample_rate: processed.sample_rate,
        language: None,
    };

    let transcribe_start = Instant::now();
    let transcription = engine_manager
        .transcribe(request)
        .await
        .map_err(|e| e.to_string())?;
    let transcription_ms = transcribe_start.elapsed().as_millis() as u64;

    info!(
        processing_ms,
        transcription_ms,
        "Pipeline timing breakdown"
    );

    Ok(PipelineResult {
        text: transcription.text,
        latency: LatencyBreakdown {
            processing_ms,
            transcription_ms,
            network_ms: 0, // Will be refined later if cloud
        },
    })
}

/// Save a completed dictation to history.
async fn save_to_history(
    db: &crate::db::DbHandle,
    app: &AppHandle,
    text: &str,
    audio: &AudioBuffer,
    total_latency_ms: u64,
    latency_breakdown: &LatencyBreakdown,
    insertion_ms: u64,
) {
    // Read settings (async-safe)
    let (enabled, private_mode, engine_id, max_count) = {
        let conn = db.lock().await;
        let enabled =
            crate::settings::get_typed::<bool>(&conn, crate::settings::keys::HISTORY_ENABLED)
                .unwrap_or(true);
        let private =
            crate::settings::get_typed::<bool>(&conn, crate::settings::keys::PRIVATE_MODE_ENABLED)
                .unwrap_or(false);
        let engine_id = {
            let engine_type = crate::settings::get_typed::<String>(&conn, crate::settings::keys::ENGINE_TYPE)
                .unwrap_or_else(|_| "local".to_string());
            if engine_type == "cloud" {
                // Build provider/model composite ID for cloud engines
                let provider = crate::settings::get_typed::<String>(&conn, crate::settings::keys::CLOUD_PROVIDER)
                    .unwrap_or_else(|_| "unknown".to_string());
                let model_key = match provider.as_str() {
                    "groq" => crate::settings::keys::GROQ_MODEL,
                    "openai" => crate::settings::keys::OPENAI_MODEL,
                    "deepgram" => crate::settings::keys::DEEPGRAM_MODEL,
                    _ => crate::settings::keys::OPENAI_MODEL,
                };
                let model = crate::settings::get_typed::<String>(&conn, model_key)
                    .unwrap_or_else(|_| "unknown".to_string());
                Some(format!("{provider}/{model}"))
            } else {
                crate::settings::get_typed::<String>(&conn, crate::settings::keys::ACTIVE_MODEL_ID).ok()
            }
        };
        let max_count: i64 =
            crate::settings::get_typed(&conn, crate::settings::keys::HISTORY_RETENTION_COUNT)
                .unwrap_or(50);
        (enabled, private, engine_id, max_count)
    };

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

    info!(
        history_enabled = enabled,
        private_mode,
        engine = ?engine_id,
        word_count = text.split_whitespace().count(),
        "save_to_history: checking flags"
    );

    if private_mode {
        info!("Private mode: skipping history save, audio storage, and metrics");
        return;
    }

    // Record metrics (always, unless private mode; skip empty transcriptions)
    if let Some(wpm_val) = wpm {
        let wc = word_count as i64;
        if wc > 0 {
            let date_local = chrono::Local::now().format("%Y-%m-%d").to_string();
            let engine_label = engine_id.as_deref().unwrap_or("unknown");
            let conn = db.lock().await;
            if let Err(e) = crate::db::metrics::record_dictation(
                &conn,
                &date_local,
                wc,
                duration_ms,
                wpm_val,
                engine_label,
            ) {
                error!(%e, "Failed to record metrics");
            }
            drop(conn);
        }
    }

    if !enabled {
        return;
    }

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
            info!(history_id = id, "Saved dictation to history");

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

    // Record latency profiling data
    let engine_label = engine_id.as_deref().unwrap_or("unknown");
    let latency_params = crate::db::latency::InsertParams {
        created_at: &now,
        engine_id: engine_label,
        audio_duration_ms: duration_ms,
        processing_ms: latency_breakdown.processing_ms as i64,
        network_ms: latency_breakdown.network_ms as i64,
        transcription_ms: latency_breakdown.transcription_ms as i64,
        insertion_ms: insertion_ms as i64,
        total_ms: total_latency_ms as i64,
        word_count: word_count as i64,
    };
    if let Err(e) = crate::db::latency::insert(&conn, &latency_params) {
        warn!(%e, "Failed to save latency data");
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

/// Read audio feedback settings and play a chime if enabled.
async fn play_feedback_chime(app_state: &AppState, chime: feedback::Chime) {
    let (enabled, volume) = {
        let conn = app_state.db.lock().await;
        let enabled = crate::settings::get_typed::<bool>(
            &conn,
            crate::settings::keys::AUDIO_FEEDBACK_ENABLED,
        )
        .unwrap_or(true);
        let volume =
            crate::settings::get_typed::<f64>(&conn, crate::settings::keys::AUDIO_FEEDBACK_VOLUME)
                .unwrap_or(0.5);
        (enabled, volume)
    };

    if enabled {
        feedback::play_chime(chime, volume as f32);
    }
}

/// Post-process transcribed text based on dictation settings.
///
/// - `dictation_mode: "raw"` → lowercase, strip punctuation
/// - `dictation_mode: "formatted"` + `auto_punctuate: false` → strip punctuation
/// - `dictation_mode: "formatted"` + `auto_punctuate: true` → unchanged
fn postprocess_text(text: &str, dictation_mode: &str, auto_punctuate: bool) -> String {
    match dictation_mode {
        "raw" => {
            let stripped = strip_punctuation(text);
            stripped.to_lowercase()
        }
        _ => {
            // "formatted" mode
            if auto_punctuate {
                text.to_string()
            } else {
                strip_punctuation(text)
            }
        }
    }
}

/// Remove punctuation from text, preserving word spacing and apostrophes in contractions.
/// Punctuation between alphanumeric characters is replaced with a space (to avoid merging tokens).
fn strip_punctuation(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();

    for (i, &ch) in chars.iter().enumerate() {
        if ch.is_alphanumeric() || ch.is_whitespace() {
            result.push(ch);
            continue;
        }

        if ch == '\'' {
            // Keep apostrophes in contractions (between letters)
            let prev_letter = i > 0 && chars[i - 1].is_alphabetic();
            let next_letter = i + 1 < chars.len() && chars[i + 1].is_alphabetic();
            if prev_letter && next_letter {
                result.push(ch);
                continue;
            }
        }

        // Replace punctuation acting as a separator (between alnum chars) with space
        let prev_alnum = i > 0 && chars[i - 1].is_alphanumeric();
        let next_alnum = i + 1 < chars.len() && chars[i + 1].is_alphanumeric();
        if prev_alnum && next_alnum {
            result.push(' ');
        }
    }

    // Collapse multiple spaces
    result.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn emit_state(app: &AppHandle, event: &DictationEvent) {
    if let Err(e) = app.emit("dictation:state", event) {
        error!(%e, "Failed to emit dictation state");
    }

    // Manage the overlay window based on dictation state
    manage_overlay(app, &event.state);
}

/// Show or hide the floating status overlay window.
/// Window operations are dispatched to the main thread to avoid panics.
fn manage_overlay(app: &AppHandle, state: &DictationState) {
    let app = app.clone();
    let state = state.clone();
    let show = matches!(
        state,
        DictationState::Recording | DictationState::Transcribing | DictationState::Inserting
    );

    if let Err(e) = app.clone().run_on_main_thread(move || {
        use tauri::Manager;

        if show {
            if app.get_webview_window("overlay").is_none() {
                match create_overlay_window(&app) {
                    Ok(()) => {
                        info!("Overlay window created");
                        // Re-emit state after a short delay so the newly created
                        // window has time to load and register its event listener.
                        let app2 = app.clone();
                        let state2 = state.clone();
                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_millis(200));
                            let event = DictationEvent {
                                state: state2,
                                text: None,
                                error: None,
                                latency_ms: None,
                            };
                            if let Err(e) = app2.emit("dictation:state", &event) {
                                warn!(%e, "Failed to re-emit state to overlay");
                            }
                        });
                    }
                    Err(e) => warn!(%e, "Failed to create overlay window"),
                }
            }
        } else {
            if let Some(w) = app.get_webview_window("overlay") {
                let _ = w.close();
            }
        }
    }) {
        warn!(%e, "Failed to dispatch overlay task to main thread");
    }
}

/// Find the monitor containing the foreground window, returning
/// (x, y, width, height) in logical coordinates suitable for Tauri positioning.
fn find_active_monitor(app: &AppHandle) -> (f64, f64, f64, f64) {
    // Get the foreground window's center point (physical pixels)
    #[cfg(target_os = "windows")]
    let fg_center: Option<(i32, i32)> = {
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowRect};
        use windows_sys::Win32::Foundation::RECT;
        unsafe {
            let hwnd = GetForegroundWindow();
            if !hwnd.is_null() {
                let mut rect: RECT = std::mem::zeroed();
                if GetWindowRect(hwnd, &mut rect) != 0 {
                    Some(((rect.left + rect.right) / 2, (rect.top + rect.bottom) / 2))
                } else {
                    None
                }
            } else {
                None
            }
        }
    };
    #[cfg(not(target_os = "windows"))]
    let fg_center: Option<(i32, i32)> = None;

    // Iterate Tauri's monitors (which use correct per-monitor DPI)
    // and find the one containing the foreground window center point.
    if let Ok(monitors) = app.available_monitors() {
        if let Some((cx, cy)) = fg_center {
            for monitor in &monitors {
                let pos = monitor.position(); // physical pixel origin
                let size = monitor.size();     // physical pixel size
                let x0 = pos.x;
                let y0 = pos.y;
                let x1 = x0 + size.width as i32;
                let y1 = y0 + size.height as i32;
                if cx >= x0 && cx < x1 && cy >= y0 && cy < y1 {
                    let scale = monitor.scale_factor();
                    return (
                        pos.x as f64 / scale,
                        pos.y as f64 / scale,
                        size.width as f64 / scale,
                        size.height as f64 / scale,
                    );
                }
            }
        }
        // Fallback: use the primary monitor
        if let Some(m) = monitors.first() {
            let pos = m.position();
            let size = m.size();
            let scale = m.scale_factor();
            return (
                pos.x as f64 / scale,
                pos.y as f64 / scale,
                size.width as f64 / scale,
                size.height as f64 / scale,
            );
        }
    }

    // Last resort fallback
    (0.0, 0.0, 1920.0, 1080.0)
}

/// Create the overlay window with transparent, borderless, always-on-top properties.
/// Must be called on the main thread.
fn create_overlay_window(app: &AppHandle) -> Result<(), String> {
    use tauri::WebviewWindowBuilder;

    // Check if the overlay is enabled (use try_lock to avoid panic in async context)
    let app_state: tauri::State<'_, AppState> = app.state();
    let enabled = match app_state.db.try_lock() {
        Ok(conn) => {
            crate::settings::get_typed::<bool>(&conn, crate::settings::keys::OVERLAY_ENABLED)
                .unwrap_or(true)
        }
        Err(_) => true, // Default to showing overlay if lock unavailable
    };
    info!(enabled = enabled, "create_overlay_window check");
    if !enabled {
        return Ok(());
    }

    // Find the monitor containing the foreground window so the overlay
    // appears on the correct display in multi-monitor setups.
    let (monitor_x, monitor_y, screen_width, screen_height) = find_active_monitor(app);

    // The window needs extra padding around the pill so the glassmorphism
    // blur/glow/shadow effects don't get clipped at the window edges.
    let padding = 40.0;
    let pill_width = 160.0;
    let pill_height = 44.0;
    let win_width = pill_width + padding * 2.0;
    let win_height = pill_height + padding * 2.0;
    let x = monitor_x + (screen_width - win_width) / 2.0;
    // Position 1/3 up from the bottom of the monitor's work area
    let y = monitor_y + screen_height - (screen_height / 3.0) - padding;

    info!(x = x, y = y, win_width = win_width, win_height = win_height, "Overlay position calculated");

    let url = tauri::WebviewUrl::App("overlay.html".into());
    let _window = WebviewWindowBuilder::new(app, "overlay", url)
        .title("EchoType Overlay")
        .inner_size(win_width, win_height)
        .position(x, y)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .shadow(false)
        .build()
        .map_err(|e| format!("Failed to create overlay window: {e}"))?;

    info!("Overlay window created");
    Ok(())
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
    fn postprocess_formatted_with_punctuation() {
        let result = postprocess_text("Hello, world!", "formatted", true);
        assert_eq!(result, "Hello, world!");
    }

    #[test]
    fn postprocess_formatted_no_punctuation() {
        let result = postprocess_text("Hello, world!", "formatted", false);
        assert_eq!(result, "Hello world");
    }

    #[test]
    fn postprocess_raw_mode() {
        let result = postprocess_text("Hello, World!", "raw", true);
        assert_eq!(result, "hello world");
    }

    #[test]
    fn strip_punctuation_keeps_contractions() {
        let result = strip_punctuation("I don't know.");
        assert_eq!(result, "I don't know");
    }

    #[test]
    fn strip_punctuation_collapses_spaces() {
        let result = strip_punctuation("Hello...   world!");
        assert_eq!(result, "Hello world");
    }

    #[test]
    fn strip_punctuation_preserves_hyphenated_words() {
        let result = strip_punctuation("well-known foo-bar");
        assert_eq!(result, "well known foo bar");
    }

    #[test]
    fn strip_punctuation_preserves_decimal_numbers() {
        let result = strip_punctuation("The value is 3.14 today.");
        assert_eq!(result, "The value is 3 14 today");
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
