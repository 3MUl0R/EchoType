use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tracing::{debug, error, info, warn};

use crate::engine::{
    EngineError, Language, ModelInfo, StreamingConfig, StreamingPartial, StreamingSttEngine,
    StreamingSttSession,
};

const WS_ENDPOINT: &str = "wss://api.deepgram.com/v1/listen";
const DEFAULT_MODEL: &str = "nova-3";
const KEEPALIVE_INTERVAL_SECS: u64 = 5;

type WsSink = SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>;

/// Deepgram streaming STT engine using their WebSocket API.
///
/// Sends raw PCM16 audio frames over a persistent WebSocket connection and
/// receives partial/final transcription results in real time.
pub struct DeepgramStreamingEngine {
    api_key: String,
    model: String,
    display_name: String,
}

impl DeepgramStreamingEngine {
    pub fn new(api_key: String) -> Self {
        let model = DEFAULT_MODEL.to_string();
        let display_name = format!("deepgram-streaming/{model}");
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
        self.display_name = format!("deepgram-streaming/{}", self.model);
        self
    }

    /// Build the WebSocket URL with Deepgram query parameters.
    fn build_ws_url(&self, config: &StreamingConfig) -> String {
        let mut url = format!(
            "{WS_ENDPOINT}?model={}&encoding=linear16&sample_rate={}&channels={}\
             &interim_results=true&punctuate=true&smart_format=true\
             &endpointing={}",
            self.model, config.sample_rate, config.channels, config.endpoint_ms,
        );
        if let Some(ref lang) = config.language {
            // Only append safe language codes
            if lang.0.chars().all(|c| c.is_alphanumeric() || c == '-') {
                url.push_str(&format!("&language={}", lang.0));
            }
        }
        url
    }
}

#[async_trait]
impl StreamingSttEngine for DeepgramStreamingEngine {
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
        let url = self.build_ws_url(&config);

        // Build HTTP request with auth header for the WebSocket upgrade.
        let mut request = url
            .into_client_request()
            .map_err(|e| EngineError::TranscriptionFailed(format!("invalid WS URL: {e}")))?;
        request.headers_mut().insert(
            "Authorization",
            format!("Token {}", self.api_key)
                .parse()
                .map_err(|e| EngineError::TranscriptionFailed(format!("bad auth header: {e}")))?,
        );

        info!(engine = %self.display_name, "connecting to Deepgram WebSocket");

        let (ws_stream, _response) = tokio_tungstenite::connect_async(request)
            .await
            .map_err(|e| {
                EngineError::TranscriptionFailed(format!("WebSocket connect failed: {e}"))
            })?;

        let (write, read) = ws_stream.split();

        let (tx, rx) = mpsc::unbounded_channel::<StreamingPartial>();
        let closed = Arc::new(AtomicBool::new(false));
        let sink = Arc::new(Mutex::new(write));

        // --- Reader task: parse incoming JSON frames into StreamingPartial ---
        // The reader runs until the server closes the connection (after processing
        // CloseStream) or the partial channel is dropped.  It does NOT check the
        // `closed` flag in its main loop — that flag is set at the *start* of
        // close(), before CloseStream is even sent, so checking it would cause
        // the reader to exit before collecting Deepgram's final results.
        let closed_reader = Arc::clone(&closed);
        let engine_name = self.display_name.clone();
        tokio::spawn(async move {
            let mut read = read;
            while let Some(msg_result) = read.next().await {
                match msg_result {
                    Ok(Message::Text(text)) => {
                        match parse_deepgram_message(&text) {
                            Ok(Some(partial)) => {
                                if tx.send(partial).is_err() {
                                    debug!(engine = %engine_name, "partial receiver dropped, stopping reader");
                                    break;
                                }
                            }
                            Ok(None) => {
                                // Non-results message (metadata, etc.) — ignore.
                            }
                            Err(e) => {
                                warn!(engine = %engine_name, error = %e, "failed to parse Deepgram message");
                            }
                        }
                    }
                    Ok(Message::Close(_)) => {
                        info!(engine = %engine_name, "Deepgram WebSocket closed by server");
                        break;
                    }
                    Ok(_) => {
                        // Binary/ping/pong — ignore.
                    }
                    Err(e) => {
                        // Only log as error if we didn't intentionally close.
                        if !closed_reader.load(Ordering::Relaxed) {
                            error!(engine = %engine_name, error = %e, "WebSocket read error");
                        }
                        break;
                    }
                }
            }
            debug!(engine = %engine_name, "Deepgram reader task exiting");
        });

