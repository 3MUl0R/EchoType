use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD;
use base64::Engine as Base64Engine;
use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::tungstenite::http::Request;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream};
use tracing::{debug, error, info, warn};

use crate::engine::{
    EngineError, Language, ModelInfo, StreamingConfig, StreamingPartial, StreamingSttEngine,
    StreamingSttSession,
};

const REALTIME_URL: &str = "wss://api.openai.com/v1/realtime?intent=transcription";
const DEFAULT_MODEL: &str = "gpt-4o-transcribe";
const TARGET_SAMPLE_RATE: u32 = 24_000;

type WsStream =
    tokio_tungstenite::WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

/// OpenAI Realtime API streaming STT engine.
///
/// Uses the `wss://api.openai.com/v1/realtime` WebSocket endpoint with
/// server-side VAD and the `gpt-4o-transcribe` model to produce low-latency
/// transcription deltas while audio is still being recorded.
pub struct OpenAiStreamingEngine {
    api_key: String,
    model: String,
    display_name: String,
}

impl OpenAiStreamingEngine {
    pub fn new(api_key: String) -> Self {
        let model = DEFAULT_MODEL.to_string();
        let display_name = format!("openai-realtime/{model}");
        Self {
            api_key,
            model,
            display_name,
        }
    }

    pub fn with_model(mut self, model: String) -> Self {
        if !model.is_empty() {
            self.model = model;
        }
        self.display_name = format!("openai-realtime/{}", self.model);
        self
    }
}

#[async_trait]
impl StreamingSttEngine for OpenAiStreamingEngine {
    fn name(&self) -> &str {
        &self.display_name
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    async fn start_stream(
        &self,
        config: StreamingConfig,
    ) -> Result<
        (
            Box<dyn StreamingSttSession>,
            mpsc::UnboundedReceiver<StreamingPartial>,
        ),
        EngineError,
    > {
        // 1. Build the HTTP upgrade request with auth headers.
        let request = Request::builder()
            .uri(REALTIME_URL)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("OpenAI-Beta", "realtime=v1")
            // Required by tungstenite for the WS handshake:
            .header("Host", "api.openai.com")
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header(
                "Sec-WebSocket-Key",
                tokio_tungstenite::tungstenite::handshake::client::generate_key(),
            )
            .body(())
            .map_err(|e| {
                EngineError::TranscriptionFailed(format!("Failed to build WS request: {e}"))
            })?;

        // 2. Connect.
        let (ws_stream, _response) = connect_async(request).await.map_err(|e| {
            EngineError::TranscriptionFailed(format!("WebSocket connection failed: {e}"))
        })?;

        info!(
            provider = "openai-realtime",
            model = %self.model,
            "Connected to OpenAI Realtime API"
        );

        let (write, read) = ws_stream.split();
        let write = Arc::new(Mutex::new(write));

        // 3. Send session.update configuration.
        let session_update = serde_json::json!({
            "type": "session.update",
            "session": {
                "input_audio_format": "pcm16",
                "input_audio_transcription": {
                    "model": self.model,
                    "language": config.language.as_ref().map(|l| &l.0),
                },
                "turn_detection": {
                    "type": "server_vad",
                    "threshold": 0.5,
                    "silence_duration_ms": 500
                },
                "input_audio_noise_reduction": {
                    "type": "near_field"
                }
            }
        });

        {
            let mut ws = write.lock().await;
            ws.send(Message::Text(session_update.to_string().into()))
                .await
                .map_err(|e| {
                    EngineError::TranscriptionFailed(format!("Failed to send session.update: {e}"))
                })?;
        }

        debug!("Sent session.update to OpenAI Realtime API");

        // 4. Spawn reader task.
        let (tx, rx) = mpsc::unbounded_channel::<StreamingPartial>();
        let closed = Arc::new(AtomicBool::new(false));
        let closed_reader = closed.clone();

        tokio::spawn(async move {
            reader_task(read, tx, closed_reader).await;
        });

        // 5. Return session handle.
        let session = OpenAiStreamingSession {
            write,
            closed,
            input_sample_rate: config.sample_rate,
        };

        Ok((Box::new(session), rx))
    }

    fn supported_languages(&self) -> Vec<Language> {
        vec![
            Language("en".to_string()),
            Language("es".to_string()),
            Language("fr".to_string()),
            Language("de".to_string()),
            Language("it".to_string()),
            Language("pt".to_string()),
            Language("nl".to_string()),
            Language("ja".to_string()),
            Language("ko".to_string()),
            Language("zh".to_string()),
            Language("ru".to_string()),
            Language("ar".to_string()),
            Language("hi".to_string()),
            Language("sv".to_string()),
            Language("pl".to_string()),
            Language("uk".to_string()),
            Language("tr".to_string()),
        ]
    }

    fn model_info(&self) -> Option<ModelInfo> {
        Some(ModelInfo {
            name: format!("OpenAI Realtime ({})", self.model),
            file_size_bytes: 0,
        })
    }
}

// ---------------------------------------------------------------------------
// Session
// ---------------------------------------------------------------------------

/// Active streaming session connected to the OpenAI Realtime WebSocket.
struct OpenAiStreamingSession {
    write: Arc<Mutex<SplitSink<WsStream, Message>>>,
    closed: Arc<AtomicBool>,
    input_sample_rate: u32,
}

#[async_trait]
impl StreamingSttSession for OpenAiStreamingSession {
    async fn send_audio(&self, samples: &[f32]) -> Result<(), EngineError> {
        if self.closed.load(Ordering::Relaxed) {
            return Err(EngineError::TranscriptionFailed(
                "Session already closed".to_string(),
            ));
        }

        if samples.is_empty() {
            return Ok(());
        }

        // Resample from input rate to 24 kHz if needed.
        let resampled = if self.input_sample_rate != TARGET_SAMPLE_RATE {
            resample_linear(samples, self.input_sample_rate, TARGET_SAMPLE_RATE)
        } else {
            samples.to_vec()
        };

        // Convert to PCM16 little-endian bytes then base64.
        let pcm_bytes = f32_to_pcm16_bytes(&resampled);
        let encoded = STANDARD.encode(&pcm_bytes);

        let msg = serde_json::json!({
            "type": "input_audio_buffer.append",
            "audio": encoded,
        });

        let mut ws = self.write.lock().await;
        ws.send(Message::Text(msg.to_string().into()))
            .await
            .map_err(|e| {
                EngineError::TranscriptionFailed(format!("Failed to send audio: {e}"))
            })?;

        Ok(())
    }

