# EchoType - Implementation Milestones

This document defines the implementation milestones for EchoType. Each milestone builds
on the previous one and produces a working, testable increment of the application.

The milestones are ordered for incremental buildup -- not component-by-component
isolation. At the end of every milestone, the app runs, the agent scripts work, and the
test suite passes. Nothing ships broken.

Reference documents:
- Product spec: [product-spec.md](product-spec.md)
- Tech stack: [tech-stack.md](tech-stack.md)
- AI operator guide: [ai-operator.md](ai-operator.md)

---

## Cross-Cutting Concerns

These are not milestones. They are standards established in M1 and enforced through
every subsequent milestone. Each milestone's acceptance criteria must verify these.

### AI-First Development

The agent command contract (`./scripts/agent/bootstrap`, `dev`, `check`, `logs`, `fix`)
is established in M1 and maintained through every subsequent milestone. Each milestone
must leave the agent workflow functional: an AI operator should be able to clone,
bootstrap, run, check, and read logs at any point in the project's history.

Machine-readable logging (structured JSON via `tracing`) is set up in M1. Diagnostics
grow richer as capabilities land, but the format and access pattern stay stable.

Per-milestone acceptance check: `./scripts/agent/check` passes, `./scripts/agent/logs`
returns parseable output, all commands exit with deterministic codes.

### Accessibility

Accessibility is built in from the start, not retrofitted. Every UI surface introduced
in any milestone must ship with:
- Keyboard navigation (no mouse-only interactions)
- Semantic HTML and ARIA roles
- Focus management (logical tab order, focus trapping in modals)
- Screen reader compatibility (VoiceOver, NVDA, Orca)

M10 performs a comprehensive audit and fills gaps, but the baseline is set from M1.

### Internationalization

UI strings are externalized from M1. Every milestone that adds UI surfaces must use the
string externalization pattern (framework TBD in M1). M12 completes the i18n story with
translation tooling, but no milestone should hardcode user-facing strings.

### Latency

The ultra-low-latency pipeline (product spec section 1) is a cross-cutting performance
target. Starting in M3 (when the dictation loop exists), each milestone must:
- Measure end-to-end dictation latency (hotkey release → text inserted)
- Log latency metrics in structured format
- Not regress latency without justification

### Engine Abstraction

An STT engine trait is introduced in M2 with Whisper as the sole implementation. All
milestones that touch transcription code against the trait, not the Whisper
implementation directly. M9 adds cloud implementations to the same trait.

---

## M1: Project Foundation

Tauri 2 + Svelte 5 project scaffold with build system, vendored dependencies, CI
pipeline, agent scripts, and structured logging.

**Delivers:** App launches (empty window), builds pass in CI on all platforms, agent
scripts work end-to-end, cross-cutting standards are in place.

**Key work:**
- Initialize Tauri 2 project with Svelte 5 + Vite frontend
- Tailwind CSS v4 setup (`@tailwindcss/vite`)
- Rust workspace with `cargo vendor` and `.cargo/config.toml`
- Structured logging with `tracing` (daily rotation, JSON format, 3-day cleanup)
- Agent scripts: `bootstrap`, `dev`, `check`, `logs`, `fix`
- Deterministic exit codes and parseable output from all agent scripts
- GitHub Actions CI: build matrix (macOS, Windows, Linux), `cargo audit`, `bun pm audit`
- Rust unit test scaffolding (`cargo test`), Vitest setup, Playwright config
- `clap` CLI skeleton (parse `--version`, `--help` for now)
- String externalization pattern for UI (i18n scaffolding)
- Accessibility baseline: semantic HTML structure, keyboard nav in shell layout

---

## M2: Audio & Transcription

Microphone capture, audio processing pipeline, and whisper-rs integration behind an
engine abstraction trait. The core capability: speak into the mic, see your words on
screen.

**Delivers:** A button in the UI that records audio, transcribes it via Whisper, and
displays the result. End-to-end proof that the audio-to-text pipeline works.

