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
    /// Cached formatted name for the engine (e.g. "openai/whisper-1").
    display_name: String,
}

impl OpenAiEngine {
    pub fn new(api_key: String) -> Result<Self, EngineError> {
        let client = build_client(DEFAULT_TIMEOUT)
            .map_err(|e| EngineError::ModelLoadFailed(format!("HTTP client: {e}")))?;
        let model = DEFAULT_MODEL.to_string();
        let display_name = format!("openai/{model}");
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
        self.display_name = format!("openai/{}", self.model);
        self
    }
}

#[async_trait]
impl SttEngine for OpenAiEngine {
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
        let prompt = request.prompt.clone();
        let client = self.client.clone();
        let api_key = self.api_key.clone();
        let model = self.model.clone();
        let lang_clone = language.clone();
        let prompt_clone = prompt.clone();

        let body: Vec<u8> = send_with_retry(CloudProvider::OpenAi, || {
            let client = client.clone();
            let api_key = api_key.clone();
            let model = model.clone();
            let wav = wav_data.clone();
            let lang = lang_clone.clone();
            let prompt = prompt_clone.clone();

            async move {
                let file_part = reqwest::multipart::Part::bytes(wav)
                    .file_name("audio.wav")
                    .mime_str("audio/wav")
                    .unwrap();

                let mut form = reqwest::multipart::Form::new()
                    .part("file", file_part)
                    .text("model", model)
                    .text("response_format", "verbose_json")
                    .text("timestamp_granularities[]", "word");

                if let Some(ref lang) = lang {
                    form = form.text("language", lang.0.clone());
                }
                if let Some(ref prompt) = prompt {
                    form = form.text("prompt", prompt.clone());
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

        // Parse word-level timestamps from verbose_json response.
        let words = json["words"].as_array().map(|arr| {
            arr.iter()
                .filter_map(|w| {
                    Some(crate::engine::WordTimestamp {
                        word: w["word"].as_str()?.to_string(),
                        start: w["start"].as_f64()?,
                        end: w["end"].as_f64()?,
                        probability: w["probability"].as_f64().map(|p| p as f32),
                    })
                })
                .collect()
        });

        let duration_ms = start.elapsed().as_millis() as u64;
        debug!(
            provider = "openai",
            duration_ms,
            text_len = text.len(),
            word_count = words.as_ref().map(|w: &Vec<_>| w.len()).unwrap_or(0),
            "OpenAI transcription complete"
        );

        Ok(Transcription {
            text,
            language: language.or_else(|| Some(Language("en".to_string()))),
            duration_ms,
            words,
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
