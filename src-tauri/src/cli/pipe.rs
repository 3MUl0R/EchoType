use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tracing::error;

use crate::audio::{capture, denoise, pipeline};
use crate::engine::whisper::WhisperEngine;
use crate::engine::{SttEngine, TranscribeRequest};

/// Minimum duration (seconds) to bother transcribing.
const MIN_DURATION: f64 = 0.3;

/// Run pipe mode: record → transcribe → print to stdout.
///
/// If `once` is true, do a single capture and exit.
/// Otherwise, run continuously until Ctrl+C.
pub fn run(once: bool) {
    let db_path = super::db_path();
    let db = crate::db::open(&db_path).expect("Cannot open database");

    // Load the active model
    let (model_path, suppression_level) = {
        let conn = db.blocking_lock();
        let model_id =
            crate::settings::get_typed::<String>(&conn, crate::settings::keys::ACTIVE_MODEL_ID)
                .unwrap_or_else(|_| {
                    eprintln!("No active model set. Use --set-model <id> or the GUI.");
                    std::process::exit(1);
                });

        let manifest = crate::models::manifest::load_bundled().expect("Cannot load model manifest");
        let entry = manifest
            .models
            .iter()
            .find(|m| m.id == model_id)
            .unwrap_or_else(|| {
                eprintln!("Model not found in manifest: {model_id}");
                std::process::exit(1);
            });

        let models_dir = super::models_dir_or_exit();
        let path = models_dir.join(&entry.file);
        if !path.exists() {
            eprintln!("Model file not found: {}", path.display());
            std::process::exit(1);
        }

        let level_str = crate::settings::get_typed::<String>(
            &conn,
            crate::settings::keys::NOISE_SUPPRESSION_LEVEL,
        )
        .unwrap_or_else(|_| "moderate".to_string());

        (path, denoise::SuppressionLevel::from_str(&level_str))
    };

    eprint!("Loading model... ");
    let engine = WhisperEngine::new(&model_path).unwrap_or_else(|e| {
        eprintln!("Failed: {e}");
        std::process::exit(1);
    });
    eprintln!("{}", engine.name());

    // Set up Ctrl+C handler
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc_handler(r);

    if once {
        // Single capture mode
        eprintln!("Recording... (press Ctrl+C to cancel)");
        let session = capture::start_capture_with_device(None, true).unwrap_or_else(|e| {
            eprintln!("Failed to start capture: {e}");
            std::process::exit(1);
        });

        // Wait for Ctrl+C to stop recording
        while running.load(Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        let buffer = session.stop();
        if buffer.duration_secs() < MIN_DURATION {
            eprintln!("Recording too short.");
            std::process::exit(0);
        }

        match transcribe(&engine, &buffer, suppression_level) {
            Ok(text) => {
                let text = apply_vocab(&db, &text);
                println!("{text}");
            }
            Err(e) => {
                eprintln!("Transcription failed: {e}");
                std::process::exit(1);
            }
        }
    } else {
        // Continuous mode: use silence detection to split utterances
        eprintln!("Listening... (press Ctrl+C to stop)");

        while running.load(Ordering::Relaxed) {
            let session = match capture::start_capture_with_device(None, true) {
                Ok(s) => s,
                Err(e) => {
                    error!(%e, "Failed to start capture");
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    continue;
                }
            };

            // Record until silence or Ctrl+C
            // Simple approach: record for a fixed window, then transcribe
            // We listen for 5 seconds at a time
            let mut silence_count = 0u32;
            let mut had_speech = false;

            loop {
                if !running.load(Ordering::Relaxed) {
                    let buffer = session.stop();
                    if had_speech && buffer.duration_secs() >= MIN_DURATION {
                        if let Ok(text) = transcribe(&engine, &buffer, suppression_level) {
                            let text = apply_vocab(&db, &text);
                            if !text.trim().is_empty() {
                                println!("{text}");
                            }
                        }
                    }
                    return;
                }

                std::thread::sleep(std::time::Duration::from_millis(200));

                // Check if we have audio with speech
                let snapshot = session.peek_buffer();
                // Check only the latest ~200ms of audio for silence detection
                let tail_samples = (snapshot.sample_rate as usize) / 5; // 200ms
                let tail = if snapshot.samples.len() > tail_samples {
                    &snapshot.samples[snapshot.samples.len() - tail_samples..]
                } else {
                    &snapshot.samples
                };
                let rms = compute_rms(tail);
                if rms > 0.01 {
                    had_speech = true;
                    silence_count = 0;
                } else if had_speech {
                    silence_count += 1;
                }

                // After ~1.5s of silence following speech, stop and transcribe
                if had_speech && silence_count >= 7 {
                    break;
                }
            }

            let buffer = session.stop();
            if buffer.duration_secs() >= MIN_DURATION {
                match transcribe(&engine, &buffer, suppression_level) {
                    Ok(text) => {
                        let text = apply_vocab(&db, &text);
                        if !text.trim().is_empty() {
                            println!("{text}");
                        }
                    }
                    Err(e) => {
                        eprintln!("Transcription error: {e}");
                    }
                }
            }
        }
    }
}

fn transcribe(
    engine: &WhisperEngine,
    buffer: &crate::audio::AudioBuffer,
    suppression_level: denoise::SuppressionLevel,
) -> Result<String, String> {
    let config = pipeline::PipelineConfig { suppression_level };
    let processed = pipeline::process(buffer, &config)?;
    let request = TranscribeRequest {
        audio: processed.samples,
        sample_rate: processed.sample_rate,
        language: None,
    };
    // Use a temporary tokio runtime to call the async transcribe method
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("Runtime error: {e}"))?;
    let result = rt
        .block_on(engine.transcribe(request))
        .map_err(|e| e.to_string())?;
    Ok(result.text)
}