    async fn close(&self) -> Result<(), EngineError> {
        if self.closed.swap(true, Ordering::Relaxed) {
            // Already closed.
            return Ok(());
        }

        let commit_msg = serde_json::json!({
            "type": "input_audio_buffer.commit"
        });

        let mut ws = self.write.lock().await;

        // Send commit to finalize any pending audio.
        if let Err(e) = ws.send(Message::Text(commit_msg.to_string().into())).await {
            warn!("Failed to send input_audio_buffer.commit: {e}");
        }

        // Drop the lock so the reader task can still receive the completion
        // event. We give the server time to finalize and send the completed
        // transcript before tearing down the connection.
        drop(ws);

        // Wait for the server to process the commit and send the completion
        // event. The reader_task will receive it and forward via the channel.
        // 2 seconds is generous — typical OpenAI response is <500ms.
        tokio::time::sleep(std::time::Duration::from_millis(2000)).await;

        // Now close the WebSocket.
        let mut ws = self.write.lock().await;
        if let Err(e) = ws.close().await {
            debug!("WebSocket close frame error (non-fatal): {e}");
        }

        info!("OpenAI Realtime streaming session closed");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Reader task
// ---------------------------------------------------------------------------

async fn reader_task(
    mut read: futures_util::stream::SplitStream<WsStream>,
    tx: mpsc::UnboundedSender<StreamingPartial>,
    closed: Arc<AtomicBool>,
) {
    while let Some(result) = read.next().await {
        let msg = match result {
            Ok(m) => m,
            Err(e) => {
                if !closed.load(Ordering::Relaxed) {
                    error!("OpenAI Realtime WS read error: {e}");
                }
                break;
            }
        };

        let text = match msg {
            Message::Text(t) => t,
            Message::Close(_) => {
                debug!("OpenAI Realtime WS closed by server");
                break;
            }
            _ => continue,
        };

        let json: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                warn!("Failed to parse OpenAI Realtime event: {e}");
                continue;
            }
        };

        let event_type = json["type"].as_str().unwrap_or("");

        match event_type {
            "conversation.item.input_audio_transcription.delta" => {
                if let Some(delta) = json["delta"].as_str() {
                    if !delta.is_empty() {
                        let partial = StreamingPartial {
                            text: delta.to_string(),
                            is_final: false,
                            confidence: None,
                        };
                        if tx.send(partial).is_err() {
                            debug!("Partial receiver dropped, stopping reader");
                            break;
                        }
                    }
                }
            }
            "conversation.item.input_audio_transcription.completed" => {
                let transcript = json["transcript"].as_str().unwrap_or("").to_string();
                let partial = StreamingPartial {
                    text: transcript,
                    is_final: true,
                    confidence: None,
                };
                if tx.send(partial).is_err() {
                    debug!("Partial receiver dropped, stopping reader");
                    break;
                }
            }
            "error" => {
                let err_msg = json["error"]["message"]
                    .as_str()
                    .unwrap_or("unknown error");
                error!(
                    provider = "openai-realtime",
                    error = err_msg,
                    "OpenAI Realtime API error"
                );
                // Send the error as a final partial so the caller can see it.
                let _ = tx.send(StreamingPartial {
                    text: String::new(),
                    is_final: true,
                    confidence: None,
                });
                break;
            }
            "session.created" | "session.updated" => {
                debug!(event = event_type, "OpenAI Realtime session event");
            }
            _ => {
                debug!(event = event_type, "Unhandled OpenAI Realtime event");
            }
        }
    }

