//! Online audio processor for simulated streaming.
//!
//! Coordinates the audio buffer, VAD, and HypothesisBuffer to produce
//! incremental transcription output from batch transcription results.
//! The actual transcription call is made by the caller (the processing loop
//! in mod.rs) — this module handles everything else.

use std::time::{Duration, Instant};

use super::local_agreement::{HypothesisBuffer, WordToken};
use super::vad::{VadConfig, VadEvent, VadIterator};

const SAMPLE_RATE: u32 = 16000;

/// Configuration for the online processor.
#[derive(Debug, Clone)]
pub struct ProcessorConfig {
    /// Minimum time between transcription cycles.
    pub cycle_interval: Duration,
    /// Maximum audio buffer duration (seconds) before trimming.
    pub buffer_trimming_sec: f64,
    /// Minimum speech duration (seconds) before allowing a transcription cycle.
    pub min_speech_before_transcribe: f64,
    /// VAD configuration.
    pub vad_config: VadConfig,
}

impl Default for ProcessorConfig {
    fn default() -> Self {
        Self {
            cycle_interval: Duration::from_millis(1500),
            buffer_trimming_sec: 15.0,
            min_speech_before_transcribe: 0.5,
            vad_config: VadConfig::default(),
        }
    }
}

/// Manages audio accumulation, VAD filtering, buffer trimming, and hypothesis
/// comparison for simulated streaming. Does NOT own or call the STT engine —
/// the caller is responsible for running transcription and feeding results back.
pub struct OnlineProcessor {
    hypothesis: HypothesisBuffer,
    vad: VadIterator,

    /// Accumulated speech audio (16kHz mono f32). Only contains audio from
    /// regions the VAD classified as speech.
    audio_buffer: Vec<f32>,
    /// Absolute time (seconds) of audio_buffer[0] in the stream.
    buffer_time_offset: f64,
    /// Total samples received (including silence), for absolute time tracking.
    total_samples_received: usize,

    /// Whether we are currently in a speech region (per VAD).
    in_speech: bool,

    /// Configuration.
    config: ProcessorConfig,

    /// When the last transcription cycle was run.
    last_process_time: Option<Instant>,
    /// Whether a transcription is currently in-flight (prevents overlapping).
    processing_in_flight: bool,
}

impl OnlineProcessor {
    pub fn new(config: ProcessorConfig) -> Self {
        Self {
            hypothesis: HypothesisBuffer::new(),
            vad: VadIterator::new(config.vad_config.clone()),
            audio_buffer: Vec::new(),
            buffer_time_offset: 0.0,
            total_samples_received: 0,
            in_speech: false,
            config,
            last_process_time: None,
            processing_in_flight: false,
        }
    }

    /// Feed raw audio samples (16kHz mono f32) into the processor.
    /// The VAD filters speech regions and accumulates them in the audio buffer.
    pub fn feed_audio(&mut self, samples: &[f32]) {
        let events = self.vad.process(samples);

        for event in &events {
            match event {
                VadEvent::SpeechStart { .. } => {
                    self.in_speech = true;
                }
                VadEvent::SpeechEnd { .. } => {
                    self.in_speech = false;
                }
            }
        }

        // If in speech, accumulate audio. We always add the full chunk when in
        // speech to avoid cutting words at VAD boundaries — the buffer trimming
        // and hypothesis comparison handle precision.
        if self.in_speech {
            self.audio_buffer.extend_from_slice(samples);
        } else if !events.is_empty() {
            // If we just transitioned out of speech in this chunk, still include it
            // so we don't cut the tail of the utterance.
            if events.iter().any(|e| matches!(e, VadEvent::SpeechEnd { .. })) {
                self.audio_buffer.extend_from_slice(samples);
            }
        }

        self.total_samples_received += samples.len();
    }

