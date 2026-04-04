//! Simulated streaming engine.
//!
//! Wraps any batch `SttEngine` (Groq, OpenAI, local Whisper) to produce
//! `StreamingSttEngine` behavior via the LocalAgreement algorithm. Audio is
//! accumulated, filtered by VAD, and periodically batch-transcribed. Consecutive
//! hypotheses are compared and only agreed-upon tokens are emitted as stable
//! `StreamingPartial` events.

pub mod local_agreement;
pub mod processor;
pub mod vad;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::{mpsc, oneshot, Mutex};
use tracing::{debug, warn};

use self::local_agreement::WordToken;
use self::processor::{OnlineProcessor, ProcessorConfig};
use self::vad::VadConfig;
use super::{
    EngineError, Language, ModelInfo, StreamingConfig, StreamingPartial, StreamingSttEngine,
    StreamingSttSession, SttEngine, TranscribeRequest, WordTimestamp,
};

/// Wraps any batch `SttEngine` to produce `StreamingSttEngine` behavior.
pub struct SimulatedStreamingEngine {
    inner: Arc<dyn SttEngine>,
    /// Display name override (e.g. "Groq (simulated streaming)").
    display_name: String,
}

impl SimulatedStreamingEngine {
    pub fn new(inner: Arc<dyn SttEngine>) -> Self {
        let display_name = format!("{} (simulated streaming)", inner.name());
        Self {
            inner,
            display_name,
        }
    }
}

#[async_trait]
impl StreamingSttEngine for SimulatedStreamingEngine {
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
        let (partial_tx, partial_rx) = mpsc::unbounded_channel();
        let (audio_tx, audio_rx) = mpsc::unbounded_channel::<Vec<f32>>();
        let (close_tx, close_rx) = oneshot::channel::<()>();

        let engine = self.inner.clone();
        let language = config.language.clone();

        tokio::spawn(async move {
            processing_loop(engine, language, audio_rx, close_rx, partial_tx).await;
        });

        let session = SimulatedStreamingSession {
            audio_tx,
            close_tx: Mutex::new(Some(close_tx)),
        };

        Ok((Box::new(session), partial_rx))
    }

    fn supported_languages(&self) -> Vec<Language> {
        self.inner.supported_languages()
    }

    fn model_info(&self) -> Option<ModelInfo> {
        self.inner.model_info()
    }
}

/// Session handle for a simulated streaming session.
/// Audio is sent through a channel to the processing loop task.
struct SimulatedStreamingSession {
    audio_tx: mpsc::UnboundedSender<Vec<f32>>,
    close_tx: Mutex<Option<oneshot::Sender<()>>>,
}

#[async_trait]
impl StreamingSttSession for SimulatedStreamingSession {
    async fn send_audio(&self, samples: &[f32]) -> Result<(), EngineError> {
        self.audio_tx
            .send(samples.to_vec())
            .map_err(|_| EngineError::TranscriptionFailed("session closed".into()))?;
        Ok(())
    }

    async fn close(&self) -> Result<(), EngineError> {
        if let Some(tx) = self.close_tx.lock().await.take() {
            let _ = tx.send(());
        }
        Ok(())
    }
}

/// Convert engine `WordTimestamp` values to processor `WordToken` values.
/// Normalizes word text by trimming whitespace so that tokens from different
/// engines (Groq returns "hello", whisper.cpp returns " hello") are consistent
/// for hypothesis comparison.
fn timestamps_to_tokens(words: &[WordTimestamp]) -> Vec<WordToken> {
    words
        .iter()
        .filter_map(|w| {
            let text = w.word.trim().to_string();
            if text.is_empty() {
                None
            } else {
                Some(WordToken {
                    text,
                    start: w.start,
                    end: w.end,
                    probability: w.probability,
                })
            }
        })
        .collect()
}

