pub mod cloud;
pub mod manager;
pub mod whisper;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt;
use tokio::sync::mpsc;

/// Errors from STT engine operations.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("model not found: {0}")]
    ModelNotFound(String),
    #[error("model load failed: {0}")]
    ModelLoadFailed(String),
    #[error("transcription failed: {0}")]
    TranscriptionFailed(String),
    #[error("no engine loaded")]
    NoEngineLoaded,
    #[error("empty audio buffer")]
    EmptyAudio,
}

/// Language identifier (ISO 639-1 code).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Language(pub String);

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Request to transcribe audio.
#[derive(Debug)]
pub struct TranscribeRequest {
    /// Audio samples in f32 format, 16kHz mono.
    pub audio: Vec<f32>,
    /// Sample rate of the audio (should be 16000).
    pub sample_rate: u32,
    /// Optional language hint.
    pub language: Option<Language>,
}

/// Result of a transcription.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcription {
    pub text: String,
    pub language: Option<Language>,
    pub duration_ms: u64,
}

/// Metadata about a loaded model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub file_size_bytes: u64,
}

/// Core STT engine abstraction (batch mode).
#[async_trait]
pub trait SttEngine: Send + Sync {
    fn name(&self) -> &str;
    async fn transcribe(&self, request: TranscribeRequest) -> Result<Transcription, EngineError>;
    fn supported_languages(&self) -> Vec<Language>;
    fn model_info(&self) -> Option<ModelInfo>;
}

/// A partial transcription result from a streaming engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingPartial {
    /// Incremental or accumulated transcript text.
    pub text: String,
    /// True when the engine considers this segment finalized.
    pub is_final: bool,
    /// Confidence score if available (0.0 - 1.0).
    pub confidence: Option<f32>,
}

/// Configuration for starting a streaming STT session.
#[derive(Debug, Clone)]
pub struct StreamingConfig {
    /// Audio sample rate being sent (e.g. 16000 for Deepgram, 24000 for OpenAI).
    pub sample_rate: u32,
    /// Number of audio channels (typically 1 = mono).
    pub channels: u16,
    /// Optional language hint.
    pub language: Option<Language>,
    /// How long the provider should wait (ms) after silence before finalizing
    /// a segment. Higher values = fewer sentence breaks on natural pauses.
    /// Maps to Deepgram `endpointing` and OpenAI `silence_duration_ms`.
    pub endpoint_ms: u64,
}

/// Handle to an active streaming STT session.
/// Send audio chunks and receive partial results through channels.
#[async_trait]
pub trait StreamingSttSession: Send + Sync {
    /// Send a chunk of PCM f32 audio samples to the engine.
    async fn send_audio(&self, samples: &[f32]) -> Result<(), EngineError>;

    /// Signal that no more audio will be sent. The engine should finalize.
    async fn close(&self) -> Result<(), EngineError>;
}

/// Streaming STT engine abstraction.
/// Engines that support real-time audio streaming implement this trait.
#[async_trait]
pub trait StreamingSttEngine: Send + Sync {
    fn name(&self) -> &str;

    /// Whether this engine supports true network streaming.
    fn supports_streaming(&self) -> bool;

    /// Start a streaming session. Returns a session handle and a receiver
    /// for partial transcription results.
    async fn start_stream(
        &self,
        config: StreamingConfig,
    ) -> Result<(Box<dyn StreamingSttSession>, mpsc::UnboundedReceiver<StreamingPartial>), EngineError>;

    fn supported_languages(&self) -> Vec<Language>;
    fn model_info(&self) -> Option<ModelInfo>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_display() {
        let lang = Language("en".to_string());
        assert_eq!(format!("{lang}"), "en");
    }

    #[test]
    fn transcription_serializes() {
        let t = Transcription {
            text: "hello world".to_string(),
            language: Some(Language("en".to_string())),
            duration_ms: 1234,
        };
        let json = serde_json::to_string(&t).unwrap();
        assert!(json.contains("hello world"));
    }
}