        // --- Keep-alive task: send KeepAlive every 5 seconds ---
        let keepalive_sink = Arc::clone(&sink);
        let keepalive_closed = Arc::clone(&closed);
        let keepalive_name = self.display_name.clone();
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(tokio::time::Duration::from_secs(KEEPALIVE_INTERVAL_SECS));
            // Skip the first immediate tick.
            interval.tick().await;

            loop {
                interval.tick().await;
                if keepalive_closed.load(Ordering::Relaxed) {
                    break;
                }
                let msg = Message::Text(r#"{"type":"KeepAlive"}"#.to_string().into());
                let mut guard = keepalive_sink.lock().await;
                if let Err(e) = guard.send(msg).await {
                    if !keepalive_closed.load(Ordering::Relaxed) {
                        warn!(engine = %keepalive_name, error = %e, "failed to send keep-alive");
                    }
                    break;
                }
            }
            debug!(engine = %keepalive_name, "Deepgram keep-alive task exiting");
        });

        let session = DeepgramStreamingSession {
            sink,
            closed,
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
            Language("hi".to_string()),
            Language("ru".to_string()),
            Language("sv".to_string()),
            Language("da".to_string()),
            Language("no".to_string()),
            Language("pl".to_string()),
            Language("uk".to_string()),
        ]
    }

    fn model_info(&self) -> Option<ModelInfo> {
        Some(ModelInfo {
            name: format!("Deepgram Streaming ({})", self.model),
            file_size_bytes: 0,
        })
    }
}

/// Active Deepgram streaming session.
///
/// Holds the write half of the WebSocket behind an `Arc<Mutex<...>>` so it can
/// be shared with the keep-alive task and still be `Send + Sync`.
pub struct DeepgramStreamingSession {
    sink: Arc<Mutex<WsSink>>,
    closed: Arc<AtomicBool>,
}

#[async_trait]
impl StreamingSttSession for DeepgramStreamingSession {
    async fn send_audio(&self, samples: &[f32]) -> Result<(), EngineError> {
        if self.closed.load(Ordering::Relaxed) {
            return Err(EngineError::TranscriptionFailed(
                "session already closed".to_string(),
            ));
        }

        let pcm_bytes = f32_to_pcm16_le(samples);
        let msg = Message::Binary(pcm_bytes.into());

        let mut guard = self.sink.lock().await;
        guard.send(msg).await.map_err(|e| {
            EngineError::TranscriptionFailed(format!("failed to send audio frame: {e}"))
        })?;

        Ok(())
    }

