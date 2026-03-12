use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use tauri::{AppHandle, Emitter};
use tokio::sync::{Mutex, oneshot};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::audio::capture::CaptureSession;
use crate::audio::AudioBuffer;
use crate::engine::{
    EngineError, StreamingConfig, StreamingPartial, StreamingSttEngine, StreamingSttSession,
};

/// Resample audio using linear interpolation.
/// Used to convert capture device sample rate (e.g. 48kHz) to engine rate (16kHz).
pub fn resample_chunk(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate || samples.is_empty() {
        return samples.to_vec();
    }
    let ratio = from_rate as f64 / to_rate as f64;
    let out_len = (samples.len() as f64 / ratio).ceil() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src_idx = i as f64 * ratio;
        let idx = src_idx as usize;
        let frac = src_idx - idx as f64;
        let s0 = samples[idx.min(samples.len() - 1)];
        let s1 = samples[(idx + 1).min(samples.len() - 1)];
        out.push(s0 + (s1 - s0) * frac as f32);
    }
    out
}

/// Minimum chunk duration in seconds before sending to streaming engine.
const MIN_CHUNK_SECS: f64 = 0.1;

/// Interval between audio chunk sends to the streaming engine.
const CHUNK_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);

/// Maximum time to wait for the STT provider to return final results after
/// CloseStream.  This is a safety-net timeout — normally the consumer_done
/// signal fires well before this.
const FINALIZE_DRAIN_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(3000);

/// Enhanced partial result emitted to the frontend.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionPartialResult {
    pub session_id: String,
    pub seq: u64,
    pub text: String,
    pub is_final: bool,
    pub stability: String,
    pub route: String,
}

/// Handle returned from `StreamingSessionController::run`, used to control
/// and finalize an active streaming session.
pub struct StreamingSessionHandle {
    cancel: Arc<AtomicBool>,
    session_id: String,
    accumulated_text: Arc<Mutex<String>>,
    /// Text of the current in-progress segment (not yet marked `is_final` by
    /// the provider).  Cleared each time a final result is accumulated.
    /// Appended to accumulated_text at finalization so trailing speech that
    /// Deepgram hasn't finalized is never lost.
    pending_segment_text: Arc<Mutex<String>>,
    session: Arc<Mutex<Option<Box<dyn StreamingSttSession>>>>,
    /// Number of samples (at capture rate) the feeder has sent to the engine.
    feeder_capture_offset: Arc<AtomicU64>,
    /// Fires when the partial consumer task has finished (channel closed by the
    /// STT provider).  `finalize()` awaits this instead of a fixed sleep.
    consumer_done: Mutex<Option<oneshot::Receiver<()>>>,
}

impl StreamingSessionHandle {
    /// Cancel the streaming session.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }

    /// Returns the session UUID.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Returns the number of capture-rate samples the feeder has already sent.
    pub fn feeder_capture_offset(&self) -> usize {
        self.feeder_capture_offset.load(Ordering::Acquire) as usize
    }

