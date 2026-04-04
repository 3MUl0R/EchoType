use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::audio::{capture::CaptureSession, pipeline};
use crate::engine::manager::EngineManager;
use crate::engine::TranscribeRequest;

/// Maximum audio duration (in seconds) to re-decode for partial results.
const MAX_PARTIAL_AUDIO_SECS: f64 = 30.0;

/// Polling interval for streaming transcription.
const POLL_INTERVAL: Duration = Duration::from_millis(1500);

/// Minimum audio duration before first poll.
const MIN_AUDIO_FOR_POLL_SECS: f64 = 0.5;

/// Partial transcription result event for the frontend.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PartialResult {
    pub text: String,
    pub is_final: bool,
}

/// Handle to a running streaming session. Drop or call cancel() to stop.
pub struct StreamingHandle {
    cancel: Arc<AtomicBool>,
}

impl StreamingHandle {
    /// Cancel the streaming task.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Drop for StreamingHandle {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Start a streaming transcription polling task.
///
/// Periodically peeks at the capture buffer, runs the audio pipeline and
/// transcription, and emits partial results via `dictation:partial` events.
///
/// Returns a handle that cancels the task when dropped.
pub fn start_streaming(
    app: AppHandle,
    capture_session: Arc<Mutex<Option<CaptureSession>>>,
    engine_manager: EngineManager,
    suppression_level: crate::audio::denoise::SuppressionLevel,
) -> StreamingHandle {
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_clone = cancel.clone();

    tokio::spawn(async move {
        // Initial delay before first poll
        tokio::time::sleep(POLL_INTERVAL).await;

        let mut skip_next = false;

        loop {
            if cancel_clone.load(Ordering::Relaxed) {
                debug!("Streaming cancelled");
                break;
            }

            // CPU budget: if the previous poll exceeded the interval, skip this one
            if skip_next {
                skip_next = false;
                tokio::time::sleep(POLL_INTERVAL).await;
                continue;
            }

            // Peek at the capture buffer
            let buffer = {
                let guard = capture_session.lock().await;
                match guard.as_ref() {
                    Some(session) => session.peek_buffer(),
                    None => {
                        debug!("Capture session ended, stopping streaming");
                        break;
                    }
                }
            };

            // Skip if too short
            if buffer.duration_secs() < MIN_AUDIO_FOR_POLL_SECS {
                tokio::time::sleep(POLL_INTERVAL).await;
                continue;
            }

            let poll_start = Instant::now();

            // Cap audio to last MAX_PARTIAL_AUDIO_SECS for long recordings
            let poll_buffer = cap_audio_duration(buffer, MAX_PARTIAL_AUDIO_SECS);

            // Run pipeline (blocking)
            let level = suppression_level;
            let processed = tokio::task::spawn_blocking(move || {
                let config = pipeline::PipelineConfig {
                    suppression_level: level,
                };
                pipeline::process(&poll_buffer, &config)
            })
            .await;

            let processed = match processed {
                Ok(Ok(p)) => p,
                _ => {
                    tokio::time::sleep(POLL_INTERVAL).await;
                    continue;
                }
            };

            // Transcribe
            let request = TranscribeRequest {
                audio: processed.samples,
                sample_rate: processed.sample_rate,
                language: None,
                prompt: None,
            };

            match engine_manager.transcribe(request).await {
                Ok(transcription) => {
                    let elapsed = poll_start.elapsed();
                    debug!(
                        elapsed_ms = elapsed.as_millis(),
                        text_len = transcription.text.len(),
                        "Streaming partial result"
                    );

                    let partial = PartialResult {
                        text: transcription.text,
                        is_final: false,
                    };

                    if let Err(e) = app.emit("dictation:partial", &partial) {
                        warn!(%e, "Failed to emit partial result");
                    }

                    // CPU budget: if poll took longer than interval, skip next poll
                    if elapsed > POLL_INTERVAL {
                        warn!(
                            elapsed_ms = elapsed.as_millis(),
                            "Poll exceeded interval, skipping next"
                        );
                        skip_next = true;
                    }
                }
                Err(e) => {
                    debug!(%e, "Streaming transcription failed (non-fatal)");
                }
            }

            // Wait for next poll
            let elapsed = poll_start.elapsed();
            if elapsed < POLL_INTERVAL {
                tokio::time::sleep(POLL_INTERVAL - elapsed).await;
            }
        }

        info!("Streaming task ended");
    });

    StreamingHandle { cancel }
}

/// Cap audio to the last N seconds to limit CPU cost for long recordings.
fn cap_audio_duration(
    buffer: crate::audio::AudioBuffer,
    max_secs: f64,
) -> crate::audio::AudioBuffer {
    let max_samples = (max_secs * buffer.sample_rate as f64) as usize;
    if buffer.samples.len() <= max_samples {
        return buffer;
    }

    let start = buffer.samples.len() - max_samples;
    crate::audio::AudioBuffer {
        samples: buffer.samples[start..].to_vec(),
        sample_rate: buffer.sample_rate,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::AudioBuffer;

    #[test]
    fn cap_audio_within_limit() {
        let buf = AudioBuffer {
            samples: vec![0.0; 16000], // 1 second at 16kHz
            sample_rate: 16000,
        };
        let capped = cap_audio_duration(buf, 30.0);
        assert_eq!(capped.samples.len(), 16000);
    }

    #[test]
    fn cap_audio_exceeds_limit() {
        let buf = AudioBuffer {
            samples: vec![0.0; 960000], // 60 seconds at 16kHz
            sample_rate: 16000,
        };
        let capped = cap_audio_duration(buf, 30.0);
        assert_eq!(capped.samples.len(), 480000); // 30 seconds
    }

    #[test]
    fn partial_result_serializes() {
        let p = PartialResult {
            text: "hello".to_string(),
            is_final: false,
        };
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("hello"));
        assert!(json.contains("false"));
    }

    #[test]
    fn streaming_handle_cancel() {
        let cancel = Arc::new(AtomicBool::new(false));
        let handle = StreamingHandle {
            cancel: cancel.clone(),
        };
        assert!(!cancel.load(Ordering::Relaxed));
        handle.cancel();
        assert!(cancel.load(Ordering::Relaxed));
    }
}
