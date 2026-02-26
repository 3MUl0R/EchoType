# M6: System Integration

System tray, OS permissions, visual and audio feedback, microphone management. The app
becomes a proper desktop citizen -- lives in the tray, handles permissions gracefully,
gives clear feedback during dictation.

**Depends on:** M5 (settings persistence, settings UI)

**Delivers:** App runs from system tray with no dock/taskbar presence. Guided permission
flows. Visual and audio indicators during dictation. Mic selection and health monitoring.

---

## Phase 1: System Tray

Move the app's persistent presence from a window to the system tray.

### Work Items

1. **Tray icon setup**
   - Enable the `tray-icon` feature on the `tauri` crate (should already be in Cargo.toml)
   - Create tray icon assets: idle state, recording state, processing state
   - Register the tray icon on app startup in `main.rs` / `lib.rs`
   - Icon states: idle (default), recording (active/highlighted), transcribing (busy)

2. **Tray menu**
   - Right-click / secondary-click context menu:
     - "Open Settings" → open the settings window
     - "Open History" → open the history window
     - Separator
     - "Pause" / "Resume" → toggle dictation availability
     - Separator
     - "Quit EchoType"
   - Menu items update dynamically based on app state

3. **Window management**
   - On app startup: no visible window, tray icon only
   - Settings/History/Models open as separate windows (or a single window with tabs)
   - Clicking the tray icon: open the main window (or toggle visibility)
   - Closing the window: hide it, don't quit the app
   - "Quit" only from tray menu or keyboard shortcut
   - macOS: remove from Dock (`LSUIElement` in Info.plist or Tauri equivalent).
     When opening windows from tray, use `NSApp.activate(ignoringOtherApps: true)` to
     bring the window to front and receive focus despite being an agent app.
   - Windows: remove from taskbar when minimized to tray
   - Linux: handle tray icon visibility per desktop environment

4. **Linux tray fallback**
   - Detect available tray protocol: XDG StatusNotifierItem (modern) vs. legacy X11 tray
   - GNOME: tray support removed in stock GNOME, needs AppIndicator extension
   - Fallback: if no tray available, keep a small persistent window
   - Log which tray protocol is in use for diagnostics

5. **Verify:**
   - App starts with tray icon, no window visible
   - Tray menu opens and all items work
   - Clicking tray icon opens the main window
   - Closing window hides it (app continues running)
   - "Quit" terminates the app

### Files Created/Modified

```
src-tauri/src/tray.rs           # tray icon setup, menu, state management
src-tauri/icons/
  tray-idle.png
  tray-recording.png
  tray-processing.png
```

---

## Phase 2: OS Permission Handling

Replace the minimal permission check from M3 with full guided flows.

### Work Items

1. **Permission detection module**
   - Expand `src-tauri/src/platform/permissions.rs`:
   - **Microphone permission** (all platforms):
     - macOS: check via `AVCaptureDevice.authorizationStatus`
     - Windows: check microphone privacy settings
     - Linux: typically no permission gate (ALSA/PulseAudio access is default)
   - **Accessibility permission** (macOS only):
     - Check via `AXIsProcessTrustedWithOptions`
     - Can prompt the OS dialog with `kAXTrustedCheckOptionPrompt`
   - **Wayland portal permissions** (Linux Wayland):
     - Global shortcuts may require portal authorization
     - Check via `ashpd` crate or direct portal D-Bus calls

2. **Guided permission flows**
   - Create `src/lib/components/PermissionGuide.svelte`
   - Per-permission step:
     1. Explain what the permission is for (in plain language)
     2. Show platform-specific instructions with screenshots or descriptions
     3. "Open System Settings" button (deep link to the relevant settings pane)
     4. Poll for permission status, update UI when granted
     5. "Skip" option with explanation of reduced functionality
   - macOS deep links:
     - Microphone: `x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone`
     - Accessibility: `x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility`

3. **Graceful fallback behavior**
   - Missing Accessibility (macOS): auto-switch to ClipboardOnly output, show banner
   - Missing microphone: disable recording, show clear error with fix instructions
   - **Wayland degraded mode:** global shortcuts and focus detection may both fail
     on Wayland. If portal shortcut registration fails AND X11 fallback unavailable:
     - Disable global hotkey (user must use the UI button to start/stop dictation)
     - Use ClipboardOnly insertion mode
     - Show persistent banner explaining the limitations and suggesting X11 session
   - **Platform capability matrix (M6):**
     - macOS: full (tray + permissions + overlay + all insertion modes)
     - Windows: full (tray + overlay + all insertion modes)
     - Linux X11: full (tray varies by DE, overlay, all insertion modes)
     - Linux Wayland: degraded (tray varies, no global hotkey without portal,
       ClipboardOnly, no focus lock, no overlay focus guarantee)

4. **Permission status in settings**
   - Show current permission status in Settings page
   - Status indicators use both color AND text/icon (not color-only) for accessibility
   - "Fix" button re-opens the guided flow for any missing permission

