# M7: Dictation Modes

Toggle mode, VAD mode, and noise suppression. The three dictation activation methods
are all functional.

**Depends on:** M6 (system tray, permissions, audio feedback, mic management)

**Delivers:** All three dictation modes work (hold-to-dictate from M3, plus toggle and
VAD). Noise suppression is configurable. Per-mode hotkeys and dictation mode selection
(raw vs. formatted) are available.

---

## Phase 1: Toggle Mode

Add tap-on / tap-off dictation alongside the existing hold-to-dictate.

### Work Items

1. **Activation mode abstraction**
   - Refactor `src-tauri/src/hotkey/mod.rs` to support multiple activation modes
   - Enum: `ActivationMode::Hold | Toggle | Vad`
   - Hold mode: existing behavior (press → start, release → stop)
   - Toggle mode: first press → start, second press → stop
   - Mode is read from settings (default: Hold)

2. **Toggle mode implementation**
   - On hotkey press:
     - If idle → transition to Recording, start capture
     - If recording → transition to Transcribing, stop capture, process
   - Release event is ignored in toggle mode
   - Visual feedback: tray icon and overlay show recording state so user knows
     when to press again

3. **Mode switching**
   - Settings UI: activation mode selector (Hold / Toggle / VAD)
   - Switching modes takes effect immediately (no restart)
   - If mode changes while recording: stop current recording cleanly, return to idle

4. **Verify:**
   - Toggle mode: tap to start, tap to stop, transcription runs
   - Switching between hold and toggle in settings works immediately
   - Visual indicators correctly reflect toggle recording state

---

## Phase 2: Voice Activity Detection (VAD)

Add always-listening mode that starts transcribing automatically when speech is detected.

### Work Items

1. **Add voice_activity_detector dependency**
   - Add `voice_activity_detector` 0.2.x to `Cargo.toml`
   - Re-run `cargo vendor vendor/`
   - **ONNX Runtime provisioning (strict policy):**
     - Pin the ORT version matching the `ort` crate's expectations
     - Download the prebuilt ORT binary once, verify its checksum, and cache it
       in CI artifacts and in a documented local path for dev machines
     - Set `ORT_LIB_LOCATION` environment variable pointing to the cached binary
     - **Builds MUST NOT download ORT at build time** -- if `ORT_LIB_LOCATION`
       is missing or points to a non-existent path, the build must fail with a
       clear error message directing the developer to `./scripts/agent/bootstrap`
     - `./scripts/agent/bootstrap` handles ORT setup: download, checksum verify,
       place in the expected location, export `ORT_LIB_LOCATION`
     - CI caches the ORT binary alongside `vendor/` -- no network access during
       `cargo build`

2. **VAD integration**
   - Create `src-tauri/src/audio/vad.rs`
   - Initialize Silero VAD model (bundled ONNX weights via the crate)
   - Process audio frames through VAD: returns speech probability per frame
   - **Dual-threshold hysteresis:**
     - Start threshold (higher): speech probability must exceed this to begin
       recording (e.g., 0.6). Prevents triggering on ambient noise.
     - Stop threshold (lower): speech probability must drop below this to end
       recording (e.g., 0.3). Prevents premature cutoff during pauses.
     - Sensitivity setting (low / medium / high) adjusts both thresholds as a
       pair, maintaining the hysteresis gap.
   - **Minimum speech duration:** ignore speech segments shorter than 300ms
     (rejects coughs, clicks, single-syllable false triggers)
   - **Minimum silence duration:** require silence to persist for the full
     silence cutoff period before stopping (prevents stop on brief pauses)
   - **Cooldown after finalize:** after transcription completes, suppress VAD
     triggers for 500ms to avoid re-triggering on audio feedback chimes
   - **Feedback audio suppression:** if audio feedback (start/stop chimes) is
     enabled, mute VAD detection for the duration of chime playback to prevent
     the app's own sounds from triggering dictation
   - VAD runs on the continuous audio stream, not on saved buffers

3. **VAD mode implementation**
   - When VAD mode is active:
     1. Continuously capture audio from the microphone (background stream)
     2. Feed frames to VAD
     3. On speech start: transition to Recording, begin buffering
     4. On speech end (silence exceeds cutoff): transition to Transcribing
     5. Process buffered audio through pipeline and engine
     6. Return to listening
   - VAD must not be active on app launch -- user explicitly enables it
   - **Pre-roll buffer:** maintain a rolling circular buffer of the most recent
     300–500ms of audio (at capture sample rate, before resampling). When VAD
     triggers speech-start, prepend the pre-roll buffer to the recording so the
     beginning of the utterance is not clipped. Buffer is capped at 500ms
     (≈48,000 samples at 48kHz mono = ~192KB) to bound memory usage.

