# M2: Audio & Transcription

Microphone capture, audio processing pipeline, and whisper-rs integration behind an
engine abstraction trait. The core capability: speak into the mic, see your words on
screen.

**Depends on:** M1 (project scaffold, logging, agent scripts, CI)

**Delivers:** A button in the UI that records audio, transcribes it via Whisper, and
displays the result. End-to-end proof that the audio-to-text pipeline works.

---

## Phase 1: STT Engine Trait

Define the abstraction that all speech-to-text engines implement. This trait is
introduced now with only Whisper behind it, but M10 adds cloud implementations against
the same interface.

### Work Items

1. **Define the engine trait**
   - Create `src-tauri/src/engine/mod.rs` with:
     ```rust
     #[async_trait]
     pub trait SttEngine: Send + Sync {
         fn name(&self) -> &str;
         async fn transcribe(&self, request: TranscribeRequest) -> Result<Transcription>;
         fn supported_languages(&self) -> Vec<Language>;
         fn model_info(&self) -> Option<ModelInfo>;
     }
     ```
   - `TranscribeRequest` struct: `audio: Vec<f32>`, `sample_rate: u32`,
     `language: Option<Language>`, `timestamps: bool`
   - `Transcription` struct: text, language detected, duration, optional word timestamps
   - `ModelInfo` struct: name, size, speed tier, accuracy tier (used by M4 model UI)
   - `Language` enum or string-based identifier
   - The trait takes raw f32 audio samples -- preprocessing (denoise, resample) happens
     before the engine sees the audio
   - Async to support both blocking Whisper (via `spawn_blocking`) and cloud engines (M10)
   - Result type uses a project-level error enum (not anyhow at the boundary)

2. **Engine manager**
   - `src-tauri/src/engine/manager.rs`: holds the active engine instance
   - Methods: `load_engine()`, `active_engine()`, `unload()`
   - Thread-safe: engine manager behind `Arc<Mutex<>>` or similar
   - Only one engine loaded at a time (Whisper models are large)

3. **Verify:** Trait compiles, manager compiles, `cargo test` passes.

### Files Created

```
src-tauri/src/engine/
  mod.rs          # SttEngine trait, Transcription, Language
  manager.rs      # EngineManager
```

---

## Phase 2: Audio Capture

Record audio from the microphone using cpal. This phase gets raw audio into a buffer.

### Work Items

1. **Add audio crate dependencies**
   - Add to `Cargo.toml`: `cpal` 0.17.x, `rodio` 0.22.x, `rubato` 0.16.x,
     `nnnoiseless` 0.5.x
   - Re-run `cargo vendor vendor/` from repo root to pick up new crates
   - Commit updated vendor directory

2. **Audio capture module**
   - Create `src-tauri/src/audio/capture.rs`
   - Enumerate available input devices via cpal
   - **Config negotiation:** query the device's supported configs, prefer 48kHz mono.
     If 48kHz is unavailable, accept the device's default sample rate and resample later.
     If mono is unavailable, accept stereo and mix down to mono.
   - Open a stream on the default (or selected) input device with the negotiated config
   - Capture audio into a growable `Vec<f32>` per capture session
   - Handle sample format conversion (cpal may deliver i16, u16, or f32)
   - Session-based API: `start_capture() -> CaptureSession`,
     `stop_capture(session) -> AudioBuffer`. Each session is independent.
   - Log the actual negotiated sample rate and channel count via tracing

3. **Capture Tauri commands**
   - Expose `list_audio_devices` as a Tauri command
   - Returns list of input device names and IDs
   - Frontend can display these (used properly in M6 mic selection UI)
   - Expose `start_capture` and `stop_capture` as Tauri commands for frontend use
   - `stop_capture` returns a session handle that can be passed to `transcribe_audio`

4. **Verify:**
   - `list_audio_devices` command returns the system's microphones
   - `start_capture()` / `stop_capture()` captures a non-empty audio buffer
   - Config negotiation selects a valid format on the default device
   - Unit test: mock or real capture produces expected sample rate and channel count

### Files Created

```
src-tauri/src/audio/
  mod.rs
  capture.rs      # cpal integration, device enumeration, recording
```

---

## Phase 3: Audio Pipeline

Denoise and resample the captured audio to prepare it for Whisper.

### Work Items