    /// Whether enough time and audio have accumulated to warrant a transcription cycle.
    pub fn should_process(&self) -> bool {
        if self.processing_in_flight {
            return false;
        }

        // Check minimum cycle interval.
        if let Some(last) = self.last_process_time {
            if last.elapsed() < self.config.cycle_interval {
                return false;
            }
        }

        // Check minimum audio buffer duration.
        let buffer_duration = self.audio_buffer.len() as f64 / SAMPLE_RATE as f64;
        buffer_duration >= self.config.min_speech_before_transcribe
    }

    /// Get the current audio buffer for transcription, along with its time offset.
    /// Returns `None` if there's not enough audio to transcribe.
    ///
    /// The caller should:
    /// 1. Call `get_audio_for_transcription()` to get the audio
    /// 2. Run the batch engine on it
    /// 3. Call `process_transcription_result()` with the word tokens
    pub fn get_audio_for_transcription(&mut self) -> Option<TranscriptionInput> {
        if !self.should_process() {
            return None;
        }

        self.processing_in_flight = true;
        self.last_process_time = Some(Instant::now());

        Some(TranscriptionInput {
            audio: self.audio_buffer.clone(),
            sample_rate: SAMPLE_RATE,
            offset: self.buffer_time_offset,
            prompt: self.hypothesis.build_prompt(),
        })
    }

    /// Process the result of a batch transcription. Feed word-level tokens
    /// from the engine response into the hypothesis buffer and return any
    /// newly committed (stable) tokens.
    ///
    /// `tokens` — word tokens with times relative to the start of the
    ///   transcribed audio buffer (not absolute stream time).
    pub fn process_transcription_result(
        &mut self,
        tokens: Vec<WordToken>,
    ) -> Vec<WordToken> {
        self.processing_in_flight = false;

        let committed = self
            .hypothesis
            .insert_and_flush(tokens, self.buffer_time_offset);

        self.maybe_trim_buffer();

        committed
    }

    /// Mark that an in-flight transcription failed. Resets the in-flight flag
    /// so the next cycle can proceed.
    pub fn mark_transcription_failed(&mut self) {
        self.processing_in_flight = false;
    }

    /// End-of-stream finalization. Returns any remaining uncommitted tokens
    /// from the hypothesis buffer.
    ///
    /// The caller should first do one final transcription of the full buffer
    /// and call `process_transcription_result()`, then call this to flush
    /// any remaining uncommitted tokens.
    pub fn finalize(&mut self) -> Vec<WordToken> {
        self.hypothesis.force_flush()
    }

    /// Get the current audio buffer for a final transcription (ignores timing checks).
    pub fn get_audio_for_final_transcription(&self) -> Option<TranscriptionInput> {
        if self.audio_buffer.is_empty() {
            return None;
        }

        Some(TranscriptionInput {
            audio: self.audio_buffer.clone(),
            sample_rate: SAMPLE_RATE,
            offset: self.buffer_time_offset,
            prompt: self.hypothesis.build_prompt(),
        })
    }

    /// Current duration of the audio buffer in seconds.
    pub fn buffer_duration(&self) -> f64 {
        self.audio_buffer.len() as f64 / SAMPLE_RATE as f64
    }

    /// Absolute time position in the stream (seconds).
    pub fn stream_time(&self) -> f64 {
        self.total_samples_received as f64 / SAMPLE_RATE as f64
    }

    /// Trim the audio buffer when it exceeds `buffer_trimming_sec`.
    /// Trims at the last committed token boundary to avoid re-transcribing
    /// already-committed audio.
    fn maybe_trim_buffer(&mut self) {
        let buffer_duration = self.audio_buffer.len() as f64 / SAMPLE_RATE as f64;
        if buffer_duration <= self.config.buffer_trimming_sec {
            return;
        }

        let trim_time = self.hypothesis.last_committed_time();
        if trim_time <= self.buffer_time_offset {
            // Nothing to trim (no committed tokens in current buffer).
            return;
        }

        let cut_seconds = trim_time - self.buffer_time_offset;
        let samples_to_remove = (cut_seconds * SAMPLE_RATE as f64) as usize;

        if samples_to_remove >= self.audio_buffer.len() {
            // Trim everything — shouldn't normally happen, but handle gracefully.
            self.audio_buffer.clear();
        } else {
            self.audio_buffer.drain(..samples_to_remove);
        }

        self.hypothesis.pop_committed(trim_time);
        self.buffer_time_offset = trim_time;
    }
}