fn apply_vocab(db: &crate::db::DbHandle, text: &str) -> String {
    let conn = db.blocking_lock();
    let custom_words: Vec<String> = crate::settings::get_typed(
        &conn,
        crate::settings::keys::CUSTOM_WORDS,
    )
    .unwrap_or_default();
    if custom_words.is_empty() {
        return text.to_string();
    }
    crate::dictation::vocabulary::apply_custom_words(
        text,
        &custom_words,
        crate::dictation::vocabulary::DEFAULT_THRESHOLD,
    )
}

fn compute_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    (sum / samples.len() as f64).sqrt() as f32
}

/// Register a Ctrl+C handler that sets the running flag to false.
fn ctrlc_handler(running: Arc<AtomicBool>) {
    // Store the flag globally so the signal handler can access it
    *RUNNING_FLAG.lock().unwrap() = Some(running);
    RUNNING.store(true, Ordering::SeqCst);

    // Spawn a thread that waits for the signal via a pipe trick
    std::thread::spawn(|| {
        // Use a simple self-pipe approach: block on reading from a pipe,
        // signal handler writes to it. But for simplicity, just poll the global.
        loop {
            std::thread::sleep(std::time::Duration::from_millis(50));
            if !RUNNING.load(Ordering::SeqCst) {
                if let Ok(guard) = RUNNING_FLAG.lock() {
                    if let Some(flag) = guard.as_ref() {
                        flag.store(false, Ordering::SeqCst);
                    }
                }
                break;
            }
        }
    });

    // Register platform-specific signal handlers
    #[cfg(unix)]
    unsafe {
        libc_signal_setup();
    }
}

static RUNNING: AtomicBool = AtomicBool::new(true);
static RUNNING_FLAG: std::sync::Mutex<Option<Arc<AtomicBool>>> = std::sync::Mutex::new(None);

/// Set up Unix signal handlers using raw syscalls.
#[cfg(unix)]
unsafe fn libc_signal_setup() {
    // SIGINT (Ctrl+C) and SIGTERM
    for sig in [2i32 /* SIGINT */, 15i32 /* SIGTERM */] {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = signal_handler as *const () as libc::sighandler_t;
        sa.sa_flags = libc::SA_RESTART;
        libc::sigaction(sig, &sa, std::ptr::null_mut());
    }
}

#[cfg(unix)]
extern "C" fn signal_handler(_sig: libc::c_int) {
    RUNNING.store(false, Ordering::SeqCst);
}
