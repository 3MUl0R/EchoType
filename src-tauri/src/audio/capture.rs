use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use tracing::{error, info, warn};

use super::{AudioBuffer, AudioDeviceInfo};

/// Maximum capture duration in seconds (10 minutes).
const MAX_CAPTURE_SECONDS: u32 = 600;

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
    had_error: Arc<AtomicBool>,
}

impl CaptureSession {
    /// Peek at the current accumulated buffer without stopping capture.
    /// Returns a clone of the current mono audio data.
    pub fn peek_buffer(&self) -> AudioBuffer {
        let samples_raw = self
            .buffer
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();

        let samples = if self.channels > 1 {
            mix_to_mono(&samples_raw, self.channels)
        } else {
            samples_raw
        };

        AudioBuffer {
            samples,
            sample_rate: self.sample_rate,
        }
    }

    /// Stop capturing and return the recorded audio buffer.
    pub fn stop(self) -> AudioBuffer {
        // Brief drain period to let the hardware callback deliver its final
        // buffer.  Without this, fast key-up can shave off the last ~10-50 ms.
        std::thread::sleep(std::time::Duration::from_millis(50));
        drop(self.stream); // Stops the stream

        if self.had_error.load(Ordering::Relaxed) {
            warn!("Audio stream reported errors during capture");
        }

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

/// Write callback that converts samples to f32 and appends to buffer.
/// Enforces a maximum buffer size to prevent unbounded memory growth.
fn write_input_data<T>(data: &[T], buffer: &Arc<Mutex<Vec<f32>>>, max_samples: usize)
where
    T: cpal::Sample,
    f32: cpal::FromSample<T>,
{
    if let Ok(mut buf) = buffer.lock() {
        let remaining_capacity = max_samples.saturating_sub(buf.len());
        if remaining_capacity == 0 {
            return;
        }
        let take = data.len().min(remaining_capacity);
        buf.extend(
            data[..take]
                .iter()
                .map(|&s| <f32 as cpal::FromSample<T>>::from_sample_(s)),
        );
    }
}

/// Resolve a named input device, falling back to default if needed.
fn resolve_device(
    host: &cpal::Host,
    name: Option<&str>,
    auto_fallback: bool,
) -> Result<cpal::Device, CaptureError> {
    if let Some(name) = name {
        if let Ok(devices) = host.input_devices() {
            for device in devices {
                if get_device_name(&device) == name {
                    info!(device = %name, "Using selected microphone");
                    return Ok(device);
                }
            }
        }

        if auto_fallback {
            warn!(
                requested = %name,
                "Selected microphone not found, falling back to default"
            );
        } else {
            return Err(CaptureError::NoDevice);
        }
    }

    host.default_input_device().ok_or(CaptureError::NoDevice)
}

/// Start capturing audio from the default input device.
#[allow(dead_code)]
pub fn start_capture() -> Result<CaptureSession, CaptureError> {
    start_capture_with_device(None, true)
}

/// Start capturing audio from a specific device (or default if None).
/// If `auto_fallback` is true and the named device isn't found, falls back to default.
pub fn start_capture_with_device(
    device_name: Option<&str>,
    auto_fallback: bool,
) -> Result<CaptureSession, CaptureError> {
    let host = cpal::default_host();
    let device = resolve_device(&host, device_name, auto_fallback)?;

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
    let had_error = Arc::new(AtomicBool::new(false));
    let error_flag = had_error.clone();

    // Max samples = rate * channels * max_seconds
    let max_samples = (sample_rate as usize) * (channels as usize) * (MAX_CAPTURE_SECONDS as usize);

    let err_fn = move |err: cpal::StreamError| {
        error!(%err, "Audio capture stream error");
        error_flag.store(true, Ordering::Relaxed);
    };

    // Build the stream with the correct sample format callback
    let stream = match sample_format {
        SampleFormat::F32 => {
            let buffer_clone = buffer.clone();
            device.build_input_stream(
                &config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if let Ok(mut buf) = buffer_clone.lock() {
                        let remaining = max_samples.saturating_sub(buf.len());
                        if remaining > 0 {
                            let take = data.len().min(remaining);
                            buf.extend_from_slice(&data[..take]);
                        }
                    }
                },
                err_fn,
                None,
            )
        }
        SampleFormat::I16 => {
            let buffer_clone = buffer.clone();
            device.build_input_stream(
                &config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    write_input_data(data, &buffer_clone, max_samples);
                },
                err_fn,
                None,
            )
        }
        SampleFormat::I32 => {
            let buffer_clone = buffer.clone();
            device.build_input_stream(
                &config,
                move |data: &[i32], _: &cpal::InputCallbackInfo| {
                    write_input_data(data, &buffer_clone, max_samples);
                },
                err_fn,
                None,
            )
        }
        format => {
            return Err(CaptureError::ConfigError(format!(
                "unsupported sample format: {format:?}"
            )));
        }
    }
    .map_err(|e| CaptureError::StreamError(e.to_string()))?;

    stream
        .play()
        .map_err(|e| CaptureError::StreamError(e.to_string()))?;

    Ok(CaptureSession {
        stream,
        buffer,
        sample_rate,
        channels,
        had_error,
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

    #[test]
    fn resolve_nonexistent_device_with_fallback() {
        let host = cpal::default_host();
        // A nonexistent device name with auto_fallback=true should not error
        // (falls back to default, which may or may not exist)
        let _ = resolve_device(&host, Some("nonexistent_device_xyz"), true);
    }

    #[test]
    fn resolve_nonexistent_device_without_fallback() {
        let host = cpal::default_host();
        let result = resolve_device(&host, Some("nonexistent_device_xyz"), false);
        assert!(result.is_err());
    }

    #[test]
    fn resolve_none_uses_default() {
        let host = cpal::default_host();
        // None device name should use default (may or may not exist on CI)
        let _ = resolve_device(&host, None, true);
    }
}