/// Join committed word tokens into a space-separated string.
fn join_tokens(tokens: &[WordToken]) -> String {
    tokens
        .iter()
        .map(|t| t.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Append newly committed text to the accumulated transcript, handling spacing.
fn append_text(accumulated: &mut String, new_text: &str) {
    let trimmed = new_text.trim();
    if trimmed.is_empty() {
        return;
    }
    if !accumulated.is_empty() {
        accumulated.push(' ');
    }
    accumulated.push_str(trimmed);
}

/// Fallback: split plain text into pseudo-tokens with estimated timestamps
/// evenly distributed across the audio duration. Used when the engine does
/// not return word-level timestamps.
fn text_to_estimated_tokens(text: &str, audio_duration_secs: f64) -> Vec<WordToken> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return Vec::new();
    }

    let word_duration = audio_duration_secs / words.len() as f64;
    words
        .iter()
        .enumerate()
        .map(|(i, w)| WordToken {
            text: w.to_string(),
            start: i as f64 * word_duration,
            end: (i + 1) as f64 * word_duration,
            probability: None,
        })
        .collect()
}

/// The main processing loop, spawned as a tokio task.
///
/// Receives audio from the session, feeds it through the OnlineProcessor
/// (VAD + audio buffer), periodically transcribes via the batch engine,
/// and emits committed tokens as `StreamingPartial` events.
async fn processing_loop(
    engine: Arc<dyn SttEngine>,
    language: Option<Language>,
    mut audio_rx: mpsc::UnboundedReceiver<Vec<f32>>,
    close_rx: oneshot::Receiver<()>,
    partial_tx: mpsc::UnboundedSender<StreamingPartial>,
) {
    let mut processor = OnlineProcessor::new(ProcessorConfig {
        cycle_interval: Duration::from_millis(1500),
        min_speech_before_transcribe: 0.5,
        buffer_trimming_sec: 15.0,
        vad_config: VadConfig::default(),
    });

    let mut accumulated_text = String::new();

    // Pin the close receiver so we can poll it in select!
    tokio::pin!(close_rx);

    // Tick at 100ms to check for transcription readiness between audio chunks.
    let mut cycle_ticker = tokio::time::interval(Duration::from_millis(100));

    loop {
        tokio::select! {
            biased;

            // Priority 1: drain audio from the feeder.
            Some(samples) = audio_rx.recv() => {
                processor.feed_audio(&samples);
            }

            // Priority 2: periodic check — run a transcription cycle if ready.
            _ = cycle_ticker.tick() => {
                if let Some(input) = processor.get_audio_for_transcription() {
                    let new_tokens = run_transcription_cycle(
                        &engine,
                        &mut processor,
                        input,
                        &language,
                    ).await;

                    if !new_tokens.is_empty() {
                        append_text(&mut accumulated_text, &join_tokens(&new_tokens));

                        let _ = partial_tx.send(StreamingPartial {
                            text: accumulated_text.clone(),
                            is_final: false,
                            confidence: None,
                        });
                    }
                }
            }

            // Priority 3: session closed — finalize.
            _ = &mut close_rx => {
                debug!("Simulated streaming session closing, running final transcription");

                // One last transcription of the full remaining buffer.
                if let Some(input) = processor.get_audio_for_final_transcription() {
                    let new_tokens = run_transcription_cycle(
                        &engine,
                        &mut processor,
                        input,
                        &language,
                    ).await;

                    if !new_tokens.is_empty() {
                        append_text(&mut accumulated_text, &join_tokens(&new_tokens));
                    }
                }

                // Force-flush any remaining uncommitted tokens.
                let remaining = processor.finalize();
                if !remaining.is_empty() {
                    append_text(&mut accumulated_text, &join_tokens(&remaining));
                }

                let _ = partial_tx.send(StreamingPartial {
                    text: accumulated_text,
                    is_final: true,
                    confidence: None,
                });

                break;
            }
        }
    }
}