    async fn close(&self) -> Result<(), EngineError> {
        if self.closed.swap(true, Ordering::Relaxed) {
            // Already closed — idempotent.
            return Ok(());
        }

        let close_msg = Message::Text(r#"{"type":"CloseStream"}"#.to_string().into());
        let mut guard = self.sink.lock().await;

        // Send CloseStream to let Deepgram finalize any pending transcription.
        // Do NOT send a WebSocket close frame here — Deepgram needs to process
        // the remaining audio and return final results before the connection
        // is terminated. The server will close the connection after it's done.
        if let Err(e) = guard.send(close_msg).await {
            warn!(error = %e, "failed to send CloseStream message");
        }

        info!("Deepgram streaming session: CloseStream sent, waiting for server to finalize");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Audio helpers
// ---------------------------------------------------------------------------

/// Convert f32 audio samples (range -1.0..1.0) to 16-bit signed PCM, little-endian bytes.
fn f32_to_pcm16_le(samples: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for &sample in samples {
        // Clamp to [-1.0, 1.0] then scale to i16 range.
        let clamped = sample.clamp(-1.0, 1.0);
        let scaled = (clamped * i16::MAX as f32) as i16;
        bytes.extend_from_slice(&scaled.to_le_bytes());
    }
    bytes
}

// ---------------------------------------------------------------------------
// Deepgram JSON parsing
// ---------------------------------------------------------------------------

/// Parse a text frame from the Deepgram WebSocket.
///
/// Returns `Ok(Some(partial))` for Results messages with a transcript,
/// `Ok(None)` for non-results messages (Metadata, etc.), or `Err` on
/// malformed JSON.
fn parse_deepgram_message(text: &str) -> Result<Option<StreamingPartial>, String> {
    let json: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("JSON parse error: {e}"))?;

    let msg_type = json
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    if msg_type != "Results" {
        return Ok(None);
    }

    let transcript = json
        .pointer("/channel/alternatives/0/transcript")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let confidence = json
        .pointer("/channel/alternatives/0/confidence")
        .and_then(|v| v.as_f64())
        .map(|c| c as f32);

    // `is_final` means the server won't revise this segment.
    // `speech_final` means the end of an utterance (endpoint detected).
    // We expose `speech_final` as the consumer-facing `is_final` since that
    // typically gates text insertion in a dictation context.
    let speech_final = json
        .get("speech_final")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let is_final_segment = json
        .get("is_final")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // Only emit partials that carry actual content or represent a final boundary.
    if transcript.is_empty() && !speech_final {
        return Ok(None);
    }

    Ok(Some(StreamingPartial {
        text: transcript,
        is_final: speech_final || is_final_segment,
        confidence,
    }))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_name_is_correct() {
        let engine = DeepgramStreamingEngine::new("test-key".to_string());
        assert_eq!(engine.name(), "deepgram-streaming/nova-3");
    }

    #[test]
    fn engine_name_with_custom_model() {
        let engine =
            DeepgramStreamingEngine::new("test-key".to_string()).with_model("nova-2".to_string());
        assert_eq!(engine.name(), "deepgram-streaming/nova-2");
    }

    #[test]
    fn with_model_ignores_empty_string() {
        let engine =
            DeepgramStreamingEngine::new("test-key".to_string()).with_model(String::new());
        assert_eq!(engine.name(), "deepgram-streaming/nova-3");
    }

    #[test]
    fn supports_streaming_returns_true() {
        let engine = DeepgramStreamingEngine::new("key".to_string());
        assert!(engine.supports_streaming());
    }

    #[test]
    fn f32_to_pcm16_silence() {
        let samples = vec![0.0f32; 4];
        let bytes = f32_to_pcm16_le(&samples);
        assert_eq!(bytes.len(), 8);
        assert!(bytes.iter().all(|&b| b == 0));
    }

    #[test]
    fn f32_to_pcm16_positive_peak() {
        let samples = vec![1.0f32];
        let bytes = f32_to_pcm16_le(&samples);
        let value = i16::from_le_bytes([bytes[0], bytes[1]]);
        assert_eq!(value, i16::MAX);
    }

    #[test]
    fn f32_to_pcm16_negative_peak() {
        let samples = vec![-1.0f32];
        let bytes = f32_to_pcm16_le(&samples);
        let value = i16::from_le_bytes([bytes[0], bytes[1]]);
        // -1.0 * 32767 = -32767, which is close to i16::MIN (-32768)
        assert_eq!(value, -i16::MAX);
    }

    #[test]
    fn f32_to_pcm16_clamps_overflow() {
        let samples = vec![2.5f32, -3.0f32];
        let bytes = f32_to_pcm16_le(&samples);
        let v0 = i16::from_le_bytes([bytes[0], bytes[1]]);
        let v1 = i16::from_le_bytes([bytes[2], bytes[3]]);
        assert_eq!(v0, i16::MAX);
        assert_eq!(v1, -i16::MAX);
    }

    #[test]
    fn f32_to_pcm16_half_amplitude() {
        let samples = vec![0.5f32];
        let bytes = f32_to_pcm16_le(&samples);
        let value = i16::from_le_bytes([bytes[0], bytes[1]]);
        // 0.5 * 32767 = 16383.5, truncated to 16383
        assert_eq!(value, 16383);
    }

    #[test]
    fn parse_results_message() {
        let json = r#"{
            "type": "Results",
            "channel": {
                "alternatives": [{"transcript": "hello world", "confidence": 0.98}]
            },
            "is_final": true,
            "speech_final": false
        }"#;
        let result = parse_deepgram_message(json).unwrap().unwrap();
        assert_eq!(result.text, "hello world");
        assert!(result.is_final); // is_final || speech_final
        assert!((result.confidence.unwrap() - 0.98).abs() < 0.001);
    }

    #[test]
    fn parse_interim_results_message() {
        let json = r#"{
            "type": "Results",
            "channel": {
                "alternatives": [{"transcript": "hel", "confidence": 0.5}]
            },
            "is_final": false,
            "speech_final": false
        }"#;
        let result = parse_deepgram_message(json).unwrap().unwrap();
        assert_eq!(result.text, "hel");
        assert!(!result.is_final);
    }

