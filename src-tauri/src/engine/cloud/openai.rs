use std::time::Instant;

use async_trait::async_trait;
use tracing::debug;

use super::{build_client, encode_audio_wav, send_with_retry, CloudProvider, DEFAULT_TIMEOUT};
use crate::engine::{
    EngineError, Language, ModelInfo, SttEngine, TranscribeRequest, Transcription,
};

const ENDPOINT: &str = "https://api.openai.com/v1/audio/transcriptions";
const DEFAULT_MODEL: &str = "whisper-1";

/// OpenAI cloud STT engine.
pub struct OpenAiEngine {
    client: reqwest::Client,
    api_key: String,
    model: String,
}

impl OpenAiEngine {
    pub fn new(api_key: String) -> Result<Self, EngineError> {
        let client = build_client(DEFAULT_TIMEOUT)
            .map_err(|e| EngineError::ModelLoadFailed(format!("HTTP client: {e}")))?;
        Ok(Self {
            client,
            api_key,
            model: DEFAULT_MODEL.to_string(),
        })
    }

    pub fn with_model(mut self, model: String) -> Self {
        if !model.is_empty() {
            self.model = model;
        }
        self
    }
}

#[async_trait]
impl SttEngine for OpenAiEngine {
    fn name(&self) -> &str {
        "OpenAI"
    }

    async fn transcribe(&self, request: TranscribeRequest) -> Result<Transcription, EngineError> {
        if request.audio.is_empty() {
            return Err(EngineError::EmptyAudio);
        }

        let start = Instant::now();

        let wav_data = encode_audio_wav(&request.audio, request.sample_rate)
            .map_err(|e| EngineError::TranscriptionFailed(format!("WAV encode: {e}")))?;

        let language = request.language.clone();
        let client = self.client.clone();
        let api_key = self.api_key.clone();
        let model = self.model.clone();
        let lang_clone = language.clone();

        let body: Vec<u8> = send_with_retry(CloudProvider::OpenAi, || {
            let client = client.clone();
            let api_key = api_key.clone();
            let model = model.clone();
            let wav = wav_data.clone();
            let lang = lang_clone.clone();

            async move {
                let file_part = reqwest::multipart::Part::bytes(wav)
                    .file_name("audio.wav")
                    .mime_str("audio/wav")
                    .unwrap();

                let mut form = reqwest::multipart::Form::new()
                    .part("file", file_part)
                    .text("model", model);

                if let Some(ref lang) = lang {
                    form = form.text("language", lang.0.clone());
                }

                client
                    .post(ENDPOINT)
                    .header("Authorization", format!("Bearer {api_key}"))
                    .multipart(form)
                    .send()
                    .await
            }
        })
        .await
        .map_err(|e| EngineError::TranscriptionFailed(e.to_string()))?;

        let json: serde_json::Value = serde_json::from_slice(&body)
            .map_err(|e| EngineError::TranscriptionFailed(format!("JSON parse: {e}")))?;

        let text = json["text"].as_str().unwrap_or("").trim().to_string();

        let duration_ms = start.elapsed().as_millis() as u64;
        debug!(
            provider = "openai",
            duration_ms,
            text_len = text.len(),
            "OpenAI transcription complete"
        );

        Ok(Transcription {
            text,
            language: language.or_else(|| Some(Language("en".to_string()))),
            duration_ms,
        })
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
        ]
    }

    fn model_info(&self) -> Option<ModelInfo> {
        Some(ModelInfo {
            name: format!("OpenAI ({})", self.model),
            file_size_bytes: 0,
        })
    }
}