    debug!("OpenAI Realtime reader task exiting");
}

// ---------------------------------------------------------------------------
// Audio helpers
// ---------------------------------------------------------------------------

/// Resample audio using linear interpolation.
///
/// Converts from `from_rate` to `to_rate`. This is adequate for the 16 kHz to
/// 24 kHz up-conversion needed by the OpenAI Realtime API. For more demanding
/// resampling needs, consider the `rubato` crate already in this project.
pub fn resample_linear(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate || samples.is_empty() {
        return samples.to_vec();
    }

    let ratio = from_rate as f64 / to_rate as f64;
    let output_len = ((samples.len() as f64) / ratio).ceil() as usize;
    let mut output = Vec::with_capacity(output_len);

    for i in 0..output_len {
        let src_pos = i as f64 * ratio;
        let idx = src_pos as usize;
        let frac = (src_pos - idx as f64) as f32;

        let sample = if idx + 1 < samples.len() {
            samples[idx] * (1.0 - frac) + samples[idx + 1] * frac
        } else {
            // Clamp to last sample.
            samples[samples.len() - 1]
        };

        output.push(sample);
    }

    output
}

/// Convert f32 audio samples (-1.0..1.0) to PCM16 little-endian bytes.
pub fn f32_to_pcm16_bytes(samples: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        // Clamp to [-1.0, 1.0] then scale to i16 range.
        let clamped = s.clamp(-1.0, 1.0);
        let scaled = if clamped >= 0.0 {
            (clamped * i16::MAX as f32) as i16
        } else {
            (clamped * -(i16::MIN as f32)) as i16
        };
        bytes.extend_from_slice(&scaled.to_le_bytes());
    }
    bytes
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_name_is_correct() {
        let engine = OpenAiStreamingEngine::new("test-key".to_string());
        assert_eq!(engine.name(), "openai-realtime/gpt-4o-transcribe");
    }

    #[test]
    fn engine_name_with_custom_model() {
        let engine =
            OpenAiStreamingEngine::new("test-key".to_string()).with_model("gpt-4o-mini-transcribe".to_string());
        assert_eq!(engine.name(), "openai-realtime/gpt-4o-mini-transcribe");
    }

    #[test]
    fn engine_supports_streaming() {
        let engine = OpenAiStreamingEngine::new("test-key".to_string());
        assert!(engine.supports_streaming());
    }

    #[test]
    fn resample_16k_to_24k_correct_length() {
        // 16000 samples at 16 kHz = 1 second.
        // At 24 kHz, 1 second = 24000 samples.
        let input = vec![0.0f32; 16_000];
        let output = resample_linear(&input, 16_000, 24_000);
        assert_eq!(output.len(), 24_000);
    }

    #[test]
    fn resample_identity() {
        let input: Vec<f32> = (0..100).map(|i| (i as f32) / 100.0).collect();
        let output = resample_linear(&input, 16_000, 16_000);
        assert_eq!(output.len(), input.len());
        for (a, b) in input.iter().zip(output.iter()) {
            assert!((a - b).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn resample_empty_input() {
        let output = resample_linear(&[], 16_000, 24_000);
        assert!(output.is_empty());
    }

    #[test]
    fn resample_preserves_range() {
        // A sine-like signal should stay in [-1, 1] after resampling.
        let input: Vec<f32> = (0..1600)
            .map(|i| (i as f32 * std::f32::consts::TAU / 160.0).sin())
            .collect();
        let output = resample_linear(&input, 16_000, 24_000);
        for &s in &output {
            assert!(s >= -1.0 && s <= 1.0, "Sample {s} out of range");
        }
    }

    #[test]
    fn f32_to_pcm16_bytes_correct_length() {
        let samples = vec![0.0f32; 100];
        let bytes = f32_to_pcm16_bytes(&samples);
        assert_eq!(bytes.len(), 200); // 2 bytes per sample
    }

    #[test]
    fn f32_to_pcm16_bytes_zero() {
        let bytes = f32_to_pcm16_bytes(&[0.0]);
        assert_eq!(bytes, vec![0, 0]);
    }

    #[test]
    fn f32_to_pcm16_bytes_max_positive() {
        let bytes = f32_to_pcm16_bytes(&[1.0]);
        let val = i16::from_le_bytes([bytes[0], bytes[1]]);
        assert_eq!(val, i16::MAX);
    }

    #[test]
    fn f32_to_pcm16_bytes_max_negative() {
        let bytes = f32_to_pcm16_bytes(&[-1.0]);
        let val = i16::from_le_bytes([bytes[0], bytes[1]]);
        assert_eq!(val, i16::MIN);
    }

    #[test]
    fn f32_to_pcm16_bytes_clamps_overflow() {
        // Values beyond [-1, 1] should be clamped.
        let bytes = f32_to_pcm16_bytes(&[2.0, -2.0]);
        let v1 = i16::from_le_bytes([bytes[0], bytes[1]]);
        let v2 = i16::from_le_bytes([bytes[2], bytes[3]]);
        assert_eq!(v1, i16::MAX);
        assert_eq!(v2, i16::MIN);
    }

    #[test]
    fn model_info_present() {
        let engine = OpenAiStreamingEngine::new("key".to_string());
        let info = engine.model_info().unwrap();
        assert!(info.name.contains("gpt-4o-transcribe"));
    }
}
