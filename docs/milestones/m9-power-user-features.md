# M9: Power User Features

Per-application profiles, custom vocabulary, private mode, and CLI daemon/pipe modes.
EchoType becomes deeply configurable and scriptable.

**Depends on:** M8 (streaming, edit buffer, selection replacement, auto-submit)

**Delivers:** Profiles auto-switch based on focused application. Custom vocabulary
corrects domain-specific terms. CLI mode enables headless transcription for automation.

---

## Phase 1: Per-Application Profiles

Automatically switch dictation configuration based on which application is focused.

### Work Items

1. **Profile data model**
   - Add `profiles` table to SQLite schema (migration v2):
     - `id` INTEGER PRIMARY KEY
     - `name` TEXT (user-friendly label, e.g., "VS Code", "Terminal")
     - `app_identifier` TEXT (bundle ID on macOS, exe path on Windows/Linux)
     - `dictation_mode` TEXT (raw / formatted)
     - `output_method` TEXT (direct_input / clipboard_paste / clipboard_only)
     - `language` TEXT
     - `auto_submit` BOOLEAN
     - `submit_key` TEXT
     - `custom_vocabulary_id` INTEGER (nullable FK)
     - `is_default` BOOLEAN (the fallback profile when no app match)
   - CRUD operations in `src-tauri/src/db/profiles.rs`

2. **Focused window detection**
   - Expand `src-tauri/src/platform/focus.rs` (created in M3):
   - **Typed `AppIdentifier` contract:**
     ```rust
     enum AppIdentifier {
         BundleId(String),   // macOS: "com.microsoft.VSCode"
         ExePath(PathBuf),   // Windows: "C:\Program Files\...\Code.exe"
         WmClass(String),    // Linux X11: "code" (WM_CLASS instance)
         Unknown,            // Wayland without extensions, or detection failure
     }
     ```
   - **Per-platform detection with fallback precedence:**
     - macOS: bundle ID via `NSRunningApplication` (primary). No fallback
       needed -- bundle ID is always available for GUI apps.
     - Windows: executable path via `GetModuleFileNameExW`. Normalize to
       lowercase for case-insensitive matching.
     - Linux (X11): try `_NET_WM_PID` → `/proc/{pid}/exe` first (→ `ExePath`).
       If unavailable, fall back to `WM_CLASS` instance name (→ `WmClass`).
       **Profile matching tries both**: match `ExePath` first, then `WmClass`.
     - Linux (Wayland): best-effort via compositor extensions
       (`wlr-foreign-toplevel-management`). If unavailable, return `Unknown`.
   - **Normalization rules:**
     - macOS bundle IDs: stored and matched case-insensitively
     - Windows exe paths: stored lowercase, matched case-insensitively
     - Linux WM_CLASS: stored and matched case-sensitively (X11 convention)
   - Return a `FocusedApp` struct containing `AppIdentifier` + display name

3. **Automatic profile switching**
   - On dictation start (or on focus change if VAD mode is active):
     1. Detect the focused application
     2. Match against stored profiles by `AppIdentifier` (try all variants
        for the platform -- e.g., on Linux X11, try `ExePath` then `WmClass`)
     3. If match: apply that profile's settings for this dictation
     4. If no match: use the default profile
   - **Immutable `DictationContext` snapshot:** at dictation start, freeze a
     `DictationContext` struct capturing: resolved profile, output method,
     vocabulary, private mode flag, focus target, language. All processing
     for this dictation session uses this snapshot. Focus changes during
     recording/transcription/edit-buffer do NOT alter the in-flight context.
   - Profile switch for the NEXT dictation happens only after the current one
     completes (insert or discard). The context snapshot is immutable.
   - **VAD continuous mode:** in VAD mode, each speech segment gets its own
     `DictationContext` snapshot. If focus changes between segments, the next
     segment uses the new profile. An in-flight segment is never affected.

