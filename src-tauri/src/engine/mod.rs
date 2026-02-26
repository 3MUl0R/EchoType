pub mod manager;
pub mod whisper;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt;

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

/// Core STT engine abstraction.
#[async_trait]
pub trait SttEngine: Send + Sync {
    fn name(&self) -> &str;
    async fn transcribe(&self, request: TranscribeRequest) -> Result<Transcription, EngineError>;
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
