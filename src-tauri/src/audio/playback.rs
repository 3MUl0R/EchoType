use std::io::Cursor;

use tracing::{error, info};

use super::AudioBuffer;

/// Play an audio buffer through the default output device.
pub fn play_audio(buffer: &AudioBuffer) -> Result<(), String> {
    if buffer.is_empty() {
        return Err("Cannot play empty audio buffer".to_string());
    }

    info!(
        sample_rate = buffer.sample_rate,
        samples = buffer.samples.len(),
        duration = buffer.duration_secs(),
        "Playing audio"
    );

    // Encode to WAV in memory
    let wav_data = encode_wav(buffer)?;

    // Play via rodio 0.22 API
    let sink = rodio::DeviceSinkBuilder::open_default_sink()
        .map_err(|e| format!("No audio output: {e}"))?;

    let cursor = Cursor::new(wav_data);
    let player = rodio::play(sink.mixer(), cursor).map_err(|e| {
        error!(%e, "Failed to play audio");
        format!("Play error: {e}")
    })?;

    player.sleep_until_end();

    Ok(())
}

/// Encode an AudioBuffer to WAV format in memory.
pub fn encode_wav(buffer: &AudioBuffer) -> Result<Vec<u8>, String> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: buffer.sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut cursor = Cursor::new(Vec::new());
    let mut writer =
        hound::WavWriter::new(&mut cursor, spec).map_err(|e| format!("WAV writer error: {e}"))?;

    for &sample in &buffer.samples {
        let scaled = (sample * 32767.0).clamp(-32768.0, 32767.0) as i16;
        writer
            .write_sample(scaled)
            .map_err(|e| format!("WAV write error: {e}"))?;
    }

    writer
        .finalize()
        .map_err(|e| format!("WAV finalize error: {e}"))?;

    Ok(cursor.into_inner())
}
