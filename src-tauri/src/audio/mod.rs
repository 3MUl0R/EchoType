pub mod capture;
pub mod denoise;
pub mod feedback;
pub mod mute;
pub mod pipeline;
#[allow(dead_code)]
pub mod playback;
pub mod resample;

use serde::{Deserialize, Serialize};

/// A buffer of audio samples with metadata.
#[derive(Debug, Clone)]
pub struct AudioBuffer {
    /// f32 samples, mono.
    pub samples: Vec<f32>,
    /// Sample rate in Hz.
    pub sample_rate: u32,
}

impl AudioBuffer {
    pub fn duration_secs(&self) -> f64 {
        if self.sample_rate == 0 {
            return 0.0;
        }
        self.samples.len() as f64 / self.sample_rate as f64
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

/// Info about an audio input device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioDeviceInfo {
    pub name: String,
    pub is_default: bool,
}
