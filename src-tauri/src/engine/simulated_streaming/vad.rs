//! Voice Activity Detection wrapper around webrtc-vad.
//!
//! Implements a state machine that detects speech start/end boundaries with
//! hysteresis (minimum silence duration before confirming speech end) and
//! configurable padding around speech boundaries.

use webrtc_vad::{SampleRate, Vad, VadMode};

/// Events emitted by the VAD state machine.
#[derive(Debug, Clone, PartialEq)]
pub enum VadEvent {
    /// Speech detected starting at this sample offset (absolute).
    SpeechStart { sample: usize },
    /// Speech ended at this sample offset (absolute).
    SpeechEnd { sample: usize },
}

/// VAD aggressiveness level (mirrors webrtc_vad::VadMode without requiring
/// Debug/Clone on the foreign type).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VadAggressiveness {
    /// Least aggressive — fewest false negatives.
    Quality,
    LowBitrate,
    Aggressive,
    /// Most aggressive — fewest false positives.
    VeryAggressive,
}

impl VadAggressiveness {
    fn to_vad_mode(self) -> VadMode {
        match self {
            Self::Quality => VadMode::Quality,
            Self::LowBitrate => VadMode::LowBitrate,
            Self::Aggressive => VadMode::Aggressive,
            Self::VeryAggressive => VadMode::VeryAggressive,
        }
    }
}

/// Configuration for the VAD iterator.
#[derive(Debug, Clone)]
pub struct VadConfig {
    /// Minimum silence duration (ms) before confirming speech end.
    pub min_silence_ms: u32,
    /// Padding (ms) added to speech start/end boundaries.
    pub speech_pad_ms: u32,
    /// VAD aggressiveness level.
    pub aggressiveness: VadAggressiveness,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            min_silence_ms: 100,
            speech_pad_ms: 30,
            aggressiveness: VadAggressiveness::Quality,
        }
    }
}

/// Frame size for 16kHz audio at 30ms per frame (webrtc-vad native).
const FRAME_SIZE: usize = 480;
const SAMPLE_RATE: u32 = 16000;

/// Stateful VAD iterator that buffers incoming audio and emits speech
/// boundary events with hysteresis and padding.
pub struct VadIterator {
    vad: Vad,
    /// True while we consider the user to be speaking.
    triggered: bool,
    /// Sample position where silence tentatively began (0 = not set).
    temp_end_sample: usize,
    /// Total samples processed so far (absolute position in stream).
    current_sample: usize,
    /// Minimum silence samples before confirming speech end.
    min_silence_samples: usize,
    /// Padding samples applied to speech boundaries.
    speech_pad_samples: usize,
    /// Internal buffer for accumulating samples until we have a full frame.
    buffer: Vec<f32>,
}

// Safety: Vad wraps a *mut to a C struct with no global/thread-local state.
// Each Vad instance is independent, so sending across threads is safe as long
// as we maintain exclusive access (which we do — VadIterator takes &mut self).
unsafe impl Send for VadIterator {}

impl VadIterator {
    pub fn new(config: VadConfig) -> Self {
        let vad = Vad::new_with_rate_and_mode(
            SampleRate::Rate16kHz,
            config.aggressiveness.to_vad_mode(),
        );
        let min_silence_samples = (SAMPLE_RATE * config.min_silence_ms / 1000) as usize;
        let speech_pad_samples = (SAMPLE_RATE * config.speech_pad_ms / 1000) as usize;

        Self {
            vad,
            triggered: false,
            temp_end_sample: 0,
            current_sample: 0,
            min_silence_samples,
            speech_pad_samples,
            buffer: Vec::with_capacity(FRAME_SIZE),
        }
    }

    /// Feed f32 audio samples (16kHz mono). Returns any speech boundary events
    /// detected in this batch. Multiple events can be returned if the input
    /// contains multiple transitions.
    pub fn process(&mut self, samples: &[f32]) -> Vec<VadEvent> {
        let mut events = Vec::new();
        self.buffer.extend_from_slice(samples);

        while self.buffer.len() >= FRAME_SIZE {
            let frame: Vec<i16> = self.buffer[..FRAME_SIZE]
                .iter()
                .map(|&s| (s * 32767.0).clamp(-32768.0, 32767.0) as i16)
                .collect();
            self.buffer.drain(..FRAME_SIZE);

            self.current_sample += FRAME_SIZE;

            let is_voice = self.vad.is_voice_segment(&frame).unwrap_or(false);

            if is_voice && self.temp_end_sample != 0 {
                // Speech resumed before silence was confirmed — cancel pending end.
                self.temp_end_sample = 0;
            }

            if is_voice && !self.triggered {
                // Speech start detected.
                self.triggered = true;
                let start = self.current_sample.saturating_sub(
                    self.speech_pad_samples + FRAME_SIZE,
                );
                events.push(VadEvent::SpeechStart { sample: start });
            }

            if !is_voice && self.triggered {
                if self.temp_end_sample == 0 {
                    // Mark tentative silence start.
                    self.temp_end_sample = self.current_sample;
                }

                if self.current_sample - self.temp_end_sample >= self.min_silence_samples {
                    // Silence confirmed — speech end.
                    let end = self.temp_end_sample + self.speech_pad_samples;
                    self.triggered = false;
                    self.temp_end_sample = 0;
                    events.push(VadEvent::SpeechEnd { sample: end });
                }
            }
        }

        events
    }

