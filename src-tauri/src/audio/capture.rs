use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use tracing::{error, info};

use super::{AudioBuffer, AudioDeviceInfo};

/// Errors from audio capture operations.
#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("no input device available")]
    NoDevice,
    #[error("failed to get device config: {0}")]
    ConfigError(String),
    #[error("failed to build stream: {0}")]
    StreamError(String),
}

/// An active audio capture session.
pub struct CaptureSession {
    stream: cpal::Stream,
    buffer: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
    channels: u16,
}

impl CaptureSession {
    /// Stop capturing and return the recorded audio buffer.
    pub fn stop(self) -> AudioBuffer {
        drop(self.stream); // Stops the stream

        let samples_raw = self
            .buffer
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();

        // Mix down to mono if stereo
        let samples = if self.channels > 1 {
            mix_to_mono(&samples_raw, self.channels)
        } else {
            samples_raw
        };

        let buf = AudioBuffer {
            samples,
            sample_rate: self.sample_rate,
        };

        info!(
            sample_rate = self.sample_rate,
            channels = self.channels,
            duration_secs = buf.duration_secs(),
            samples = buf.samples.len(),
            "Capture stopped"
        );

        buf
    }
}

fn mix_to_mono(interleaved: &[f32], channels: u16) -> Vec<f32> {
    let ch = channels as usize;
    interleaved
        .chunks(ch)
        .map(|frame| frame.iter().sum::<f32>() / ch as f32)
        .collect()
}

fn get_device_name(device: &cpal::Device) -> String {
    #[allow(deprecated)]
    device.name().unwrap_or_else(|_| "unknown".to_string())
}

/// List available audio input devices.
pub fn list_devices() -> Vec<AudioDeviceInfo> {
    let host = cpal::default_host();
    let default_name = host
        .default_input_device()
        .map(|d| get_device_name(&d))
        .unwrap_or_default();

    let mut devices = Vec::new();
    if let Ok(input_devices) = host.input_devices() {
        for device in input_devices {
            let name = get_device_name(&device);
            devices.push(AudioDeviceInfo {
                is_default: name == default_name,
                name,
            });
        }
    }
    devices
}

/// Start capturing audio from the default input device.
pub fn start_capture() -> Result<CaptureSession, CaptureError> {
    let host = cpal::default_host();
    let device = host.default_input_device().ok_or(CaptureError::NoDevice)?;

    let device_name = get_device_name(&device);

    // Negotiate config: prefer 48kHz mono
    let default_config = device
        .default_input_config()
        .map_err(|e| CaptureError::ConfigError(e.to_string()))?;

    let sample_rate = default_config.sample_rate();
    let channels = default_config.channels();
    let sample_format = default_config.sample_format();

    info!(
        device = %device_name,
        sample_rate,
        channels,
        format = ?sample_format,
        "Starting audio capture"
    );

    let config: cpal::StreamConfig = default_config.into();
    let buffer: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
    let buffer_clone = buffer.clone();

    let err_fn = |err: cpal::StreamError| {
        error!(%err, "Audio capture stream error");
    };

    let stream = device
        .build_input_stream(
            &config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if let Ok(mut buf) = buffer_clone.lock() {
                    buf.extend_from_slice(data);
                }
            },
            err_fn,
            None,
        )
        .map_err(|e| CaptureError::StreamError(e.to_string()))?;

    stream
        .play()
        .map_err(|e| CaptureError::StreamError(e.to_string()))?;

    Ok(CaptureSession {
        stream,
        buffer,
        sample_rate,
        channels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mix_to_mono_averages_channels() {
        let stereo = vec![1.0, 0.0, 0.5, 0.5, 0.0, 1.0];
        let mono = mix_to_mono(&stereo, 2);
        assert_eq!(mono.len(), 3);
        assert!((mono[0] - 0.5).abs() < f32::EPSILON);
        assert!((mono[1] - 0.5).abs() < f32::EPSILON);
        assert!((mono[2] - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn list_devices_does_not_panic() {
        // Should not panic even if no audio device is available
        let _devices = list_devices();
    }
}