**Key work:**
- STT engine trait definition (async transcribe, model info, language support)
- Whisper engine implementation behind the trait (`whisper-rs`)
- Audio capture with `cpal` (enumerate devices, record to buffer)
- Audio pipeline: `nnnoiseless` (denoise at 48kHz) → `rubato` (resample to 16kHz)
- Bundle or side-load a small Whisper model for development
- Basic UI: record button, transcription result display
- Audio playback with `rodio` (verify capture quality)
- ONNX Runtime provisioning for `voice_activity_detector` build dependency

---

## M3: Core Dictation Loop

Global hotkey, hold-to-dictate flow, and text insertion. This is the milestone where
EchoType becomes a dictation tool -- hold a key anywhere, speak, release, text appears.

**Delivers:** Hold a global hotkey in any application, speak, release, transcribed text
is inserted at the cursor position. The core product works.

**Key work:**
- Global hotkey registration (`tauri-plugin-global-shortcut`, press + release events)
- Hold-to-dictate mode: start recording on press, stop and transcribe on release
- Minimal OS permission check: detect Accessibility (macOS), prompt or auto-fallback
  to clipboard mode if missing (full guided flow in M6)
- Text insertion via `enigo` (simulated keystrokes)
- Clipboard + paste fallback via `arboard` (save → write → paste → restore)
- Focus lock: capture target window reference on dictation start, insert there on finish
- Output method selection (direct input / clipboard+paste / clipboard-only)
- Dictation state machine (idle → recording → transcribing → inserting → idle)
- Latency measurement: log end-to-end time from hotkey release to text inserted

---

## M4: Model Management

Model catalog, download manager, and management UI. Users can browse, download, and
switch between Whisper models without touching the filesystem.

**Delivers:** Model management page in the UI. Browse available models with metadata
labels (size, speed tier, accuracy tier, supported languages), download with progress
bar, verify checksums, switch active model, delete unused models.

**Key work:**
- Static JSON model manifest (hosted in repo, fetched from GitHub)
- Model metadata: size on disk, relative speed, accuracy tier, supported languages
- Download manager: `reqwest` with streaming, progress reporting, resumable downloads
- Integrity verification: `sha2` + `hex` checksum comparison
- Model storage in Tauri app data directory
- Model management UI (browse catalog, download progress, installed models, delete)
- Model update workflow: detect newer version in manifest, prompt to re-download
- Engine switching: unload current model, load selected model (via engine trait)
- Language selection per model

---

## M5: Persistence & Settings

SQLite database, settings storage, dictation history, and settings UI. The app
remembers your preferences and past dictations.

**Delivers:** Settings persist across restarts. Dictation history with audio playback.
Configuration UI for all existing settings (hotkey, output method, model, language).
GPU backend selection in settings with hardware capability detection.