/// Audio data prepared for a transcription call.
#[derive(Debug)]
pub struct TranscriptionInput {
    /// Audio samples (16kHz mono f32).
    pub audio: Vec<f32>,
    /// Sample rate (always 16000).
    pub sample_rate: u32,
    /// Absolute time offset of the audio buffer start. Used to convert
    /// word timestamps from buffer-relative to stream-absolute.
    pub offset: f64,
    /// Prompt text from previously committed tokens (for context continuity).
    pub prompt: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_processor() -> OnlineProcessor {
        OnlineProcessor::new(ProcessorConfig {
            cycle_interval: Duration::from_millis(0), // No timing gate in tests
            min_speech_before_transcribe: 0.1,        // 100ms min
            buffer_trimming_sec: 2.0,                 // Trim at 2s for easy testing
            vad_config: VadConfig::default(),
        })
    }

    fn speech_tone(duration_secs: f32) -> Vec<f32> {
        let num_samples = (SAMPLE_RATE as f32 * duration_secs) as usize;
        (0..num_samples)
            .map(|i| {
                let t = i as f32 / SAMPLE_RATE as f32;
                0.5 * (2.0 * std::f32::consts::PI * 300.0 * t).sin()
            })
            .collect()
    }

    fn word(text: &str, start: f64, end: f64) -> WordToken {
        WordToken {
            text: text.to_string(),
            start,
            end,
            probability: None,
        }
    }

    #[test]
    fn silence_does_not_accumulate() {
        let mut proc = make_processor();
        proc.feed_audio(&vec![0.0; SAMPLE_RATE as usize]); // 1s silence
        assert!(proc.audio_buffer.is_empty() || proc.buffer_duration() < 0.1);
    }

    #[test]
    fn speech_accumulates_in_buffer() {
        let mut proc = make_processor();
        proc.feed_audio(&speech_tone(1.0));
        assert!(proc.buffer_duration() > 0.5, "Speech should accumulate");
    }

    #[test]
    fn should_process_respects_minimum_audio() {
        let mut proc = OnlineProcessor::new(ProcessorConfig {
            cycle_interval: Duration::from_millis(0),
            min_speech_before_transcribe: 1.0, // Need 1s of speech
            ..Default::default()
        });

        proc.feed_audio(&speech_tone(0.3));
        // May or may not have enough depending on VAD — but definitely not 1s.
        // We test the logic, not the exact VAD boundary.
        if proc.buffer_duration() < 1.0 {
            assert!(!proc.should_process());
        }
    }

    #[test]
    fn transcription_result_flows_through_hypothesis_buffer() {
        let mut proc = make_processor();
        proc.feed_audio(&speech_tone(1.0));

        // Simulate first transcription.
        if let Some(_input) = proc.get_audio_for_transcription() {
            let tokens = vec![word("hello", 0.0, 0.5), word("world", 0.5, 1.0)];
            let committed = proc.process_transcription_result(tokens);
            // First hypothesis — nothing committed yet.
            assert!(committed.is_empty());
        }

        // Feed more audio and simulate second transcription with agreement.
        proc.feed_audio(&speech_tone(0.5));
        // Reset timing gate.
        proc.last_process_time = None;
        proc.processing_in_flight = false;

        if let Some(_input) = proc.get_audio_for_transcription() {
            let tokens = vec![
                word("hello", 0.0, 0.5),
                word("world", 0.5, 1.0),
                word("foo", 1.0, 1.5),
            ];
            let committed = proc.process_transcription_result(tokens);
            // "hello" and "world" should now be committed (agreement).
            assert_eq!(committed.len(), 2);
            assert_eq!(committed[0].text, "hello");
            assert_eq!(committed[1].text, "world");
        }
    }

