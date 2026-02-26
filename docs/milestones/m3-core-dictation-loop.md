# M3: Core Dictation Loop

Global hotkey, hold-to-dictate flow, and text insertion. This is the milestone where
EchoType becomes a dictation tool -- hold a key anywhere, speak, release, text appears.

**Depends on:** M2 (audio capture, pipeline, whisper integration, engine trait)

**Delivers:** Hold a global hotkey in any application, speak, release, transcribed text
is inserted at the cursor position. The core product works.

---

## Phase 1: Global Hotkey

Register a system-wide hotkey that EchoType responds to regardless of which application
is focused.

### Work Items

1. **Add tauri-plugin-global-shortcut**
   - Add `tauri-plugin-global-shortcut` 2.3.x to `Cargo.toml` and frontend deps
   - Add the plugin's JS package: `@tauri-apps/plugin-global-shortcut`
   - Re-run `cargo vendor vendor/`
   - Register the plugin in `main.rs` / `lib.rs`
   - Add `global-shortcut:allow-register` and `global-shortcut:allow-unregister`
     to capabilities

2. **Hotkey registration**
   - Create `src-tauri/src/hotkey/mod.rs`
   - Register a default hotkey (e.g., `Ctrl+Shift+Space` / `Cmd+Shift+Space`)
   - Handle both **press** and **release** events (required for hold-to-dictate)
   - Emit Tauri events on press and release: `dictation:start`, `dictation:stop`
   - Hotkey is hardcoded for now -- configurable hotkey comes in M5 (settings)

3. **Hotkey lifecycle**
   - Register hotkey on app startup
   - Unregister on app shutdown
   - Handle registration failure gracefully (log error, notify frontend)
   - If another app holds the shortcut, show a clear error

4. **Verify:**
   - With EchoType running, the global hotkey triggers in any focused application
   - Press event fires on key down, release event fires on key up
   - Events are visible in frontend console

### Files Created

```
src-tauri/src/hotkey/
  mod.rs          # hotkey registration, press/release event emission
```

---

## Phase 2: Dictation State Machine

Orchestrate the full dictation flow: press → record → release → transcribe → insert.

### Work Items

1. **Dictation state machine**
   - Create `src-tauri/src/dictation/mod.rs`
   - States: `Idle` → `Recording` → `Transcribing` → `Inserting` → `Idle`
   - Transitions triggered by hotkey events and completion callbacks
   - State changes emit Tauri events so the frontend can reflect them
   - Error/edge states:
     - `TranscriptionFailed` → notify frontend, return to `Idle`
     - `InsertionFailed` → notify frontend, return to `Idle`
     - `Cancelled` → user pressed cancel hotkey or Escape during recording
     - Duplicate press while recording → ignore (debounce)
     - Release without meaningful audio (< 0.3s) → skip transcription, return to `Idle`
     - Permission denied (mic or Accessibility) → notify frontend, return to `Idle`
   - Only one dictation session active at a time (new press during transcribing is ignored)

2. **Hold-to-dictate flow**
   - On hotkey press (`dictation:start`):
     1. Transition to `Recording`
     2. Start audio capture (from M2)
     3. Capture focus lock target (Phase 3)
   - On hotkey release (`dictation:stop`):
     1. Stop audio capture → get audio buffer
     2. Transition to `Transcribing`
     3. Spawn background task: run audio pipeline → transcribe via engine trait
     4. On completion: transition to `Inserting` → insert text (Phase 4)
     5. Transition to `Idle`

3. **Frontend state display**
   - Update `App.svelte` to listen for dictation state events
   - Show current state: idle, recording, transcribing
   - During recording: show duration timer
   - During transcribing: show spinner/indicator
   - On completion: briefly flash the transcribed text

4. **Verify:**
   - Hold hotkey → recording starts → release → transcription runs → state returns to idle
   - State transitions fire in correct order
   - Frontend reflects each state change in real time

### Files Created

```
src-tauri/src/dictation/
  mod.rs          # DictationState, state machine, event emission
```

---

## Phase 3: Focus Lock and Permission Check

Capture the target window when dictation starts. Check for required OS permissions.

### Work Items

1. **Focus lock module**
   - Create `src-tauri/src/platform/focus.rs`
   - On dictation start, capture:
     - macOS: frontmost app bundle ID via `NSWorkspace` (via objc crate or raw FFI)
     - Windows: `GetForegroundWindow` handle via `windows` crate
     - Linux X11: `_NET_ACTIVE_WINDOW` via xcb or x11rb
     - Linux Wayland: focus detection is severely limited by design. Log a warning
       and skip focus lock. Text insertion uses clipboard-only mode on Wayland as
       the primary fallback. Document this as a known platform limitation.
   - Store the captured reference in the dictation state
   - On text insertion, re-focus the captured window if focus has moved
   - **Cursor position:** M3 captures the target *window* only, not cursor position
     within that window. Cursor position is preserved by re-focusing the window (the
     OS maintains cursor position within a window). This is sufficient for most apps.
   - **Windows note:** `SetForegroundWindow` may fail due to Windows foreground
     lock policies. If refocus fails, fall back to clipboard-only mode with a status
     notification. Do not retry in a loop.
   - Platform-specific code behind `#[cfg(target_os = "...")]` blocks

