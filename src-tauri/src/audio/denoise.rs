use tracing::{debug, warn};

const FRAME_SIZE: usize = 480; // 10ms at 48kHz
const EXPECTED_SAMPLE_RATE: u32 = 48000;
const AMPLITUDE_SCALE: f32 = 32767.0;

/// Noise suppression level.
/// Controls the number of denoise passes: light=1, moderate=2, aggressive=3.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SuppressionLevel {
    Off,
    Light,
    Moderate,
    Aggressive,
}

impl SuppressionLevel {
    /// Parse from settings string value.
    pub fn from_str(s: &str) -> Self {
        match s {
            "off" => Self::Off,
            "light" => Self::Light,
            "moderate" => Self::Moderate,
            "aggressive" => Self::Aggressive,
            _ => Self::Moderate,
        }
    }

    fn passes(self) -> u32 {
        match self {
            Self::Off => 0,
            Self::Light => 1,
            Self::Moderate => 2,
            Self::Aggressive => 3,
        }
    }
}

/// Denoise 48kHz f32 audio using nnnoiseless with configurable level.
pub fn denoise_with_level(samples: &[f32], sample_rate: u32, level: SuppressionLevel) -> Vec<f32> {
    if level == SuppressionLevel::Off {
        return samples.to_vec();
    }

    let mut result = denoise(samples, sample_rate);
    // Additional passes for higher suppression levels
    for _ in 1..level.passes() {
        result = denoise(&result, sample_rate);
    }
    result
}

/// Denoise 48kHz f32 audio using nnnoiseless (single pass).
///
/// nnnoiseless expects 480-sample frames at 48kHz with amplitudes in the
/// 16-bit range (-32768..32767). We scale f32 (-1.0..1.0) samples accordingly.
///
/// Returns the input unchanged if the sample rate is not 48kHz.
pub fn denoise(samples: &[f32], sample_rate: u32) -> Vec<f32> {
    if samples.is_empty() {
        return Vec::new();
    }

    if sample_rate != EXPECTED_SAMPLE_RATE {
        warn!(
            sample_rate,
            expected = EXPECTED_SAMPLE_RATE,
            "Skipping denoise: nnnoiseless requires 48kHz input"
        );
        return samples.to_vec();
    }

    let mut state = nnnoiseless::DenoiseState::new();
    let mut output = Vec::with_capacity(samples.len());
    let original_len = samples.len();

    // Pre-feed a silent frame to avoid first-frame artifacts
    let silent_input = [0.0f32; FRAME_SIZE];
    let mut silent_output = [0.0f32; FRAME_SIZE];
    state.process_frame(&mut silent_output, &silent_input);

    // Process in 480-sample frames
    let mut pos = 0;
    while pos < samples.len() {
        let mut input_frame = [0.0f32; FRAME_SIZE];
        let mut output_frame = [0.0f32; FRAME_SIZE];
        let remaining = samples.len() - pos;
        let copy_len = remaining.min(FRAME_SIZE);

        // Scale to 16-bit amplitude range
        for i in 0..copy_len {
            input_frame[i] = samples[pos + i] * AMPLITUDE_SCALE;
        }
        // Zero-pad if partial frame (already zero-initialized)

        state.process_frame(&mut output_frame, &input_frame);

        // Scale back to f32 range and collect
        for sample in output_frame.iter().take(copy_len) {
            output.push(sample / AMPLITUDE_SCALE);
        }

        pos += copy_len;
    }

    // Trim to original length (in case of rounding)
    output.truncate(original_len);

    debug!(
        input_samples = samples.len(),
        output_samples = output.len(),
        "Audio denoised"
    );

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denoise_preserves_length() {
        let input = vec![0.1f32; 1000];
        let output = denoise(&input, 48000);
        assert_eq!(output.len(), input.len());
    }

    #[test]
    fn denoise_handles_empty() {
        let output = denoise(&[], 48000);
        assert!(output.is_empty());
    }

    #[test]
    fn denoise_handles_short_buffer() {
        let input = vec![0.05f32; 100]; // Less than one frame
        let output = denoise(&input, 48000);
        assert_eq!(output.len(), input.len());
    }

    #[test]
    fn denoise_handles_exact_frame() {
        let input = vec![0.05f32; 480]; // Exactly one frame
        let output = denoise(&input, 48000);
        assert_eq!(output.len(), input.len());
    }

    #[test]
    fn suppression_level_from_str() {
        assert_eq!(SuppressionLevel::from_str("off"), SuppressionLevel::Off);
        assert_eq!(SuppressionLevel::from_str("light"), SuppressionLevel::Light);
        assert_eq!(
            SuppressionLevel::from_str("moderate"),
            SuppressionLevel::Moderate
        );
        assert_eq!(
            SuppressionLevel::from_str("aggressive"),
            SuppressionLevel::Aggressive
        );
        // Unknown defaults to moderate
        assert_eq!(
            SuppressionLevel::from_str("unknown"),
            SuppressionLevel::Moderate
        );
    }

    #[test]
    fn denoise_with_level_off_returns_copy() {
        let input = vec![0.1f32; 1000];
        let output = denoise_with_level(&input, 48000, SuppressionLevel::Off);
        assert_eq!(output, input);
    }

    #[test]
    fn denoise_with_level_preserves_length() {
        let input = vec![0.1f32; 1000];
        let output = denoise_with_level(&input, 48000, SuppressionLevel::Light);
        assert_eq!(output.len(), input.len());
        let output = denoise_with_level(&input, 48000, SuppressionLevel::Aggressive);
        assert_eq!(output.len(), input.len());
    }

    #[test]
    fn denoise_skips_non_48k_input() {
        let input = vec![0.1f32; 1000];
        let output = denoise(&input, 44100);
        // Should return input unchanged when not 48kHz
        assert_eq!(output, input);
    }
}