    #[test]
    fn finalize_flushes_remaining() {
        let mut proc = make_processor();
        proc.feed_audio(&speech_tone(1.0));

        if let Some(_) = proc.get_audio_for_transcription() {
            let tokens = vec![word("hello", 0.0, 0.5)];
            proc.process_transcription_result(tokens);
        }

        let remaining = proc.finalize();
        // The buffer from the first (only) hypothesis should be force-flushed.
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].text, "hello");
    }

    #[test]
    fn buffer_trimming_removes_old_audio() {
        let mut proc = OnlineProcessor::new(ProcessorConfig {
            cycle_interval: Duration::from_millis(0),
            min_speech_before_transcribe: 0.1,
            buffer_trimming_sec: 1.0, // Very aggressive trimming for test
            vad_config: VadConfig::default(),
        });

        // Feed 3 seconds of speech.
        proc.feed_audio(&speech_tone(3.0));
        let pre_trim_duration = proc.buffer_duration();

        // Simulate two agreeing transcriptions so tokens get committed.
        proc.processing_in_flight = false;
        if let Some(_) = proc.get_audio_for_transcription() {
            let tokens = vec![
                word("one", 0.0, 0.5),
                word("two", 0.5, 1.0),
                word("three", 1.0, 1.5),
            ];
            proc.process_transcription_result(tokens);
        }
        proc.last_process_time = None;
        proc.processing_in_flight = false;
        if let Some(_) = proc.get_audio_for_transcription() {
            let tokens = vec![
                word("one", 0.0, 0.5),
                word("two", 0.5, 1.0),
                word("three", 1.0, 1.5),
                word("four", 1.5, 2.0),
            ];
            proc.process_transcription_result(tokens);
        }

        // After committing "one", "two", "three", the buffer should have been trimmed.
        let post_trim_duration = proc.buffer_duration();
        assert!(
            post_trim_duration < pre_trim_duration,
            "Buffer should have been trimmed: {post_trim_duration} < {pre_trim_duration}"
        );
    }

    #[test]
    fn in_flight_prevents_concurrent_processing() {
        let mut proc = make_processor();
        proc.feed_audio(&speech_tone(1.0));

        let input = proc.get_audio_for_transcription();
        assert!(input.is_some());
        assert!(proc.processing_in_flight);

        // Should not allow another cycle while one is in-flight.
        assert!(!proc.should_process());

        // After marking failed, should allow again.
        proc.mark_transcription_failed();
        assert!(!proc.processing_in_flight);
    }

    #[test]
    fn prompt_is_passed_through() {
        let mut proc = make_processor();
        proc.feed_audio(&speech_tone(1.0));

        // First transcription — no prompt yet.
        if let Some(input) = proc.get_audio_for_transcription() {
            assert!(input.prompt.is_none());
            let tokens = vec![word("hello", 0.0, 0.5)];
            proc.process_transcription_result(tokens);
        }

        // Second transcription with agreement — commits "hello".
        proc.last_process_time = None;
        proc.processing_in_flight = false;
        proc.feed_audio(&speech_tone(0.5));
        if let Some(input) = proc.get_audio_for_transcription() {
            // After committing tokens, prompt should be available
            // (it may be None if nothing committed from prev round,
            //  but after agreement it should have content)
            let tokens = vec![word("hello", 0.0, 0.5), word("world", 0.5, 1.0)];
            proc.process_transcription_result(tokens);
        }

        // Third transcription — prompt should contain committed text.
        proc.last_process_time = None;
        proc.processing_in_flight = false;
        if let Some(input) = proc.get_audio_for_transcription() {
            assert!(input.prompt.is_some());
            assert!(input.prompt.unwrap().contains("hello"));
        }
    }
}
