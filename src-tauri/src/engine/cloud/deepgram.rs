use std::time::Instant;

use async_trait::async_trait;
use tracing::debug;

use super::{build_client, encode_audio_wav, send_with_retry, CloudProvider, DEFAULT_TIMEOUT};
use crate::engine::{
    EngineError, Language, ModelInfo, SttEngine, TranscribeRequest, Transcription,
};

const ENDPOINT: &str = "https://api.deepgram.com/v1/listen";
const DEFAULT_MODEL: &str = "nova-2";

/// Deepgram cloud STT engine.
///
/// Key differences from Groq/OpenAI:
/// - Auth: `Token` scheme (not `Bearer`)
/// - Body: raw WAV bytes (not multipart)
/// - Params: query string (not form fields)
/// - Response: nested JSON path `results.channels[0].alternatives[0].transcript`
pub struct DeepgramEngine {
    client: reqwest::Client,
    api_key: String,
    model: String,
    /// Cached formatted name for the engine (e.g. "deepgram/nova-2").
    display_name: String,
}

impl DeepgramEngine {
    pub fn new(api_key: String) -> Result<Self, EngineError> {
        let client = build_client(DEFAULT_TIMEOUT)
            .map_err(|e| EngineError::ModelLoadFailed(format!("HTTP client: {e}")))?;
        let model = DEFAULT_MODEL.to_string();
        let display_name = format!("deepgram/{model}");
        Ok(Self {
            client,
            api_key,
            model,
            display_name,
        })
    }

    pub fn with_model(mut self, model: String) -> Self {
        if !model.is_empty() {
            self.model = model;
        }
        self.display_name = format!("deepgram/{}", self.model);
        self
    }
}

#[async_trait]
impl SttEngine for DeepgramEngine {
    fn name(&self) -> &str {
        &self.display_name
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

        let body: Vec<u8> = send_with_retry(CloudProvider::Deepgram, || {
            let client = client.clone();
            let api_key = api_key.clone();
            let model = model.clone();
            let wav = wav_data.clone();
            let lang = lang_clone.clone();

            async move {
                // Build URL with query params — validate safe chars only
                fn is_safe_param(s: &str) -> bool {
                    s.chars()
                        .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
                }
                let model_safe = if is_safe_param(&model) {
                    &model
                } else {
                    "nova-2"
                };
                let mut url = format!("{ENDPOINT}?model={model_safe}");
                if let Some(ref lang) = lang {
                    if is_safe_param(&lang.0) {
                        url.push_str(&format!("&language={}", lang.0));
                    }
                }

                client
                    .post(&url)
                    .header("Authorization", format!("Token {api_key}"))
                    .header("Content-Type", "audio/wav")
                    .body(wav)
                    .send()
                    .await
            }
        })
        .await
        .map_err(|e| EngineError::TranscriptionFailed(e.to_string()))?;

        let json: serde_json::Value = serde_json::from_slice(&body)
            .map_err(|e| EngineError::TranscriptionFailed(format!("JSON parse: {e}")))?;

        // Deepgram response: results.channels[0].alternatives[0].transcript
        let text = json["results"]["channels"]
            .as_array()
            .and_then(|channels| channels.first())
            .and_then(|ch| ch["alternatives"].as_array())
            .and_then(|alts| alts.first())
            .and_then(|alt| alt["transcript"].as_str())
            .unwrap_or("")
            .trim()
            .to_string();

        let duration_ms = start.elapsed().as_millis() as u64;
        debug!(
            provider = "deepgram",
            duration_ms,
            text_len = text.len(),
            "Deepgram transcription complete"
        );

        Ok(Transcription {
            text,
            language: language.or_else(|| Some(Language("en".to_string()))),
            duration_ms,
            words: None,
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
            name: format!("Deepgram ({})", self.model),
            file_size_bytes: 0,
        })
    }
}