4. **VAD pause/resume hotkey**
   - Dedicated hotkey for pausing/resuming VAD (privacy panic)
   - When paused: VAD stops listening, mic capture paused, tray icon shows paused state
   - When resumed: VAD restarts listening
   - Pausing does not exit VAD mode -- it's a temporary mute
   - Hotkey is configurable in settings (separate from the main dictation hotkey)
   - **Pause-state authority and conflict rules:**
     - There is one global "paused" state (shared between tray Pause menu item
       from M6 and the VAD panic hotkey). Both toggle the same flag.
     - When paused: all dictation modes are suspended (hold/toggle hotkeys
       ignored, VAD stops listening). Tray icon shows paused state.
     - When global hotkey is disabled in settings: hold/toggle modes are
       unavailable but VAD mode still works (VAD does not use the global
       hotkey). The VAD pause hotkey remains active independently.
     - If the user presses the dictation hotkey while VAD is mid-recording:
       ignore the press (VAD owns the session). Log a warning.
     - Per-mode hotkey conflicts: if the user assigns the same key combo to
       both the dictation hotkey and the VAD pause hotkey, reject the
       assignment with a validation error in the UI.

5. **Resource management**
   - VAD mode keeps the microphone stream open continuously
   - **CPU budget:** VAD idle loop (mic capture + VAD inference, no active
     transcription) must stay under 3% CPU on a modern machine. Measure and
     log the actual idle CPU usage during development.
   - **Battery awareness:** show a warning in settings when enabling VAD mode:
     "VAD mode keeps the microphone active continuously, which uses more
     battery than hold or toggle modes."
   - **Idle telemetry:** log a periodic `vad_idle_load` metric (every 60s)
     recording CPU % and audio buffer pressure, visible in `./scripts/agent/logs`
   - Stop the continuous stream when switching away from VAD mode
   - When the OS reports low battery (if detectable), consider emitting a
     notification suggesting the user switch to hold/toggle mode

6. **Verify:**
   - VAD mode detects speech and starts recording automatically
   - Silence after speech triggers transcription
   - Pause hotkey instantly mutes VAD (no audio processing)
   - Resume hotkey restarts listening
   - Pre-roll buffer captures the start of speech without cutoff
   - VAD does not activate on non-speech sounds (test with music, typing)

### Files Created

```
src-tauri/src/audio/vad.rs      # Silero VAD wrapper, speech detection
```

---

## Phase 3: Noise Suppression Controls and Silence Cutoff

Make noise suppression configurable and add adjustable silence cutoff.

### Work Items

1. **Noise suppression settings**
   - Denoise was implemented in M2 as a binary toggle
   - Add aggressiveness levels: off / light / moderate / aggressive
   - Map levels to nnnoiseless parameters: `nnnoiseless` does not expose a
     built-in aggressiveness knob. Implement levels by varying the number of
     consecutive denoise passes per frame (light=1, moderate=2, aggressive=3)
     and/or by scaling the output gain. Document the chosen mapping in code
     comments. If multi-pass introduces unacceptable latency (>2ms per frame),
     fall back to gain-only differentiation.
   - UI: slider or dropdown in settings
   - Default: moderate

2. **Adjustable silence cutoff**
   - Configure how long a pause counts as "done speaking"
   - Range: 0.5s to 5.0s (default: 1.5s)
   - **Per-mode stop semantics:**

     | Mode   | Primary stop      | Silence auto-stop | "Disable auto-stop" meaning |
     |--------|-------------------|-------------------|-----------------------------|
     | Hold   | Release key       | N/A               | N/A (release always stops)  |
     | Toggle | Second tap        | Optional (off by default) | Second tap is the only stop trigger |
     | VAD    | Silence threshold | Always on (required) | Not allowed -- VAD requires silence detection to function |

   - Toggle mode: silence auto-stop is opt-in. When enabled, silence OR a
     second tap will stop recording. When disabled, only a second tap stops.
   - VAD mode: silence auto-stop cannot be disabled (it IS the stop mechanism).
     The cutoff duration is adjustable but always active.
   - Hold mode: silence cutoff setting is ignored (release = stop).
   - UI: slider in settings with label showing the current value. Slider is
     hidden/disabled when the selected mode makes it irrelevant.