    /// Send any remaining audio, close the engine session, and return the
    /// accumulated transcript.
    pub async fn finalize(&self, final_audio: Option<&[f32]>) -> Result<String, EngineError> {
        // Send remaining audio if provided
        if let Some(audio) = final_audio {
            if !audio.is_empty() {
                let guard = self.session.lock().await;
                if let Some(ref session) = *guard {
                    debug!(
                        samples = audio.len(),
                        "Sending final audio chunk to streaming session"
                    );
                    session.send_audio(audio).await?;
                }
            }
        }

        // Close the session to signal end-of-audio.
        // For providers like Deepgram this sends CloseStream, which tells
        // the server to finish processing and return any remaining results
        // before closing the connection.
        {
            let guard = self.session.lock().await;
            if let Some(ref session) = *guard {
                debug!("Closing streaming STT session");
                session.close().await?;
            }
        }

        // Wait for the partial consumer to finish — it exits when the STT
        // provider closes the channel after returning all final results.
        // Fall back to a timeout so we never hang indefinitely.
        let consumer_rx = self.consumer_done.lock().await.take();
        if let Some(rx) = consumer_rx {
            match tokio::time::timeout(FINALIZE_DRAIN_TIMEOUT, rx).await {
                Ok(_) => {
                    debug!("Partial consumer finished, all results collected");
                }
                Err(_) => {
                    warn!(
                        timeout_ms = FINALIZE_DRAIN_TIMEOUT.as_millis() as u64,
                        "Timed out waiting for partial consumer to finish"
                    );
                }
            }
        } else {
            // Fallback if consumer_done was already taken (shouldn't happen).
            tokio::time::sleep(FINALIZE_DRAIN_TIMEOUT).await;
        }

        // Signal cancellation so background tasks (feeder, keepalive) wind down
        self.cancel.store(true, Ordering::Release);

        let mut text = self.accumulated_text.lock().await.clone();

        // Append any in-progress segment that was never marked `is_final`.
        // This handles two cases:
        //   1. Short utterances where Deepgram never sent a `speech_final`.
        //   2. Longer dictations where the last segment was still interim
        //      when the user released the hotkey.
        let pending = self.pending_segment_text.lock().await.clone();
        if !pending.is_empty() {
            info!(
                session_id = %self.session_id,
                pending_len = pending.len(),
                accumulated_len = text.len(),
                "Appending pending interim segment to final transcript"
            );
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(&pending);
        }

        info!(
            session_id = %self.session_id,
            text_len = text.len(),
            "Streaming session finalized"
        );
        Ok(text)
    }
}

/// Manages the lifecycle of a single streaming STT session.
///
/// Coordinates audio feeding from the capture buffer into the streaming engine
/// and emits partial transcription results to the frontend.
pub struct StreamingSessionController {
    session_id: String,
    seq: Arc<AtomicU64>,
    cancel: Arc<AtomicBool>,
    started_at: Instant,
    accumulated_text: Arc<Mutex<String>>,
}

impl StreamingSessionController {
    /// Create a new session controller with a fresh UUID.
    pub fn new() -> Self {
        let session_id = Uuid::new_v4().to_string();
        info!(session_id = %session_id, "Created new streaming session controller");
        Self {
            session_id,
            seq: Arc::new(AtomicU64::new(0)),
            cancel: Arc::new(AtomicBool::new(false)),
            started_at: Instant::now(),
            accumulated_text: Arc::new(Mutex::new(String::new())),
        }
    }

    /// Returns the session UUID.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Set the cancellation flag.
    pub fn cancel(&self) {
        info!(session_id = %self.session_id, "Cancelling streaming session");
        self.cancel.store(true, Ordering::Release);
    }