4. **Profile management UI**
   - Create `src/lib/components/ProfileManager.svelte`
   - List existing profiles with their app associations
   - "Add Profile" flow:
     1. Click "Add" → focus the target application → EchoType detects its identifier
     2. Or manually enter an app identifier / choose from running apps
     3. Configure dictation settings for this profile
     4. Save
   - Edit and delete existing profiles
   - Set which profile is the default (fallback)
   - Display the detected app identifier for each profile

5. **Wayland limitation handling**
   - If running on Wayland without compositor extensions:
     - Auto-switching is unavailable
     - Show a message in profile settings explaining the limitation
     - Offer manual profile switching: tray menu item or hotkey to cycle profiles
   - Log the detection method in use for diagnostics

6. **Verify:**
   - Create a profile for a text editor with specific settings
   - Dictate in that editor: profile settings apply
   - Switch to another app: default profile applies
   - Profile management UI: add, edit, delete profiles
   - Wayland: graceful degradation with manual switching

### Files Created

```
src-tauri/src/db/profiles.rs
src-tauri/src/profiles/
  mod.rs          # profile matching, auto-switching logic
src/lib/components/
  ProfileManager.svelte
  ProfileEditor.svelte
```

---

## Phase 2: Custom Vocabulary

User-maintained word lists for correcting frequently misheard terms.

### Work Items

1. **Vocabulary data model**
   - Add `vocabularies` and `vocabulary_entries` tables (migration v2):
     - `vocabularies`: `id`, `name` (e.g., "Medical Terms", "Code")
     - `vocabulary_entries`: `id`, `vocabulary_id` (FK), `correction` TEXT
     - `vocabulary_aliases`: `id`, `entry_id` (FK), `misheard` TEXT
       (one entry can have multiple aliases / misheard variants)
   - A vocabulary is a named collection of correction entries, each with one
     or more misheard variants (aliases)
   - Multiple vocabularies can exist; profiles reference one at a time

2. **Post-transcription correction**
   - After the engine returns transcription text:
     1. Load the active vocabulary (from the current profile or global setting)
     2. **Build a sorted match list:** sort entries by `misheard` length
        descending (longest-match-first). This prevents shorter entries from
        matching inside longer ones (e.g., "echo" matching before "echo type").
     3. **Single-pass, non-recursive replacement:** scan the text left-to-right.
        For each position, try to match the longest entry. If matched, replace
        and advance past the replacement. Do NOT re-scan replaced text (prevents
        infinite loops from circular corrections).
     4. Matching is case-insensitive and word-boundary aware (don't replace
        substrings within larger words). Preserve the original case pattern
        of the matched text in the replacement where possible.
   - **Phonetic aliases (similar-sounding support):** each vocabulary entry
     can optionally include multiple `misheard` variants (aliases). For
     example, "EchoType" might have aliases: "echo type", "eco type",
     "ekko type". All aliases map to the same correction. This satisfies
     the product spec requirement for similar-sounding correction without
     requiring a phonetic matching engine.
   - Correction runs before text insertion (and before edit buffer if enabled)

3. **Vocabulary management UI**
   - Create `src/lib/components/VocabularyManager.svelte`
   - List vocabularies with entry counts
   - Edit vocabulary: table of misheard → correction pairs
   - Add/edit/delete entries
   - Import vocabulary from JSON file (array of `{ "correction": "...",
     "aliases": ["...", "..."] }` objects). Also accept simple TSV format
     (one pair per line, tab-separated `misheard\tcorrection`) for quick
     import -- TSV entries create single-alias entries.
   - Export vocabulary to JSON file (canonical format with aliases)