3. **Per-mode hotkey configuration**
   - Allow different hotkeys for different modes:
     - Main dictation hotkey (hold or toggle)
     - VAD pause/resume hotkey
   - Option to disable the global hotkey entirely (for VAD-only users)
   - UI: separate hotkey capture fields per mode in settings

4. **Punctuation toggle**
   - Most Whisper models handle punctuation natively
   - Add a setting: "Auto-punctuate" (default: on)
   - When off: strip punctuation from transcription output (raw mode)
   - When on: keep model-native punctuation (formatted mode)
   - This is the foundation for raw vs. formatted dictation modes

5. **Dictation mode selection (raw vs. formatted)**
   - Setting: dictation mode (raw / formatted)
   - Raw: auto-punctuate off, no capitalization correction
   - Formatted: auto-punctuate on, sentence capitalization
   - Switchable per-dictation or globally in settings
   - UI: mode indicator in tray menu and overlay

6. **Settings persistence and migration**
   - All new M7 settings must be persisted via the M5 settings infrastructure:
     - `activation_mode` (Hold / Toggle / VAD) — default: Hold
     - `vad_sensitivity` (low / medium / high) — default: medium
     - `silence_cutoff_seconds` (0.5–5.0) — default: 1.5
     - `toggle_auto_stop_enabled` (bool) — default: false
     - `noise_suppression_level` (off / light / moderate / aggressive) — default: moderate
     - `auto_punctuate` (bool) — default: true
     - `dictation_mode` (raw / formatted) — default: formatted
     - `vad_pause_hotkey` (key combo) — default: platform-specific TBD
     - `global_hotkey_enabled` (bool) — default: true
   - **Migration:** add a schema migration (M5 pattern) that inserts default
     values for all new settings. Existing databases upgraded on first launch
     after M7 update. Missing keys fall back to defaults in code (defensive).

7. **Verify:**
   - Noise suppression aggressiveness is adjustable and audibly different
   - Silence cutoff controls when auto-stop happens in VAD and toggle modes
   - Per-mode hotkeys work independently
   - Raw mode produces verbatim output without punctuation
   - Formatted mode produces punctuated, capitalized output
   - All new settings persist and take effect immediately

---

## Acceptance Criteria

M7 is complete when all of the following are true:

- [ ] Toggle mode works: tap to start, tap to stop
- [ ] VAD mode works: speech auto-detected, silence triggers transcription
- [ ] VAD pre-roll buffer prevents start-of-speech cutoff
- [ ] VAD pause/resume hotkey works (privacy panic)
- [ ] VAD is not active on app launch (explicit opt-in only)
- [ ] Activation mode is switchable in settings (Hold / Toggle / VAD)
- [ ] Noise suppression has adjustable aggressiveness (off / light / moderate / aggressive)
- [ ] Silence cutoff is configurable (0.5s to 5.0s, or disabled)
- [ ] Per-mode hotkeys are configurable
- [ ] Global hotkey can be disabled entirely
- [ ] Raw dictation mode produces verbatim output
- [ ] Formatted dictation mode produces punctuated, capitalized output
- [ ] ONNX Runtime provisioning uses cached binary via `ORT_LIB_LOCATION` (no build-time download)
- [ ] `./scripts/agent/bootstrap` sets up ORT binary with checksum verification
- [ ] Vendor directory updated with voice_activity_detector and ort crates
- [ ] All new settings persisted via M5 infrastructure with schema migration
- [ ] Settings missing from DB fall back to documented defaults
- [ ] VAD idle CPU usage logged and under 3% budget
- [ ] VAD false triggers: tested with music, typing, coughing -- none trigger recording
- [ ] Pre-roll buffer captures first 300–500ms of speech (no audible clipping)
- [ ] Hotkey conflict validation rejects duplicate key assignments
- [ ] Tray pause and VAD panic hotkey share a single paused state
- [ ] Mode-switch latency and transcription latency logged as structured events
- [ ] All new UI strings externalized in i18n resource files
- [ ] All new UI components are keyboard navigable and screen-reader accessible
- [ ] `./scripts/agent/check` passes
- [ ] CI builds pass on all platforms
