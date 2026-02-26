use tracing::debug;

const FRAME_SIZE: usize = 480; // 10ms at 48kHz
const AMPLITUDE_SCALE: f32 = 32767.0;

/// Denoise 48kHz f32 audio using nnnoiseless.
///
/// nnnoiseless expects 480-sample frames at 48kHz with amplitudes in the
/// 16-bit range (-32768..32767). We scale f32 (-1.0..1.0) samples accordingly.
pub fn denoise(samples: &[f32], _sample_rate: u32) -> Vec<f32> {
    if samples.is_empty() {
        return Vec::new();
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
}
