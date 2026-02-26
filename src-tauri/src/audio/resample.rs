use rubato::{FftFixedIn, Resampler};
use tracing::debug;

/// Resample audio from one sample rate to another.
///
/// Uses rubato's FFT-based resampler for high quality.
/// Primary use: 48kHz → 16kHz for Whisper.
pub fn resample(samples: &[f32], from_rate: u32, to_rate: u32) -> Result<Vec<f32>, String> {
    if from_rate == to_rate {
        return Ok(samples.to_vec());
    }

    if samples.is_empty() {
        return Ok(Vec::new());
    }

    let chunk_size = 1024;

    let mut resampler = FftFixedIn::<f32>::new(
        from_rate as usize,
        to_rate as usize,
        chunk_size,
        2, // sub_chunks
        1, // channels (mono)
    )
    .map_err(|e| format!("Failed to create resampler: {e}"))?;

    let mut output = Vec::new();

    // Process full chunks
    let mut pos = 0;
    while pos + chunk_size <= samples.len() {
        let chunk = &samples[pos..pos + chunk_size];
        let result = resampler
            .process(&[chunk], None)
            .map_err(|e| format!("Resample error: {e}"))?;
        output.extend_from_slice(&result[0]);
        pos += chunk_size;
    }

    // Handle remaining samples: pad to chunk_size
    if pos < samples.len() {
        let remaining = &samples[pos..];
        let mut padded = vec![0.0f32; chunk_size];
        padded[..remaining.len()].copy_from_slice(remaining);
        let result = resampler
            .process(&[&padded], None)
            .map_err(|e| format!("Resample tail error: {e}"))?;

        // Calculate how many output samples correspond to the remaining input
        let expected_out =
            (remaining.len() as f64 * to_rate as f64 / from_rate as f64).ceil() as usize;
        let take = expected_out.min(result[0].len());
        output.extend_from_slice(&result[0][..take]);
    }

    debug!(
        from_rate,
        to_rate,
        input_samples = samples.len(),
        output_samples = output.len(),
        "Audio resampled"
    );

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_rate_is_identity() {
        let input = vec![0.5f32; 1000];
        let output = resample(&input, 48000, 48000).unwrap();
        assert_eq!(output.len(), input.len());
    }

    #[test]
    fn downsample_48k_to_16k() {
        let input = vec![0.1f32; 48000]; // 1 second at 48kHz
        let output = resample(&input, 48000, 16000).unwrap();
        // Should be approximately 16000 samples (1 second at 16kHz)
        let ratio = output.len() as f64 / 16000.0;
        assert!(
            (0.95..1.05).contains(&ratio),
            "Expected ~16000 samples, got {}",
            output.len()
        );
    }

    #[test]
    fn empty_input() {
        let output = resample(&[], 48000, 16000).unwrap();
        assert!(output.is_empty());
    }
}
