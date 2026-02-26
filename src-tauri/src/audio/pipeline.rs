use tracing::{debug, info};

use super::AudioBuffer;

const WHISPER_SAMPLE_RATE: u32 = 16000;

/// Configuration for the audio processing pipeline.
pub struct PipelineConfig {
    /// Whether to apply noise suppression.
    pub denoise_enabled: bool,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            denoise_enabled: true,
        }
    }
}

/// Process raw captured audio into a format ready for Whisper.
///
/// Pipeline: denoise (if enabled) → resample to 16kHz.
pub fn process(raw: &AudioBuffer, config: &PipelineConfig) -> Result<AudioBuffer, String> {
    if raw.is_empty() {
        return Ok(AudioBuffer {
            samples: Vec::new(),
            sample_rate: WHISPER_SAMPLE_RATE,
        });
    }

    info!(
        input_rate = raw.sample_rate,
        input_samples = raw.samples.len(),
        denoise = config.denoise_enabled,
        "Processing audio pipeline"
    );

    // Step 1: Denoise at capture sample rate
    let denoised = if config.denoise_enabled {
        debug!("Running noise suppression");
        super::denoise::denoise(&raw.samples, raw.sample_rate)
    } else {
        raw.samples.clone()
    };

    // Step 2: Resample to 16kHz
    let resampled = super::resample::resample(&denoised, raw.sample_rate, WHISPER_SAMPLE_RATE)?;

    info!(
        output_rate = WHISPER_SAMPLE_RATE,
        output_samples = resampled.len(),
        duration_secs = resampled.len() as f64 / WHISPER_SAMPLE_RATE as f64,
        "Audio pipeline complete"
    );

    Ok(AudioBuffer {
        samples: resampled,
        sample_rate: WHISPER_SAMPLE_RATE,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_empty_input() {
        let buf = AudioBuffer {
            samples: Vec::new(),
            sample_rate: 48000,
        };
        let result = process(&buf, &PipelineConfig::default()).unwrap();
        assert!(result.is_empty());
        assert_eq!(result.sample_rate, 16000);
    }

    #[test]
    fn pipeline_processes_audio() {
        let buf = AudioBuffer {
            samples: vec![0.1; 48000], // 1 second at 48kHz
            sample_rate: 48000,
        };
        let result = process(&buf, &PipelineConfig::default()).unwrap();
        assert_eq!(result.sample_rate, 16000);
        assert!(!result.is_empty());
    }

    #[test]
    fn pipeline_denoise_disabled() {
        let buf = AudioBuffer {
            samples: vec![0.1; 48000],
            sample_rate: 48000,
        };
        let config = PipelineConfig {
            denoise_enabled: false,
        };
        let result = process(&buf, &config).unwrap();
        assert_eq!(result.sample_rate, 16000);
    }
}