    /// Whether the VAD currently considers the stream to be in a speech region.
    pub fn is_speech(&self) -> bool {
        self.triggered
    }

    /// Total samples processed so far.
    pub fn current_sample(&self) -> usize {
        self.current_sample
    }

    /// Reset all internal state for a new stream.
    pub fn reset(&mut self) {
        self.vad = Vad::new_with_rate_and_mode(SampleRate::Rate16kHz, VadMode::Quality);
        self.triggered = false;
        self.temp_end_sample = 0;
        self.current_sample = 0;
        self.buffer.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn silence(num_samples: usize) -> Vec<f32> {
        vec![0.0; num_samples]
    }

    fn tone(num_samples: usize, freq_hz: f32) -> Vec<f32> {
        // Generate a sine wave that webrtc-vad will classify as voice.
        // We use a moderate amplitude and speech-like frequency.
        (0..num_samples)
            .map(|i| {
                let t = i as f32 / SAMPLE_RATE as f32;
                0.5 * (2.0 * std::f32::consts::PI * freq_hz * t).sin()
            })
            .collect()
    }

    #[test]
    fn silence_produces_no_events() {
        let mut vad = VadIterator::new(VadConfig::default());
        // Feed 1 second of silence.
        let events = vad.process(&silence(SAMPLE_RATE as usize));
        assert!(events.is_empty(), "Silence should not trigger any events");
        assert!(!vad.is_speech());
    }

    #[test]
    fn speech_triggers_start_event() {
        let mut vad = VadIterator::new(VadConfig::default());
        // Feed speech-like audio (300Hz tone, well within voice range).
        let events = vad.process(&tone(SAMPLE_RATE as usize, 300.0));
        let starts: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, VadEvent::SpeechStart { .. }))
            .collect();
        assert!(!starts.is_empty(), "Voice tone should trigger SpeechStart");
        assert!(vad.is_speech());
    }

    #[test]
    fn speech_then_silence_triggers_both_events() {
        let mut vad = VadIterator::new(VadConfig::default());

        // Speech for 0.5s.
        let mut all_events = vad.process(&tone(8000, 300.0));
        assert!(vad.is_speech());

        // Silence for 0.5s (well beyond 100ms min_silence).
        all_events.extend(vad.process(&silence(8000)));

        let starts = all_events
            .iter()
            .filter(|e| matches!(e, VadEvent::SpeechStart { .. }))
            .count();
        let ends = all_events
            .iter()
            .filter(|e| matches!(e, VadEvent::SpeechEnd { .. }))
            .count();

        assert_eq!(starts, 1, "Expected exactly one SpeechStart");
        assert_eq!(ends, 1, "Expected exactly one SpeechEnd");
        assert!(!vad.is_speech());
    }

    #[test]
    fn brief_silence_does_not_end_speech() {
        let mut vad = VadIterator::new(VadConfig::default());

        // Speech.
        vad.process(&tone(8000, 300.0));
        assert!(vad.is_speech());

        // Very brief silence (50ms = 800 samples, under 100ms threshold).
        let events = vad.process(&silence(800));

        let ends = events
            .iter()
            .filter(|e| matches!(e, VadEvent::SpeechEnd { .. }))
            .count();
        assert_eq!(ends, 0, "Brief silence should not end speech");
    }

    #[test]
    fn incremental_feeding_works() {
        let mut vad = VadIterator::new(VadConfig::default());

        // Feed tone in small chunks (100 samples at a time, less than frame size).
        let tone_data = tone(8000, 300.0);
        let mut all_events = Vec::new();
        for chunk in tone_data.chunks(100) {
            all_events.extend(vad.process(chunk));
        }

        let starts = all_events
            .iter()
            .filter(|e| matches!(e, VadEvent::SpeechStart { .. }))
            .count();
        assert_eq!(starts, 1, "Should detect speech even with small chunks");
    }

    #[test]
    fn reset_clears_state() {
        let mut vad = VadIterator::new(VadConfig::default());
        vad.process(&tone(8000, 300.0));
        assert!(vad.is_speech());

        vad.reset();
        assert!(!vad.is_speech());
        assert_eq!(vad.current_sample(), 0);
    }
}
