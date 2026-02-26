use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use tracing::{debug, info};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use super::{EngineError, Language, ModelInfo, SttEngine, TranscribeRequest, Transcription};

/// Whisper-based STT engine using whisper-rs (whisper.cpp bindings).
pub struct WhisperEngine {
    ctx: Arc<WhisperContext>,
    model_name: String,
    model_size: u64,
}

impl WhisperEngine {
    /// Load a Whisper model from the given GGML model file path.
    pub fn new(model_path: impl AsRef<Path>) -> Result<Self, EngineError> {
        let path = model_path.as_ref();

        if !path.exists() {
            return Err(EngineError::ModelNotFound(path.display().to_string()));
        }

        let model_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);

        let model_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        info!(
            model = %model_name,
            path = %path.display(),
            size_mb = model_size / 1_000_000,
            "Loading Whisper model"
        );

        let params = WhisperContextParameters::default();
        let ctx = WhisperContext::new_with_params(
            path.to_str()
                .ok_or_else(|| EngineError::ModelLoadFailed("Invalid path encoding".to_string()))?,
            params,
        )
        .map_err(|e| EngineError::ModelLoadFailed(format!("{e}")))?;

        info!(model = %model_name, "Whisper model loaded successfully");

        Ok(Self {
            ctx: Arc::new(ctx),
            model_name,
            model_size,
        })
    }
}

#[async_trait]
impl SttEngine for WhisperEngine {
    fn name(&self) -> &str {
        &self.model_name
    }

    async fn transcribe(&self, request: TranscribeRequest) -> Result<Transcription, EngineError> {
        if request.audio.is_empty() {
            return Err(EngineError::EmptyAudio);
        }

        let audio = request.audio;
        let language = request.language;
        let ctx = Arc::clone(&self.ctx);

        // Run blocking Whisper inference off the async runtime
        let result = tokio::task::spawn_blocking(move || {
            let start = Instant::now();

            let mut state = ctx
                .create_state()
                .map_err(|e| EngineError::TranscriptionFailed(format!("Create state: {e}")))?;

            let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
            params.set_print_progress(false);
            params.set_print_realtime(false);
            params.set_print_timestamps(false);

            if let Some(ref lang) = language {
                params.set_language(Some(&lang.0));
            } else {
                params.set_language(Some("en"));
            }

            state
                .full(params, &audio)
                .map_err(|e| EngineError::TranscriptionFailed(format!("Inference: {e}")))?;

            let num_segments = state.full_n_segments();

            let mut text = String::new();
            for i in 0..num_segments {
                if let Some(segment) = state.get_segment(i) {
                    if let Ok(seg_text) = segment.to_str() {
                        text.push_str(seg_text);
                    }
                }
            }

            let duration = start.elapsed();
            let text = text.trim().to_string();

            debug!(
                segments = num_segments,
                text_len = text.len(),
                duration_ms = duration.as_millis(),
                "Transcription complete"
            );

            Ok(Transcription {
                text,
                language: language.or_else(|| Some(Language("en".to_string()))),
                duration_ms: duration.as_millis() as u64,
            })
        })
        .await
        .map_err(|e| EngineError::TranscriptionFailed(format!("Task join: {e}")))?;

        result
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
            name: self.model_name.clone(),
            file_size_bytes: self.model_size,
        })
    }
}