**Key work:**
- SQLite setup with `rusqlite` (`bundled` feature) and `rusqlite_migration`
- Schema: settings, dictation history (text + audio + metadata), model preferences
- Settings UI: hotkey configuration, output method, active model, language
- GPU backend selection in settings with hardware capability labeling
  (detect available backends, show which are supported on user's hardware)
- Dictation history UI: browse, replay audio, re-copy text, delete entries
- History retention policy: default last 5 with audio, configurable count (raise,
  lower, unlimited, disable entirely), optional time-based ceiling
- User-selectable database location
- Settings export/import (JSON)

---

## M6: System Integration

System tray, OS permissions, visual and audio feedback, microphone management. The app
becomes a proper desktop citizen -- lives in the tray, handles permissions gracefully,
gives clear feedback during dictation.

**Delivers:** App runs from system tray with no dock/taskbar presence. Guided permission
flows. Visual and audio indicators during dictation. Mic selection and health monitoring.

**Key work:**
- System tray icon and menu (Tauri `tray-icon` feature)
- Hide from dock/taskbar (tray-only presence)
- Linux tray fallback strategy (XDG StatusNotifierItem vs. legacy tray vs. GNOME)
- OS permission handling: microphone (all platforms), Accessibility (macOS),
  Wayland portal permissions for global hotkeys (Linux)
- Guided permission flows with platform-specific instructions and deep links to settings
- Graceful fallback (auto-switch to clipboard mode if Accessibility missing)
- Visual dictation feedback (tray animation or overlay)
- Audio feedback: start/stop chimes via `rodio`, configurable output device and volume
- Microphone selection UI with hot-switching
- Runtime mic health: input level indicator, clipping detection, device-disconnect
  alerts with optional auto-fallback to system default

---

## M7: Dictation Modes

Toggle mode, VAD mode, and noise suppression. The three dictation activation methods
are all functional.

**Delivers:** All three dictation modes work (hold-to-dictate from M3, plus toggle and
VAD). Noise suppression improves accuracy in noisy environments.

**Key work:**
- Toggle mode (tap-on / tap-off)
- VAD mode: Silero VAD (`voice_activity_detector`) for speech detection, auto-start/stop
- VAD pause/resume hotkey (privacy panic)
- Noise suppression toggle (`nnnoiseless` with configurable aggressiveness)
- Adjustable silence cutoff (configurable pause duration before auto-stop)
- Per-mode hotkey configuration (configurable hotkeys, disable global hotkey option)
- Punctuation auto-insertion toggle (raw output vs. model-native punctuation)
- Multiple dictation modes: raw (verbatim) vs. formatted (auto-capitalization,
  punctuation, paragraph breaks)

---

## M8: Dictation UX

Streaming transcription, edit-before-insert buffer, selection-aware replacement, and
auto-submit. The dictation experience becomes refined and complete.

**Delivers:** Streaming preview shows provisional text in real time. Edit buffer lets
you review before committing. Selected text is replaced by new dictation. Auto-submit
sends after insertion.

**Key work:**
- Streaming with replacement: show partial text, replace with final on completion
- Partial text visually distinct (lighter/faded) so user knows it's provisional
- Graceful failure: if final transcription fails, partial text remains as-is
- Edit-before-insert buffer (floating window, edit, confirm/discard)
- Selection-aware replacement (detect selection, replace on dictation complete)
- Auto-submit after insertion (Enter, Ctrl+Enter, Cmd+Enter -- platform-aware)

---

## M9: Power User Features

Per-application profiles, custom vocabulary, private mode, and CLI daemon/pipe modes.
EchoType becomes deeply configurable and scriptable.

**Delivers:** Profiles auto-switch based on focused application. Custom vocabulary
corrects domain-specific terms. CLI mode enables headless transcription for automation.

**Key work:**
- Focused window detection (platform APIs: NSWorkspace, GetForegroundWindow, xcb)
- Wayland limitation handling: detect at runtime, fall back to manual profile selection
  when compositor doesn't support window inspection
- Per-app profile matching (bundle ID on macOS, executable path on Windows/Linux)
- Profile configuration UI (bind mode, output method, vocabulary to apps)
- Custom vocabulary: word list management, post-transcription correction
- Private mode: toggle or hotkey, skip history storage
- CLI daemon mode (`echotype --daemon`, IPC via `interprocess`)
- CLI pipe mode (`echotype --stdout`, stream transcription to stdout)
- Mute system audio during dictation (optional toggle)

---

## M10: Cloud Engines

Cloud transcription providers with bring-your-own API keys. Strictly opt-in, user
controls everything. Cloud providers implement the same engine trait as local Whisper.

**Delivers:** Users can add API keys, select a cloud provider, and transcribe via cloud
instead of local Whisper. Engine selection UI shows local and cloud options side by side.

**Key work:**
- Cloud engine implementations behind the STT engine trait (Groq, OpenAI, Deepgram --
  or subset)
- API key management via `keyring` (platform keychain, masked display in UI)
- Engine selection UI (local models + cloud providers in one view)
- Cloud usage indicators (explicit opt-in confirmation, usage awareness)
- Error handling for cloud failures (timeout, auth, rate limits) with fallback to local

---

## M11: Metrics & Polish

Usage metrics dashboard, theme support, accessibility audit, and UI polish. The app
becomes pleasant to use and verified accessible.

**Delivers:** Metrics dashboard with real usage data. Dark, light, and high-contrast
themes. Comprehensive accessibility audit confirms all UI surfaces pass.

**Key work:**
- Real-time WPM display during dictation
- Metrics storage: total words, daily/weekly stats, per-engine breakdown
- Streaks and milestones (fun badges, discoverable in dashboard)
- Speaking vs. typing comparison (user sets typing baseline)
- Average WPM tracking (rolling + lifetime)
- Metrics available as text (not only charts) for screen readers
- Theme support: dark (default), light, high-contrast; respect OS text scaling
- Comprehensive accessibility audit across all UI surfaces (fill gaps from earlier
  milestones, verify keyboard nav, screen reader compat, focus management)
- Settings export/import polish

---

## M12: Distribution & Onboarding

First-run wizard, auto-updater, platform installers and package managers, code signing,
and diagnostic tooling. The app is ready for users to install and keep updated.

**Delivers:** Installable packages for all platforms. First-run wizard guides new users
through setup. Auto-updater keeps the app current. Package manager listings and signed
releases for trust.

**Key work:**
- First-run setup wizard (mic permission → Accessibility → model download → hotkey → test)
- Auto-updater (`tauri-plugin-updater`, signed GitHub Releases)
- Update channels per GPU flavor (CPU user never receives CUDA build)
- Update behavior setting (auto-update / download-and-prompt / notify-only)

---

## M13: Packaging & Release

Platform-specific packages, package manager listings, code signing, release signing,
project website, and diagnostic tooling.

**Delivers:** Downloadable signed packages on all platforms. Package manager installs
work. GitHub Pages site live. Diagnostic self-checks in settings.

**Key work:**
- Platform installers: `.msi` (Windows), `.dmg` (macOS), AppImage + `.deb` + `.rpm` (Linux)
- WinGet manifest (`winget install echotype`)
- Homebrew cask (`brew install --cask echotype`)
- Flatpak / Flathub listing
- Code signing: Apple notarization, Authenticode (certificate acquisition TBD)
- Signed releases: SHA256 checksums + GPG signatures alongside every binary
- GPU build matrix in CI (CPU default, Metal on macOS, CUDA/Vulkan as separate artifacts)
- GitHub Pages project website with custom domain
- Diagnostic logging: "Copy diagnostic info" button (OS, version, model, audio device, logs)
- "Test microphone" and "test transcription" self-checks in settings
- Internationalization completion: translation tooling, community contribution workflow

---

## Milestone Sequence

```
M1  Foundation
│
M2  Audio & Transcription
│
M3  Core Dictation Loop
│
M4  Model Management
│
M5  Persistence & Settings
│
M6  System Integration
│
├── M7  Dictation Modes
│
├── M8  Dictation UX
│
M9  Power User Features
│
M10 Cloud Engines
│
M11 Metrics & Polish
│
M12 Distribution & Onboarding
│
M13 Packaging & Release
```

Each milestone depends on the ones before it. M7 and M8 are sequential (modes before
UX refinements) but both gate on M6. Later milestones may reference capabilities from
any earlier milestone.

---

## Requirement Traceability

| Product Spec Section | Primary Milestone(s) |
|---------------------|---------------------|
| 1. Core Functionality | M3, M7, M8 |
| 2. Speech Engine Options | M2, M4, M10 |
| 3. Privacy & Architecture | Cross-cutting (all milestones) |
| 4. Local Database & Sync | M5 |
| 5. Metrics & Fun Telemetry | M11 |
| 6. UI & UX | M6, M11 |
| 7. Advanced Controls | M7, M8, M9 |
| 8. Future-Ready Hooks | Not in scope (architecture accommodates, no milestone) |
| 9. Distribution & Project | M12, M13 |
| 10. AI-First Development | Cross-cutting (M1 establishes, all maintain) |

---

*Each milestone will have its own detailed document with implementation phases, file-level
work breakdown, and acceptance criteria. This document is the high-level roadmap.*