4. **Verify:**
   - Add a correction pair (e.g., "echo type" → "EchoType")
   - Dictate a sentence containing the misheard term → correction is applied
   - Correction respects word boundaries (doesn't replace inside longer words)
   - Vocabulary can be assigned to a profile

### Files Created

```
src-tauri/src/db/vocabularies.rs
src-tauri/src/vocabulary/
  mod.rs          # correction engine
src/lib/components/
  VocabularyManager.svelte
  VocabularyEditor.svelte
```

---

## Phase 3: Private Mode

Temporary mode that prevents dictation from being stored in history.

### Work Items

1. **Private mode toggle**
   - Setting: private mode on/off (default: off)
   - Toggle via: tray menu item, settings toggle, or dedicated hotkey
   - When active:
     - Dictation text is NOT saved to history
     - Audio recording is NOT saved to disk
     - Metrics for this dictation are NOT recorded
     - Everything else works normally (transcription, insertion)
   - Visual indicator: tray icon badge or overlay indicator showing private mode

2. **Private mode hotkey**
   - Configurable hotkey to toggle private mode
   - Separate from the dictation hotkey
   - Quick toggle without opening settings

3. **Implementation**
   - Check private mode flag in the dictation state machine
   - If private: skip the history save step, skip the audio file write
   - The `is_private` column in history table is for entries created before private
     mode was toggled mid-session (edge case)

4. **Verify:**
   - Enable private mode, dictate, verify nothing appears in history
   - Disable private mode, dictate, verify entry appears in history
   - Private mode indicator visible in tray/overlay
   - Private mode hotkey works

---

## Phase 4: CLI Daemon and Pipe Modes

Headless operation for power users and automation.

### Work Items

1. **CLI argument expansion**
   - Expand the `clap` CLI from M1:
     - `echotype` (no args): launch the GUI app (existing behavior)
     - `echotype --daemon`: start in daemon mode (no GUI)
     - `echotype --stdout`: pipe mode (transcription to stdout)
     - `echotype --list-models`: list installed models and exit
     - `echotype --set-model <id>`: set the active model and exit
     - `echotype --status`: query the running daemon's status

2. **Daemon mode**
   - Start EchoType without a GUI window
   - **Single-owner process model:** only one EchoType backend process runs at
     a time (either GUI or daemon, not both). The backend owns: hotkey
     registration, microphone, database writes, engine loading.
     - `echotype` (GUI): starts the full Tauri app with tray, windows, etc.
     - `echotype --daemon`: starts a headless backend (no Tauri webview).
     - If a GUI is already running, `--daemon` prints an error and exits.
     - If a daemon is already running, launching the GUI either: (a) connects
       to the daemon as an IPC client and provides a UI over it, or (b) asks
       the daemon to exit and takes over. Decision: option (b) for simplicity
       -- the daemon exits gracefully when the GUI starts.
   - **IPC protocol (versioned, structured):**
     - Transport: Unix domain socket (macOS/Linux) / named pipe (Windows)
       via `interprocess` crate
     - Socket path: Tauri app data dir / `echotype.sock`
     - **Message format:** newline-delimited JSON, one message per line
     - **Protocol version:** every message includes `"v": 1`. If the server
       receives a version it doesn't support, respond with an error and
       include the supported version range.
     - **Request/response:** each request includes a unique `"id"` field.
       Responses include the same `"id"` for correlation.
     - **Timeouts:** client times out after 5s if no response. Server drops
       connections idle for >60s.
     - **Commands:** `start`, `stop`, `status`, `set-model`, `set-mode`,
       `get-status`
     - **Stale socket cleanup:** on startup, if the socket file exists but no
       process is listening (connect fails), delete the stale socket and
       create a new one.
     - **Permissions:** socket created with mode 0600 (owner-only) on Unix.
   - Daemon responds to the same hotkeys as the GUI app
   - Daemon writes to the same log files

3. **Pipe mode**
   - Two distinct sub-modes (not ambiguous):
     - `echotype --stdout --once`: record one utterance, transcribe, print one
       line to stdout, exit with code 0. Useful for scripting single captures.
     - `echotype --stdout`: continuous mode. Use VAD to detect speech segments.
       Each segment is transcribed and printed as one line to stdout. Runs
       indefinitely until Ctrl+C (SIGINT → clean exit with code 0) or SIGTERM.
   - Each transcription result is one line on stdout (no trailing newline
     ambiguity -- always `\n`-terminated)
   - Errors go to stderr (never mixed with transcription output)
   - Pipe mode does not start the GUI or register global hotkeys
   - Pipe mode uses the default profile's settings (model, language, vocabulary)

4. **IPC client**
   - When `echotype --status` or similar is run while a daemon is already running:
     - Connect to the existing daemon's socket
     - Send the command, print the response, exit
   - If no daemon is running: print error and exit 1

5. **Verify:**
   - `echotype --daemon` starts without a window, hotkey works
   - `echotype --status` reports the daemon's state
   - `echotype --stdout` prints transcription to stdout
   - IPC commands work between CLI client and daemon
   - Daemon logs to the same location as the GUI

### Files Created

```
src-tauri/src/cli/
  mod.rs          # expanded CLI parsing and dispatch
  daemon.rs       # daemon mode (IPC server, headless operation)
  pipe.rs         # pipe mode (stdout streaming)
  ipc.rs          # IPC protocol (messages, socket handling)
```

---

## Phase 5: System Audio Muting

Optional muting of system audio during dictation.

### Work Items

1. **System audio mute**
   - Setting: "Mute system audio during dictation" (default: off)
   - On dictation start: mute the system output (or a selected output device)
   - On dictation end: restore the previous volume
   - Platform APIs:
     - macOS: CoreAudio `AudioObjectSetPropertyData` to set output mute
     - Windows: `IAudioEndpointVolume::SetMute` via WASAPI
     - Linux: `pactl set-sink-mute` or ALSA equivalent
   - Store the pre-mute volume level to restore exactly

2. **Verify:**
   - Enable the setting, start dictation, system audio mutes
   - End dictation, system audio restores to previous level
   - If the setting is off, system audio is not affected

---

## Acceptance Criteria

M9 is complete when all of the following are true:

- [ ] Per-app profiles auto-switch based on focused application
- [ ] Profile management UI: add, edit, delete, set default
- [ ] macOS: profile matching uses bundle ID (case-insensitive)
- [ ] Windows: profile matching uses executable path (case-insensitive)
- [ ] Linux X11: profile matching tries exe path first, then WM_CLASS fallback
- [ ] Linux Wayland: `Unknown` identifier handled gracefully (manual switching)
- [ ] Wayland: graceful degradation with manual profile switching
- [ ] Custom vocabulary corrects misheard terms in transcription output
- [ ] Vocabulary management UI: add/edit/delete entries, import/export
- [ ] Correction respects word boundaries and preserves case
- [ ] Private mode prevents history and audio storage
- [ ] Private mode toggle via tray, settings, and hotkey
- [ ] CLI daemon mode runs without GUI, responds to hotkeys and IPC
- [ ] Single-owner enforced: daemon exits when GUI starts (or vice versa)
- [ ] IPC protocol versioned, request IDs correlated, timeouts enforced
- [ ] Stale socket detected and cleaned up on startup
- [ ] CLI pipe mode: `--stdout --once` prints one line and exits
- [ ] CLI pipe mode: `--stdout` runs continuously with VAD, Ctrl+C exits cleanly
- [ ] IPC commands work between CLI client and running daemon
- [ ] Vocabulary replacement is longest-match-first, single-pass, non-recursive
- [ ] Vocabulary phonetic aliases work (multiple misheard → one correction)
- [ ] DictationContext snapshot is immutable: focus change during recording does not alter in-flight profile
- [ ] System audio mute during dictation works (when enabled)
- [ ] All new UI strings externalized in i18n resource files
- [ ] All new UI components are keyboard navigable and screen-reader accessible
- [ ] Profile switch and vocabulary correction latency logged as structured events
- [ ] Private mode suppresses metrics and history (verified: nothing stored)
- [ ] `./scripts/agent/check` passes
- [ ] CI builds pass on all platforms