2. **Permission check (macOS Accessibility)**
   - Create `src-tauri/src/platform/permissions.rs`
   - On macOS: check Accessibility permission via
     `AXIsProcessTrustedWithOptions`
   - If not granted: log a warning, emit event to frontend with clear user notification
   - **Auto-fallback: if Accessibility is missing, switch output method to
     `ClipboardOnly` (NOT clipboard+paste)**. Clipboard+paste still uses enigo to
     simulate Cmd+V, which also requires Accessibility on macOS. ClipboardOnly copies
     text to clipboard and notifies the user to paste manually.
   - Full guided permission flow comes in M6 -- this is the minimal safety net

3. **Platform abstraction**
   - Create `src-tauri/src/platform/mod.rs` as the platform layer entry point
   - Re-export focus and permissions modules
   - Each platform function is conditional-compiled per OS
   - **Platform capability matrix for M3:**
     - macOS + Accessibility: full (direct input + focus lock)
     - macOS - Accessibility: clipboard-only + no focus lock, user notified
     - Windows: full (direct input + focus lock), with refocus fallback
     - Linux X11: full (direct input + focus lock)
     - Linux Wayland: clipboard-only + no focus lock (platform limitation, user notified)

4. **Verify:**
   - On macOS: focus lock captures the correct bundle ID
   - On macOS without Accessibility: output method silently falls back to clipboard
   - Focus lock re-activates the original window before text insertion
   - Permission state is logged at startup

### Files Created

```
src-tauri/src/platform/
  mod.rs            # platform abstraction entry
  focus.rs          # focused window detection per OS
  permissions.rs    # OS permission checks (Accessibility on macOS)
```

---

## Phase 4: Text Insertion

Insert transcribed text at the cursor position in the target application.

### Work Items

1. **Add insertion crate dependencies**
   - Add `enigo` 0.6.x and `arboard` 3.4.x to `Cargo.toml`
   - `arboard`: enable `wayland-data-control` feature for Linux Wayland
   - Re-run `cargo vendor vendor/`

2. **Text insertion module**
   - Create `src-tauri/src/output/mod.rs`
   - Three output methods (configurable, but default to direct input):

   **Direct input (`enigo`):**
   - Type text character by character via simulated keystrokes
   - Handle Unicode properly (enigo supports this on most platforms)
   - Requires Accessibility permission on macOS

   **Clipboard + paste (`arboard`):**
   - Save current clipboard contents
   - Write transcription text to clipboard
   - Simulate Cmd+V / Ctrl+V paste keystroke via enigo
   - **Conditional restore:** after paste delay, check if clipboard still contains the
     text we inserted. Only restore original contents if it does. If the user has
     copied something else in the meantime, do not overwrite their new clipboard.
   - Requires Accessibility on macOS (enigo for Cmd+V). Not the macOS fallback.

   **Clipboard only (`arboard`):**
   - Write transcription text to clipboard
   - Do not paste -- user pastes manually
   - Simplest and most portable

3. **Output method selection**
   - Enum: `OutputMethod::DirectInput | ClipboardPaste | ClipboardOnly`
   - Default: `DirectInput` (if Accessibility/permissions available),
     `ClipboardOnly` (fallback when permissions are missing)
   - `ClipboardPaste` is opt-in for users who prefer it (available when permissions exist)
   - Hardcoded default for now -- user selection comes in M5 (settings)

4. **Integration with dictation state machine**
   - When transcription completes:
     1. Re-focus the captured target window (focus lock from Phase 3)
     2. Wait a brief moment for focus to settle
     3. Insert text via the selected output method
     4. Transition to `Idle`
   - If insertion fails, log the error and transition to `Idle` anyway (don't hang)

5. **Latency measurement**
   - Log timestamps at each stage: hotkey release, pipeline start, transcription start,
     transcription end, insertion start, insertion end
   - Compute and log total end-to-end latency
   - This is the baseline for the latency cross-cutting concern

6. **Verify:**
   - Hold hotkey in a text editor → speak → release → text appears at cursor
   - Direct input types the text character by character
   - Clipboard+paste inserts text and restores clipboard
   - Clipboard-only puts text on clipboard without pasting
   - Focus lock returns to the correct window after transcription
   - End-to-end latency is logged

### Files Created

```
src-tauri/src/output/
  mod.rs          # OutputMethod enum, insert_text(), clipboard ops
```

---

## Acceptance Criteria

M3 is complete when all of the following are true:

- [ ] Global hotkey registers and fires press/release events system-wide
- [ ] Hold-to-dictate flow works: hold → record → release → transcribe → insert
- [ ] Dictation state machine transitions correctly through all states
- [ ] Edge cases handled: duplicate press ignored, very short recording skipped,
  cancel during recording returns to idle
- [ ] Direct input mode types text at the cursor via simulated keystrokes
- [ ] Clipboard+paste mode inserts text; conditionally restores prior clipboard
- [ ] Clipboard-only mode copies text without pasting
- [ ] Focus lock captures the target window on dictation start
- [ ] Text is inserted into the original window even if focus moved during transcription
- [ ] macOS without Accessibility: falls back to ClipboardOnly, user notified
- [ ] Linux Wayland: falls back to ClipboardOnly, user notified of limitations
- [ ] Windows refocus failure: falls back to ClipboardOnly gracefully
- [ ] Frontend displays current dictation state (idle, recording, transcribing)
- [ ] End-to-end latency is measured and logged in structured format
  (hotkey release → text inserted, with per-stage timestamps)
- [ ] `./scripts/agent/logs` output is parseable (cross-cutting)
- [ ] Vendor directory updated with new crates
- [ ] `./scripts/agent/check` passes
- [ ] CI builds pass on all platforms