    #[test]
    fn parse_speech_final_message() {
        let json = r#"{
            "type": "Results",
            "channel": {
                "alternatives": [{"transcript": "done", "confidence": 0.99}]
            },
            "is_final": false,
            "speech_final": true
        }"#;
        let result = parse_deepgram_message(json).unwrap().unwrap();
        assert!(result.is_final);
    }

    #[test]
    fn parse_empty_transcript_skipped() {
        let json = r#"{
            "type": "Results",
            "channel": {
                "alternatives": [{"transcript": "", "confidence": 0.0}]
            },
            "is_final": false,
            "speech_final": false
        }"#;
        let result = parse_deepgram_message(json).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn parse_metadata_message_ignored() {
        let json = r#"{"type": "Metadata", "request_id": "abc123"}"#;
        let result = parse_deepgram_message(json).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn parse_invalid_json_returns_error() {
        let result = parse_deepgram_message("not json");
        assert!(result.is_err());
    }

    #[test]
    fn build_ws_url_default() {
        let engine = DeepgramStreamingEngine::new("key".to_string());
        let config = StreamingConfig {
            sample_rate: 16000,
            channels: 1,
            language: None,
            endpoint_ms: 1500,
        };
        let url = engine.build_ws_url(&config);
        assert!(url.starts_with("wss://api.deepgram.com/v1/listen?"));
        assert!(url.contains("model=nova-3"));
        assert!(url.contains("encoding=linear16"));
        assert!(url.contains("sample_rate=16000"));
        assert!(url.contains("channels=1"));
        assert!(url.contains("interim_results=true"));
        assert!(url.contains("punctuate=true"));
        assert!(url.contains("smart_format=true"));
        assert!(url.contains("endpointing=1500"));
        assert!(!url.contains("language="));
    }

    #[test]
    fn build_ws_url_with_language() {
        let engine = DeepgramStreamingEngine::new("key".to_string());
        let config = StreamingConfig {
            sample_rate: 16000,
            channels: 1,
            language: Some(Language("es".to_string())),
            endpoint_ms: 1500,
        };
        let url = engine.build_ws_url(&config);
        assert!(url.contains("language=es"));
    }

    #[test]
    fn model_info_present() {
        let engine = DeepgramStreamingEngine::new("key".to_string());
        let info = engine.model_info().unwrap();
        assert!(info.name.contains("nova-3"));
        assert_eq!(info.file_size_bytes, 0);
    }

    #[test]
    fn supported_languages_not_empty() {
        let engine = DeepgramStreamingEngine::new("key".to_string());
        let langs = engine.supported_languages();
        assert!(!langs.is_empty());
        assert!(langs.iter().any(|l| l.0 == "en"));
    }
}