5. **Extend settings for M6 features**
   - Add typed settings keys for all new configurable values:
     - `overlay_enabled` (bool, default: true)
     - `audio_feedback_enabled` (bool, default: true)
     - `audio_feedback_device` (string, default: system default)
     - `audio_feedback_volume` (float 0.0–1.0, default: 0.5)
     - `selected_mic_device` (string, default: system default)
     - `mic_auto_fallback` (bool, default: true)
   - All new settings persisted in SQLite via M5's settings infrastructure
   - All settings take effect at runtime without restart

5. **Verify:**
   - On macOS without Accessibility: app falls back to clipboard mode, shows guide
   - On macOS without microphone: app shows clear error and instructions
   - Permission guide deep-links open the correct system settings pane
   - Granting permission updates the UI without restart

---

## Phase 3: Visual and Audio Feedback

Give the user clear feedback about dictation state.

### Work Items

1. **Tray icon state animation**
   - Idle: default tray icon
   - Recording: highlighted/red tray icon (update on dictation state change)
   - Transcribing: pulsing or alternate icon
   - Update the tray icon when dictation state machine transitions

2. **Optional overlay indicator**
   - Small floating indicator near the cursor or screen corner
   - Shows dictation state: recording dot, processing spinner
   - Configurable: enabled/disabled in settings (default: enabled)
   - Overlay must not steal focus from the target application
   - Platform implementation: borderless, always-on-top, click-through window

3. **Audio feedback**
   - Bundle audio assets: start chime, stop chime (short, unobtrusive sounds)
   - **Timing to avoid mic capture:**
     - Start chime: play *before* audio capture begins (pre-roll). Brief delay
       between hotkey press and capture start -- chime plays in that window.
       Start chime must complete before mic stream opens.
     - Stop chime: play *after* audio capture has stopped (mic is closed, safe)
   - Use `rodio` for playback (already available from M2)
   - Configurable in settings:
     - Enable/disable audio feedback
     - Output device selection (play on specific device, e.g., headphones)
     - Volume control (independent from system volume)

4. **Verify:**
   - Tray icon changes during dictation
   - Overlay appears during recording (if enabled)
   - Start/stop chimes play at correct times
   - Audio feedback respects output device and volume settings
   - Overlay doesn't steal focus

### Files Created

```
src-tauri/src/feedback.rs        # visual + audio feedback coordination
src/lib/components/
  DictationOverlay.svelte        # floating overlay indicator
assets/audio/
  start.wav
  stop.wav
```

---

## Phase 4: Microphone Management

Let users select, switch, and monitor their microphone.

### Work Items

1. **Microphone selection UI**
   - Expand Settings with a microphone section
   - Dropdown listing all available input devices (from cpal enumeration)
   - Current device highlighted
   - "Test" button: record a short sample, play it back, show input level meter
   - Persist selected device in settings
   - Hot-switching: changing the device takes effect immediately (no restart)

2. **Runtime microphone health**
   - Monitor input level during recording (RMS or peak amplitude)
   - Detect clipping (samples hitting max amplitude) → log warning
   - Detect silence (no input above threshold for extended period) → log warning
   - Device disconnect detection: if the selected device disappears:
     - Emit an event to the frontend
     - Show a notification/toast: "Microphone disconnected"
     - Optionally auto-fallback to the system default device
     - Persist the auto-fallback preference in settings

3. **Input level indicator**
   - Small level meter visible during recording (in overlay or tray tooltip)
   - Shows real-time input amplitude
   - Helps users verify their mic is working before a long dictation

4. **Verify:**
   - Mic selection dropdown shows all system input devices
   - Switching mics takes effect immediately
   - Mic test records and plays back audio
   - Disconnecting a USB mic triggers the disconnect alert
   - Auto-fallback to default device works when enabled
   - Input level indicator responds to sound

### Files Created/Modified

```
src/lib/components/
  MicrophoneSettings.svelte
  InputLevelMeter.svelte
src-tauri/src/audio/capture.rs   # modified: device switching, health monitoring
```

---

## Acceptance Criteria

M6 is complete when all of the following are true:

- [ ] App runs from system tray with no dock/taskbar presence
- [ ] Tray menu provides quick access to settings, history, pause, and quit
- [ ] Tray icon reflects dictation state (idle, recording, transcribing)
- [ ] Linux tray works with StatusNotifierItem; GNOME fallback to persistent window
- [ ] macOS: guided flow for Accessibility and Microphone permissions
- [ ] Missing Accessibility auto-switches to ClipboardOnly with explanation
- [ ] Missing microphone shows clear error with instructions
- [ ] Wayland degraded mode: clear notification of limitations, UI button fallback
- [ ] Permission status visible in settings (not color-only indicators)
- [ ] Visual overlay shows dictation state without stealing focus from target app
- [ ] Audio start chime completes before mic opens; stop chime plays after mic closes
- [ ] Audio feedback respects output device and volume settings
- [ ] All new settings (overlay, feedback, mic device, volume) persist across restarts
- [ ] Microphone selection dropdown with hot-switching
- [ ] Mic test: record, playback, and input level display
- [ ] Device disconnect detection with notification and optional auto-fallback
- [ ] Input level indicator visible during recording
- [ ] New UI components are keyboard navigable and screen-reader accessible
- [ ] All new strings externalized through i18n
- [ ] `./scripts/agent/check` passes
- [ ] CI builds pass on all platforms