1. **Noise suppression**
   - Create `src-tauri/src/audio/denoise.rs`
   - Wrap `nnnoiseless::DenoiseState`
   - Process audio in 480-sample frames (10ms at 48kHz) as nnnoiseless requires
   - **Amplitude scaling:** nnnoiseless expects samples in 16-bit amplitude range
     (-32768..32767). Scale f32 samples (assumed -1.0..1.0) by multiplying by 32767
     before processing, then scale back after denoising.
   - **Tail handling:** if the audio length isn't a multiple of 480, zero-pad the
     final frame and trim the output to the original length
   - **First-frame artifacts:** the first frame of nnnoiseless output may contain
     transient artifacts. Document this and consider discarding the first frame or
     pre-feeding a silent frame on initialization.
   - Input: 48kHz f32 mono → Output: 48kHz f32 mono (denoised)
   - Configurable: enabled/disabled toggle (default: enabled)
   - Processing is synchronous and fast (pure Rust, no allocation per frame)

2. **Resampling**
   - Create `src-tauri/src/audio/resample.rs`
   - Use `rubato` to downsample 48kHz → 16kHz (Whisper's expected rate)
   - If capture negotiated a rate other than 48kHz, adjust the resampling ratio
   - Configure a `SincFixedIn` or `FftFixedIn` resampler
   - Process the full denoised buffer in one pass (not streaming yet)
   - **Tail flushing:** rubato buffers internal state; flush remaining samples after
     the last chunk to avoid truncation on short recordings
   - **Short recordings:** handle edge case where audio is shorter than one rubato
     chunk (pad or process as a single partial chunk)
   - Output: 16kHz f32 mono, ready for Whisper

3. **Audio pipeline orchestration**
   - Create `src-tauri/src/audio/pipeline.rs`
   - `process(raw: AudioBuffer) -> AudioBuffer`:
     1. Denoise (if enabled)
     2. Resample 48kHz → 16kHz
     3. Return processed buffer
   - The pipeline is a pure function on audio buffers -- no side effects

4. **Audio playback (verification)**
   - Create `src-tauri/src/audio/playback.rs`
   - Use `rodio` to play back an audio buffer through the default output device
   - Expose as a Tauri command: `play_audio(buffer)` -- used for development testing
     and later for dictation history audio playback (M5)

5. **Verify:**
   - Record audio → denoise → resample → playback sounds correct
   - Output buffer is 16kHz mono f32
   - Denoiser removes audible background noise
   - Unit test: pipeline produces expected sample rate and frame count

### Files Created

```
src-tauri/src/audio/
  denoise.rs      # nnnoiseless wrapper
  resample.rs     # rubato 48kHz → 16kHz
  pipeline.rs     # orchestration: denoise → resample
  playback.rs     # rodio playback for verification
```

---

## Phase 4: Whisper Integration

Integrate whisper-rs to transcribe processed audio into text.

### Work Items

1. **Add whisper-rs dependency**
   - Add `whisper-rs` 0.15.x to `Cargo.toml` with `tracing_backend` feature enabled
     (routes whisper.cpp logs into our tracing pipeline per tech stack spec)
   - For M2, use CPU-only (no GPU feature flags yet -- GPU settings come in M5)
   - Re-run `cargo vendor vendor/` from repo root -- this will pull in whisper-rs-sys
     and the whisper.cpp C source
   - Note: vendoring whisper.cpp source means builds compile it from C. Requires a C
     compiler on the build machine (cc, clang, or MSVC)

2. **Whisper engine implementation**
   - Create `src-tauri/src/engine/whisper.rs`
   - Implement `SttEngine` for `WhisperEngine`
   - Call `whisper_rs::install_logging_hooks()` on initialization to connect
     whisper.cpp logging to the tracing subsystem
   - `WhisperEngine::new(model_path)`: load a `.bin` GGML model file
   - `transcribe()`: wrap blocking Whisper inference in `spawn_blocking` to satisfy
     the async trait. Create WhisperState, set params (language, etc.), run inference.
   - Extract text segments from Whisper output
   - Map to `Transcription` struct
   - `model_info()`: return model name, file size, speed/accuracy tier
   - Log transcription duration via tracing

3. **Development model**
   - For development, use `ggml-tiny.en.bin` (~75MB, fast, English-only)
   - Download manually for now (M4 builds the proper download manager)
   - Store in a known path (Tauri app data dir / `models/`)
   - Document the manual download step in the milestone's dev notes

4. **Transcribe Tauri command**
   - Expose `transcribe_audio` as a Tauri command
   - Takes a capture session handle (returned by `stop_capture`) to identify
     which audio buffer to transcribe -- no implicit "last buffer" state
   - Runs the audio pipeline (denoise → resample) then passes to the engine
   - Returns the transcription text to the frontend
   - Runs on a background thread (transcription can take seconds on CPU)

5. **Verify:**
   - Whisper loads the tiny model without error
   - Speaking into the mic and triggering transcribe returns recognizable text
   - Transcription time is logged
   - `cargo test` passes (whisper tests may need to be gated behind a feature flag
     or ignored in CI if the model file isn't present)

### Files Created

```
src-tauri/src/engine/
  whisper.rs      # WhisperEngine implements SttEngine
```

---

## Phase 5: Basic Transcription UI

Wire everything together with a minimal frontend that lets you click to record, then
see the transcription result.

### Work Items

1. **Recording UI component**
   - Create `src/lib/components/RecordButton.svelte`
   - Large, obvious button: "Hold to Record" (or click to start/stop for now)
   - Button states: idle, recording (with visual indicator), processing
   - On recording start: call `start_capture` Tauri command (receives session handle)
   - On recording stop: call `stop_capture(session)`, then `transcribe_audio(session)`
   - Show a simple recording timer while active

2. **Transcription display component**
   - Create `src/lib/components/TranscriptionDisplay.svelte`
   - Shows the transcription result text
   - Shows metadata: transcription duration, model used
   - Scrollable if text is long
   - Empty state: helpful prompt ("Record something to see it transcribed")

3. **Wire into App.svelte**
   - Main page layout: RecordButton centered, TranscriptionDisplay below
   - Use Tailwind for styling
   - All user-facing strings through i18n helper

4. **ONNX Runtime: not in M2**
   - The `voice_activity_detector` crate (needed in M7) depends on `ort` which needs
     ONNX Runtime. Do NOT add `voice_activity_detector` to Cargo.toml in M2.
   - Only vendor what's actually used. VAD and its ONNX dependency arrive in M7.

5. **Verify (end-to-end):**
   - Click record → speak → click stop → see transcription text on screen
   - Audio pipeline runs (denoise, resample) and logs confirm it
   - Whisper transcription result appears in the UI
   - All strings are externalized
   - `./scripts/agent/check` still passes
   - CI builds still pass (model file won't be in CI, so whisper tests should be
     conditional)

### Files Created

```
src/lib/components/
  RecordButton.svelte
  TranscriptionDisplay.svelte
src/App.svelte              # modified: wire in new components
src/lib/i18n/en.json        # modified: add new strings
```

---

## Acceptance Criteria

M2 is complete when all of the following are true:

- [ ] `SttEngine` trait is defined with async `transcribe`, `model_info`, `supported_languages`
- [ ] `WhisperEngine` implements `SttEngine` using `spawn_blocking`
- [ ] `EngineManager` can load and unload a Whisper model
- [ ] Audio capture negotiates device config (prefers 48kHz mono, falls back gracefully)
- [ ] Capture uses session-based API (no implicit shared buffer state)
- [ ] Audio pipeline denoises (nnnoiseless with correct amplitude scaling) and resamples (rubato) to 16kHz
- [ ] Whisper transcribes the processed audio and returns text
- [ ] Frontend record button triggers capture → pipeline → transcription → display
- [ ] Transcription result appears in the UI with duration metadata
- [ ] Audio playback command works (for dev verification)
- [ ] Device enumeration Tauri command lists available microphones
- [ ] Vendor directory at repo root updated with new crate sources
- [ ] `./scripts/agent/check` passes
- [ ] CI builds pass on all platforms (whisper model tests gated appropriately)
- [ ] Structured logs show audio capture, pipeline, and transcription events
- [ ] whisper.cpp logs route through tracing (`tracing_backend` feature enabled)
- [ ] RecordButton and TranscriptionDisplay are keyboard navigable and screen-reader accessible
- [ ] Graceful error handling: no mic available, permission denied, mic disconnect mid-capture,
  missing model file, empty capture buffer