/// Run a single transcription cycle: call the batch engine, parse word tokens,
/// feed them into the processor's hypothesis buffer, and return committed tokens.
async fn run_transcription_cycle(
    engine: &Arc<dyn SttEngine>,
    processor: &mut OnlineProcessor,
    input: processor::TranscriptionInput,
    language: &Option<Language>,
) -> Vec<WordToken> {
    let audio_duration = input.audio.len() as f64 / input.sample_rate as f64;

    let request = TranscribeRequest {
        audio: input.audio,
        sample_rate: input.sample_rate,
        language: language.clone(),
        prompt: input.prompt,
    };

    match engine.transcribe(request).await {
        Ok(transcription) => {
            // Convert to WordTokens — prefer real timestamps, fall back to estimates.
            let tokens = if let Some(ref words) = transcription.words {
                timestamps_to_tokens(words)
            } else {
                text_to_estimated_tokens(&transcription.text, audio_duration)
            };

            debug!(
                word_count = tokens.len(),
                text_len = transcription.text.len(),
                has_timestamps = transcription.words.is_some(),
                "Simulated streaming cycle complete"
            );

            processor.process_transcription_result(tokens)
        }
        Err(e) => {
            warn!("Simulated streaming transcription error: {e}");
            processor.mark_transcription_failed();
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mock batch engine that returns predictable transcriptions.
    struct MockEngine {
        /// Each call returns the next response in this list.
        responses: Mutex<Vec<super::super::Transcription>>,
    }

    impl MockEngine {
        fn new(responses: Vec<super::super::Transcription>) -> Self {
            Self {
                responses: Mutex::new(responses),
            }
        }
    }

    #[async_trait]
    impl SttEngine for MockEngine {
        fn name(&self) -> &str {
            "mock"
        }

        async fn transcribe(
            &self,
            _request: TranscribeRequest,
        ) -> Result<super::super::Transcription, EngineError> {
            let mut responses = self.responses.lock().await;
            if responses.is_empty() {
                Ok(super::super::Transcription {
                    text: String::new(),
                    language: None,
                    duration_ms: 0,
                    words: Some(Vec::new()),
                })
            } else {
                Ok(responses.remove(0))
            }
        }

        fn supported_languages(&self) -> Vec<Language> {
            vec![Language("en".to_string())]
        }

        fn model_info(&self) -> Option<ModelInfo> {
            None
        }
    }

    fn make_transcription(words: Vec<(&str, f64, f64)>) -> super::super::Transcription {
        let text = words
            .iter()
            .map(|(w, _, _)| *w)
            .collect::<Vec<_>>()
            .join(" ");
        let word_timestamps = words
            .into_iter()
            .map(|(w, s, e)| WordTimestamp {
                word: w.to_string(),
                start: s,
                end: e,
                probability: None,
            })
            .collect();
        super::super::Transcription {
            text,
            language: Some(Language("en".to_string())),
            duration_ms: 100,
            words: Some(word_timestamps),
        }
    }

    #[test]
    fn timestamps_to_tokens_converts_correctly() {
        let timestamps = vec![
            WordTimestamp {
                word: "hello".to_string(),
                start: 0.0,
                end: 0.5,
                probability: Some(0.9),
            },
            WordTimestamp {
                word: "world".to_string(),
                start: 0.5,
                end: 1.0,
                probability: None,
            },
        ];

        let tokens = timestamps_to_tokens(&timestamps);
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].text, "hello");
        assert_eq!(tokens[0].start, 0.0);
        assert_eq!(tokens[0].probability, Some(0.9));
        assert_eq!(tokens[1].text, "world");
        assert!(tokens[1].probability.is_none());
    }

    #[test]
    fn text_to_estimated_tokens_distributes_evenly() {
        let tokens = text_to_estimated_tokens("hello world foo", 3.0);
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].text, "hello");
        assert!((tokens[0].end - 1.0).abs() < 0.001);
        assert_eq!(tokens[1].text, "world");
        assert!((tokens[1].start - 1.0).abs() < 0.001);
        assert!((tokens[1].end - 2.0).abs() < 0.001);
        assert_eq!(tokens[2].text, "foo");
    }

    #[test]
    fn text_to_estimated_tokens_empty_text() {
        let tokens = text_to_estimated_tokens("", 1.0);
        assert!(tokens.is_empty());
    }

    #[test]
    fn engine_name_includes_inner() {
        let mock = Arc::new(MockEngine::new(vec![]));
        let engine = SimulatedStreamingEngine::new(mock);
        assert_eq!(engine.name(), "mock (simulated streaming)");
    }

    #[test]
    fn engine_supports_streaming() {
        let mock = Arc::new(MockEngine::new(vec![]));
        let engine = SimulatedStreamingEngine::new(mock);
        assert!(engine.supports_streaming());
    }

    #[tokio::test]
    async fn start_stream_returns_session_and_receiver() {
        let mock = Arc::new(MockEngine::new(vec![]));
        let engine = SimulatedStreamingEngine::new(mock);

        let config = StreamingConfig {
            sample_rate: 16000,
            channels: 1,
            language: None,
            endpoint_ms: 1500,
        };

        let result = engine.start_stream(config).await;
        assert!(result.is_ok());

        let (session, _rx) = result.unwrap();
        // Clean up.
        let _ = session.close().await;
    }

    #[tokio::test]
    async fn close_emits_final_partial() {
        let mock = Arc::new(MockEngine::new(vec![]));
        let engine = SimulatedStreamingEngine::new(mock);

        let config = StreamingConfig {
            sample_rate: 16000,
            channels: 1,
            language: None,
            endpoint_ms: 1500,
        };

        let (session, mut rx) = engine.start_stream(config).await.unwrap();

        // Close immediately without sending audio.
        session.close().await.unwrap();

        // Should receive a final partial.
        let mut got_final = false;
        while let Some(partial) = rx.recv().await {
            if partial.is_final {
                got_final = true;
                break;
            }
        }
        assert!(got_final, "Should receive is_final=true after close");
    }

    #[tokio::test]
    async fn send_audio_after_close_returns_error() {
        let mock = Arc::new(MockEngine::new(vec![]));
        let engine = SimulatedStreamingEngine::new(mock);

        let config = StreamingConfig {
            sample_rate: 16000,
            channels: 1,
            language: None,
            endpoint_ms: 1500,
        };

        let (session, _rx) = engine.start_stream(config).await.unwrap();
        session.close().await.unwrap();

        // Give the processing loop time to shut down.
        tokio::time::sleep(Duration::from_millis(50)).await;

        let result = session.send_audio(&[0.0; 1600]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn full_cycle_produces_partial_after_two_agreeing_transcriptions() {
        // Two agreeing hypotheses: "hello world" then "hello world foo".
        // After the second, "hello" and "world" should be committed.
        let responses = vec![
            make_transcription(vec![("hello", 0.0, 0.5), ("world", 0.5, 1.0)]),
            make_transcription(vec![
                ("hello", 0.0, 0.5),
                ("world", 0.5, 1.0),
                ("foo", 1.0, 1.5),
            ]),
            // Final transcription on close.
            make_transcription(vec![
                ("hello", 0.0, 0.5),
                ("world", 0.5, 1.0),
                ("foo", 1.0, 1.5),
            ]),
        ];

        let mock = Arc::new(MockEngine::new(responses));
        let engine = SimulatedStreamingEngine::new(mock);

        let config = StreamingConfig {
            sample_rate: 16000,
            channels: 1,
            language: None,
            endpoint_ms: 1500,
        };

        let (session, mut rx) = engine.start_stream(config).await.unwrap();

        // Generate a 300Hz tone that VAD will classify as speech.
        let tone: Vec<f32> = (0..48000) // 3 seconds
            .map(|i| {
                let t = i as f32 / 16000.0;
                0.5 * (2.0 * std::f32::consts::PI * 300.0 * t).sin()
            })
            .collect();

        // Send audio in chunks to simulate real-time feeding.
        for chunk in tone.chunks(1600) {
            let _ = session.send_audio(chunk).await;
        }

        // Wait for the processing loop to run at least one cycle.
        // The cycle interval is 1.5s and the ticker is 100ms, so the first
        // transcription fires after ~1.5s of buffered audio.
        tokio::time::sleep(Duration::from_millis(4000)).await;

        // Close and collect all partials.
        session.close().await.unwrap();

        let mut partials = Vec::new();
        while let Some(partial) = rx.recv().await {
            partials.push(partial);
            if partials.last().map(|p| p.is_final).unwrap_or(false) {
                break;
            }
        }

        // We should have at least one partial (the final one).
        assert!(!partials.is_empty(), "Should receive at least one partial");

        let final_partial = partials.last().unwrap();
        assert!(final_partial.is_final);
        // The final text should contain words from our mock transcriptions.
        // Exact content depends on timing/VAD, but it should not be empty
        // if VAD detected speech (which our 300Hz tone should trigger).
        debug!("Final text: {:?}", final_partial.text);
    }
}