    /// Check whether the session has been cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Acquire)
    }

    /// Run the streaming session.
    ///
    /// Starts the engine stream, spawns audio feeder and partial consumer tasks,
    /// and returns a `StreamingSessionHandle` for controlling the session.
    pub async fn run(
        &self,
        app: AppHandle,
        capture_session: Arc<Mutex<Option<CaptureSession>>>,
        engine: Arc<dyn StreamingSttEngine>,
        config: StreamingConfig,
    ) -> Result<StreamingSessionHandle, EngineError> {
        info!(
            session_id = %self.session_id,
            engine = engine.name(),
            sample_rate = config.sample_rate,
            "Starting streaming STT session"
        );

        let (stt_session, partial_rx) = engine.start_stream(config.clone()).await?;

        let stt_session: Arc<Mutex<Option<Box<dyn StreamingSttSession>>>> =
            Arc::new(Mutex::new(Some(stt_session)));

        let cancel = self.cancel.clone();
        let seq = self.seq.clone();
        let accumulated_text = self.accumulated_text.clone();
        let session_id = self.session_id.clone();

        // --- Audio feeder task ---
        let feeder_cancel = cancel.clone();
        let feeder_session = stt_session.clone();
        let feeder_capture = capture_session.clone();
        let feeder_sample_rate = config.sample_rate;
        let feeder_capture_offset = Arc::new(AtomicU64::new(0));
        let feeder_capture_offset_writer = feeder_capture_offset.clone();

        tokio::spawn(async move {
            let mut samples_sent: usize = 0;

            debug!("Audio feeder task started");

            loop {
                if feeder_cancel.load(Ordering::Acquire) {
                    debug!("Audio feeder: cancel flag set, stopping");
                    break;
                }

                // Peek at current capture buffer
                let buffer: Option<AudioBuffer> = {
                    let guard = feeder_capture.lock().await;
                    guard.as_ref().map(|cs| cs.peek_buffer())
                };

                let buffer = match buffer {
                    Some(b) => b,
                    None => {
                        debug!("Audio feeder: capture session ended, stopping");
                        break;
                    }
                };

                let total_samples = buffer.samples.len();
                let capture_rate = buffer.sample_rate;

                if total_samples > samples_sent {
                    let new_samples = total_samples - samples_sent;

                    // Use capture rate for min chunk calculation (before resampling)
                    let min_chunk_at_capture_rate =
                        (MIN_CHUNK_SECS * capture_rate as f64) as usize;

                    if new_samples >= min_chunk_at_capture_rate {
                        let raw_chunk = &buffer.samples[samples_sent..total_samples];

                        // Resample from capture device rate to engine-expected rate
                        let chunk = if capture_rate != feeder_sample_rate {
                            resample_chunk(raw_chunk, capture_rate, feeder_sample_rate)
                        } else {
                            raw_chunk.to_vec()
                        };

                        let guard = feeder_session.lock().await;
                        if let Some(ref session) = *guard {
                            match session.send_audio(&chunk).await {
                                Ok(()) => {
                                    debug!(
                                        new_samples = new_samples,
                                        resampled_samples = chunk.len(),
                                        capture_rate = capture_rate,
                                        target_rate = feeder_sample_rate,
                                        total_sent = total_samples,
                                        "Audio feeder: sent chunk"
                                    );
                                    samples_sent = total_samples;
                                    feeder_capture_offset_writer
                                        .store(total_samples as u64, Ordering::Release);
                                }
                                Err(e) => {
                                    error!(%e, "Audio feeder: failed to send audio chunk");
                                    break;
                                }
                            }
                        } else {
                            debug!("Audio feeder: STT session gone, stopping");
                            break;
                        }
                    }
                }

                tokio::time::sleep(CHUNK_INTERVAL).await;
            }

            info!("Audio feeder task ended");
        });

        // --- Partial consumer task ---
        let consumer_cancel = cancel.clone();
        let consumer_seq = seq.clone();
        let consumer_accumulated = accumulated_text.clone();
        let consumer_session_id = session_id.clone();
        let consumer_app = app.clone();
        let (consumer_done_tx, consumer_done_rx) = oneshot::channel::<()>();
        let pending_segment_text: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
        let consumer_pending = pending_segment_text.clone();

        tokio::spawn(async move {
            let mut partial_rx = partial_rx;

            debug!("Partial consumer task started");

            /// Process a single partial result: emit to frontend, update
            /// accumulated / pending state.
            async fn process_partial(
                partial: &StreamingPartial,
                seq: &AtomicU64,
                session_id: &str,
                app: &AppHandle,
                accumulated: &Mutex<String>,
                pending: &Mutex<String>,
            ) {
                let current_seq = seq.fetch_add(1, Ordering::Relaxed);

                let stability = if partial.is_final {
                    "high".to_string()
                } else {
                    "low".to_string()
                };

                let result = SessionPartialResult {
                    session_id: session_id.to_string(),
                    seq: current_seq,
                    text: partial.text.clone(),
                    is_final: partial.is_final,
                    stability,
                    route: "streaming".to_string(),
                };

                debug!(
                    seq = current_seq,
                    is_final = partial.is_final,
                    text_len = partial.text.len(),
                    "Emitting partial result"
                );

                if let Err(e) = app.emit("dictation:partial", &result) {
                    warn!(%e, "Failed to emit dictation:partial event");
                }

                if partial.is_final {
                    if !partial.text.is_empty() {
                        let mut acc = accumulated.lock().await;
                        if !acc.is_empty() {
                            acc.push(' ');
                        }
                        acc.push_str(&partial.text);
                        debug!(
                            accumulated_len = acc.len(),
                            "Appended final segment to accumulated text"
                        );
                    }
                    // Clear pending — this segment is now finalized
                    *pending.lock().await = String::new();
                } else if !partial.text.is_empty() {
                    *pending.lock().await = partial.text.clone();
                }
            }

            // Main loop: wait for partials from the channel.  Also
            // periodically check the cancel flag so we exit cleanly for
            // providers that don't close the connection (e.g. OpenAI).
            loop {
                let partial = tokio::select! {
                    result = partial_rx.recv() => {
                        match result {
                            Some(p) => p,
                            None => {
                                debug!("Partial consumer: channel closed");
                                break;
                            }
                        }
                    }
                    _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                        if consumer_cancel.load(Ordering::Acquire) {
                            debug!("Partial consumer: cancel flag set, draining remaining messages");
                            // Drain any messages already buffered in the channel.
                            while let Ok(p) = partial_rx.try_recv() {
                                process_partial(
                                    &p,
                                    &consumer_seq,
                                    &consumer_session_id,
                                    &consumer_app,
                                    &consumer_accumulated,
                                    &consumer_pending,
                                )
                                .await;
                            }
                            break;
                        }
                        continue;
                    }
                };

                process_partial(
                    &partial,
                    &consumer_seq,
                    &consumer_session_id,
                    &consumer_app,
                    &consumer_accumulated,
                    &consumer_pending,
                )
                .await;
            }

            // Signal that we've drained all results
            let _ = consumer_done_tx.send(());
            info!("Partial consumer task ended");
        });

        let elapsed = self.started_at.elapsed();
        info!(
            session_id = %self.session_id,
            setup_ms = elapsed.as_millis(),
            "Streaming session running"
        );

        Ok(StreamingSessionHandle {
            cancel,
            session_id,
            accumulated_text,
            pending_segment_text,
            session: stt_session,
            feeder_capture_offset,
            consumer_done: Mutex::new(Some(consumer_done_rx)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_partial_result_serializes() {
        let result = SessionPartialResult {
            session_id: "test-id".to_string(),
            seq: 42,
            text: "hello world".to_string(),
            is_final: false,
            stability: "low".to_string(),
            route: "streaming".to_string(),
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("test-id"));
        assert!(json.contains("42"));
        assert!(json.contains("hello world"));
        assert!(json.contains("\"is_final\":false"));
        assert!(json.contains("\"stability\":\"low\""));
        assert!(json.contains("\"route\":\"streaming\""));
    }

    #[test]
    fn session_partial_result_serializes_final() {
        let result = SessionPartialResult {
            session_id: "abc".to_string(),
            seq: 0,
            text: "done".to_string(),
            is_final: true,
            stability: "high".to_string(),
            route: "streaming".to_string(),
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"is_final\":true"));
        assert!(json.contains("\"stability\":\"high\""));
    }

    #[test]
    fn session_id_is_valid_uuid() {
        let controller = StreamingSessionController::new();
        let id = controller.session_id();

        // Verify it parses as a valid UUID
        let parsed = Uuid::parse_str(id);
        assert!(parsed.is_ok(), "session_id should be a valid UUID, got: {id}");

        // Verify it's a v4 UUID
        let uuid = parsed.unwrap();
        assert_eq!(uuid.get_version(), Some(uuid::Version::Random));
    }

    #[test]
    fn cancel_flag_works() {
        let controller = StreamingSessionController::new();

        assert!(!controller.is_cancelled(), "should not be cancelled initially");

        controller.cancel();

        assert!(controller.is_cancelled(), "should be cancelled after cancel()");
    }

    #[test]
    fn cancel_flag_is_shared() {
        let controller = StreamingSessionController::new();
        let cancel_ref = controller.cancel.clone();

        assert!(!cancel_ref.load(Ordering::Acquire));

        controller.cancel();

        assert!(cancel_ref.load(Ordering::Acquire));
    }

    #[test]
    fn new_sessions_have_unique_ids() {
        let a = StreamingSessionController::new();
        let b = StreamingSessionController::new();
        assert_ne!(a.session_id(), b.session_id());
    }

    #[test]
    fn seq_starts_at_zero() {
        let controller = StreamingSessionController::new();
        assert_eq!(controller.seq.load(Ordering::Relaxed), 0);
    }
}
