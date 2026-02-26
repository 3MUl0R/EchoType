use std::io::Cursor;

use tracing::{debug, warn};

const SAMPLE_RATE: u32 = 44100;

/// Type of audio feedback chime.
#[derive(Debug, Clone, Copy)]
pub enum Chime {
    /// Ascending tone: dictation started recording.
    Start,
    /// Descending tone: dictation stopped recording.
    Stop,
}

/// Play a feedback chime at the given volume (0.0–1.0). Non-blocking (spawns a thread).
pub fn play_chime(chime: Chime, volume: f32) {
    let volume = volume.clamp(0.0, 1.0);
    if volume < 0.01 {
        return;
    }

    std::thread::spawn(move || {
        if let Err(e) = play_chime_blocking(chime, volume) {
            warn!(%e, "Audio feedback chime failed");
        }
    });
}

fn play_chime_blocking(chime: Chime, volume: f32) -> Result<(), String> {
    let samples = generate_chime(chime, volume);
    let wav_data = encode_wav_i16(&samples, SAMPLE_RATE)?;

    let sink = rodio::DeviceSinkBuilder::open_default_sink()
        .map_err(|e| format!("No audio output: {e}"))?;

    let cursor = Cursor::new(wav_data);
    let player = rodio::play(sink.mixer(), cursor).map_err(|e| format!("Play error: {e}"))?;

    player.sleep_until_end();
    debug!(?chime, "Audio feedback chime played");

    Ok(())
}

/// Generate chime samples as i16.
fn generate_chime(chime: Chime, volume: f32) -> Vec<i16> {
    match chime {
        Chime::Start => {
            // Two-tone ascending: C5 (523 Hz) → E5 (659 Hz), ~150ms total
            let mut samples = Vec::new();
            samples.extend(tone(523.0, 0.075, volume));
            samples.extend(tone(659.0, 0.075, volume));
            samples
        }
        Chime::Stop => {
            // Two-tone descending: E5 (659 Hz) → C5 (523 Hz), ~150ms total
            let mut samples = Vec::new();
            samples.extend(tone(659.0, 0.075, volume));
            samples.extend(tone(523.0, 0.075, volume));
            samples
        }
    }
}

/// Generate a single sine tone with fade-in/out envelope.
fn tone(freq: f32, duration_secs: f32, volume: f32) -> Vec<i16> {
    let num_samples = (SAMPLE_RATE as f32 * duration_secs) as usize;
    let fade_samples = (num_samples / 5).max(1); // 20% fade

    (0..num_samples)
        .map(|i| {
            let t = i as f32 / SAMPLE_RATE as f32;
            let sine = (2.0 * std::f32::consts::PI * freq * t).sin();

            // Smooth fade-in/out envelope
            let envelope = if i < fade_samples {
                i as f32 / fade_samples as f32
            } else if i > num_samples - fade_samples {
                (num_samples - i) as f32 / fade_samples as f32
            } else {
                1.0
            };

            (sine * envelope * volume * 24000.0) as i16
        })
        .collect()
}

fn encode_wav_i16(samples: &[i16], sample_rate: u32) -> Result<Vec<u8>, String> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut cursor = Cursor::new(Vec::new());
    let mut writer =
        hound::WavWriter::new(&mut cursor, spec).map_err(|e| format!("WAV writer error: {e}"))?;

    for &sample in samples {
        writer
            .write_sample(sample)
            .map_err(|e| format!("WAV write error: {e}"))?;
    }

    writer
        .finalize()
        .map_err(|e| format!("WAV finalize error: {e}"))?;

    Ok(cursor.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_start_chime_is_not_empty() {
        let samples = generate_chime(Chime::Start, 0.5);
        assert!(!samples.is_empty());
    }

    #[test]
    fn generate_stop_chime_is_not_empty() {
        let samples = generate_chime(Chime::Stop, 0.5);
        assert!(!samples.is_empty());
    }

    #[test]
    fn tone_length_is_correct() {
        let samples = tone(440.0, 0.1, 1.0);
        let expected = (SAMPLE_RATE as f32 * 0.1) as usize;
        assert_eq!(samples.len(), expected);
    }

    #[test]
    fn zero_volume_chime_is_silent() {
        let samples = generate_chime(Chime::Start, 0.0);
        assert!(samples.iter().all(|&s| s == 0));
    }

    #[test]
    fn encode_wav_produces_valid_data() {
        let samples = tone(440.0, 0.05, 0.5);
        let wav = encode_wav_i16(&samples, SAMPLE_RATE).unwrap();
        // WAV header is 44 bytes + data
        assert!(wav.len() > 44);
    }
}
