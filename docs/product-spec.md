# EchoType - Product Specification

> Your voice, your machine, your rules.

EchoType is a desktop dictation tool that transcribes speech to text and pastes it
wherever your cursor is. It runs locally by default, stays out of your way, and never
phones home unless you tell it to. Built with Rust and Tauri for a razor-thin runtime
footprint.

---

## 1. Core Functionality

- **Global hotkey activation** - System-wide shortcut to start dictation. Three modes,
  togglable in settings:
  - *Hold-to-dictate* - Press and hold to record, release to finalize. (Default)
  - *Toggle (tap-on / tap-off)* - Tap once to start, tap again to stop. Better for
    longer dictation sessions or hands-free workflows.
  - *Voice Activity Detection (VAD)* - Always-listening mode that automatically starts
    transcribing when speech is detected and stops on silence. True hands-free
    operation with no hotkey required. Not active on launch -- user must explicitly
    enable VAD mode. Includes a dedicated global pause/resume hotkey for instant
    mute (privacy panic) without leaving VAD mode.
- **Configurable output method** - How transcribed text reaches the target app:
  - *Direct input* - Simulate keystrokes to type text at the current cursor position.
    No clipboard involvement. (Default)
  - *Clipboard + paste* - Copy text to clipboard and paste. More compatible with some
    apps. Saves and restores prior clipboard contents after paste so the user's
    clipboard is not clobbered.
  - *Clipboard only* - Copy text to clipboard but do not paste. User decides when and
    where to paste. Good for review-before-commit workflows.
- **Auto-submit after insertion** - Optionally send a keypress after text is inserted.
  Configurable to Enter, Ctrl+Enter, or Cmd+Enter (platform-aware). Useful for chat
  apps, search bars, or terminal input where you want to dictate and send in one
  motion.
- **Edit-before-insert buffer** - Optional small floating window that shows the
  transcription result before committing it to the target app. User can review, make
  quick edits, then confirm or discard. Bridges the gap between instant paste and
  clipboard-only workflows.
- **Selection-aware replacement** - If the user has text selected in the target app
  when dictation begins, the transcription replaces the selection. Enables
  "re-dictate this phrase" workflows without manual delete-and-retype.
- **Focus lock** - When dictation starts, EchoType captures a reference to the
  currently focused window and cursor position. When the transcription finishes
  (even seconds later on a slow local model), text is inserted into that original
  location regardless of where the user's focus has moved since. Prevents text from
  landing in the wrong app if the user switches windows during processing.
- **Private mode** - A one-click toggle (or dedicated hotkey) for "don't store this
  dictation." When active, the current transcription and its audio are not saved to
  history, regardless of retention settings. For sensitive moments.
- **Ultra-low-latency pipeline** - The path from voice to text on screen must feel
  instant. Every millisecond in the transcription loop is a target.
- **Cross-platform desktop** - Windows, macOS, and Linux via Tauri. One codebase,
  native feel on each platform.

## 2. Speech Engine Options

- **Built-in local models** - Users can browse, download, and install transcription
  models from within the app. No manual file wrangling.
- **Fully offline transcription** - The default path. No network required once a model
  is downloaded.
- **Optional cloud transcription** - For users who want it. Strictly opt-in, never
  default.
- **Bring-your-own API keys** - Support for third-party cloud providers (e.g., Groq,
  OpenAI, Deepgram, or others). Users supply their own keys and own their usage.
- **Automatic model management** - Download, update, and delete local models from a
  clean management UI. No hunting for files on disk. Model catalog and download URLs
  sourced from a static manifest in the GitHub repo (no server required). Integrity
  verified via checksums.
- **Multi-language support** - Language selection for transcription. Whisper-based
  models support dozens of languages out of the box. Users pick their language in
  settings, or per-dictation-mode. Bilingual users should be able to switch languages
  without restarting.
- **GPU acceleration** - Support for hardware-accelerated inference where available
  (CUDA on Nvidia, Metal on macOS, Vulkan as a cross-platform fallback). CPU-only as
  the default that always works. Selectable in settings with clear labeling of what
  the user's hardware supports.

## 3. Privacy & Architecture

- **Offline-first** - The app assumes no network. Cloud features are additive, never
  required.
- **No forced accounts** - No sign-up, no login, no telemetry phone-home. Ever.
- **All data stored locally** - Settings, history, metrics, models - everything lives
  on the user's machine by default.
- **Cloud is opt-in** - If a user enables cloud transcription, it uses *their* API
  keys, *their* provider, *their* choice. EchoType never proxies through its own
  servers.

## 4. Local Database & Sync

- **SQLite backing store** for:
  - Dictation history (transcribed text, timestamps, duration, engine used)
  - Saved audio recordings (when retention is enabled)
  - User settings and preferences
  - Model preferences and configurations
  - Metrics and usage statistics
- **Dictation history retention** - Configurable limit on how many past transcriptions
  to keep. Default: last 5 transcriptions with audio, retained indefinitely (no
  time-based expiry unless the user sets one). Users can raise or lower the count,
  set unlimited, disable entirely, or add an optional time-based ceiling (e.g., "keep
  last 100, but nothing older than 7 days"). Retained entries include:
  - Original transcribed text (for re-copying)
  - Source audio recording (for playback and review)
  - Metadata (timestamp, duration, engine, WPM)
  - Users can browse history, replay audio, re-copy text, or delete individual entries.
- **User-selectable database location** - Place the DB file wherever you want.
- **Settings export / import** - Export all settings and preferences to a standalone
  file (JSON or TOML) for backup or sharing. Import on another machine without needing
  the full database. Useful for helping others replicate your setup.

## 5. Metrics & Fun Telemetry

All metrics are local-only. This is telemetry *for the user*, not from them.

- **Real-time WPM display** - Show words-per-minute live during dictation.
- **Average WPM tracking** - Rolling and lifetime averages.
- **Total words dictated** - Lifetime count plus rolling period breakdowns.
- **Daily and weekly usage stats** - How much you dictated, when, and with which
  engine.
- **Dictation streak tracking** - Consecutive days of dictation usage.
- **Milestones and achievements** - Fun milestone markers in the metrics dashboard
  (e.g., "10,000 words dictated", "30-day streak"). Not notifications or pop-ups --
  just something to discover when you open the dashboard. Bragging rights you have
  to go look at.
- **Speaking vs. typing comparison** - If the user sets a typing baseline (WPM), show
  how much faster (or slower) dictation is by comparison.

## 6. UI & UX

- **Minimal runtime presence** - System tray / menu bar icon as the persistent anchor.
  No dock icon, no taskbar clutter. The tray icon is the only visible footprint unless
  the user opens a window. Platform note: Linux system tray support is fragmented
  (XDG StatusNotifierItem vs. legacy tray vs. GNOME's removal of the tray) -- needs
  platform-appropriate fallbacks.
- **First-run setup wizard** - Guided onboarding on first launch:
  1. Grant microphone permission (with platform-specific guidance if denied).
  2. Grant Accessibility permission on macOS (required for direct input mode).
  3. Download a recommended default model (with size/speed/quality labels so the user
     can make an informed choice, or just hit "recommended" and go).
  4. Set preferred hotkey and test it.
  5. Test microphone input and run a sample transcription.
  6. Done. The whole flow should take under two minutes.
- **OS permission handling** - Guided flows for platform-specific permissions.
  Microphone access on all platforms, Accessibility on macOS (for simulated
  keystrokes), portal permissions on Linux/Wayland for global hotkeys. Clear
  instructions and deep-links to system settings when permissions are denied.
  Graceful fallback behavior (e.g., auto-switch to clipboard mode if Accessibility
  permission is missing rather than silently failing).
- **Settings window** - Clean, Tauri-based settings panel for all configuration.
- **Model management UI** - Browse available models, see installed models, download
  and delete with one click. Each model shows size, relative speed, accuracy tier, and
  supported languages so non-technical users know what to pick.
- **Metrics dashboard** - Visual display of usage stats, streaks, and badges.
- **API key management** - Add, edit, and remove provider API keys. Masked display,
  secure storage.
- **Theme support** - Dark mode by default. Light mode available. High-contrast mode
  for low-vision users. Respect OS-level text scaling preferences.
- **Accessibility** - Full keyboard navigation across all UI surfaces. Screen reader
  support (ARIA roles, focus management) for VoiceOver, NVDA, and Orca. Metrics and
  stats available as text, not just charts. The settings UI must be usable without a
  mouse -- this is a tool for people who may not be able to type, so the UI itself
  needs to be navigable by alternative input methods.
- **Runtime microphone health** - Ongoing input level indicator (in tray icon or
  overlay), clipping detection, and device-disconnect alerts. The first-run mic test
  covers setup; this covers daily use. If the selected mic disappears mid-session,
  notify the user and optionally fall back to the system default.
- **Visual dictation feedback** - Optional visual indicator (overlay, tray animation,
  or similar) that shows EchoType is actively listening.
- **Audio dictation feedback** - Optional audio cues (start/stop chimes, etc.) to
  confirm dictation state changes. Configurable:
  - *Output device selection* - Route feedback audio to a specific output device
    (e.g., headphones only, not speakers).
  - *Volume control* - Independent volume slider for feedback sounds, separate from
    system volume.

## 7. Advanced Controls

- **Configurable hotkeys** - Change the dictation hotkey, set per-mode hotkeys, or
  disable the global hotkey entirely. Any key or key combo the OS can see is valid,
  including non-standard inputs (external buttons, foot pedals, macro pads).
- **Multiple dictation modes** - At minimum:
  - *Raw mode* - Verbatim transcription, minimal processing.
  - *Formatted mode* - Auto-capitalization, punctuation, paragraph breaks.
  - Additional user-defined modes possible.
- **Streaming with replacement** - Optionally stream partial transcription text in
  real time, then replace with the final polished version on release. Partial text
  should be visually distinct (e.g., lighter color or faded) so the user knows it's
  provisional. If the final transcription fails to arrive, the partial text remains
  as-is -- no silent deletion of what's already on screen.
- **Punctuation auto-insertion** - Most modern STT models handle this natively. Expose
  a toggle for users who want raw output.
- **Microphone selection** - Choose which input device to use. Support hot-switching.
- **Mute system audio during dictation** - Optional toggle to silence system output
  while recording to avoid feedback loops.
- **Adjustable silence cutoff** - Configure how long a pause before dictation
  auto-stops (or disable auto-stop entirely for manual release only).
- **Custom vocabulary** - User-maintained word list for terms that are frequently
  misheard or misspelled by the speech engine. EchoType auto-corrects similar-sounding
  transcription results to match the user's defined words. Useful for names, jargon,
  brand names, or niche terminology the model hasn't seen.
- **Per-application profiles** - Bind different dictation modes, output methods, and
  custom vocabularies to specific applications. For example: raw mode in terminal,
  formatted mode in Google Docs, a custom "code dictation" profile in VS Code.
  Profiles switch automatically based on the focused application. Matching uses the
  most reliable identifier available per platform (bundle ID on macOS, executable
  path on Windows/Linux) with optional user-defined display names. Users can also
  create named profiles and manually assign them.
- **Noise suppression** - Optional preprocessing pass (e.g., RNNoise or similar) on
  the audio input before it reaches the speech engine. Reduces transcription errors
  from background noise, keyboard clatter, or fans. Togglable with configurable
  aggressiveness.
- **CLI mode** - Headless operation for power users and automation:
  - *Daemon mode* - Run EchoType as a background process controllable via IPC, without
    the GUI. Integrates with scripts, Alfred/Raycast workflows, Hammerspoon,
    AutoHotkey, etc.
  - *Pipe mode* - `echotype --stdout` streams transcription to stdout for piping into
    other programs.

## 8. Future-Ready Hooks

These are not launch requirements but the architecture should make them possible
without rewrites.

- **Plugin architecture** - A defined interface for third-party or user-built
  extensions.
- **Custom post-processing pipeline** - Chain transformations on transcribed text:
  rewrite, summarize, expand, translate, etc.
- **Text transformation rules** - User-defined find-and-replace, regex, or template
  expansions applied to output.
- **Confidence indicators** - Surface word-level confidence scores from the engine.
  Low-confidence words could be highlighted or underlined, with alternative suggestions
  on click. Reduces silent errors for technical, medical, or legal dictation.
- **Custom model loading** - Import user-provided fine-tuned models (GGML, ONNX, etc.)
  beyond the built-in catalog. For users who have trained models on domain-specific
  data.
- **Editor integration protocol** - A local API or WebSocket interface that editor
  extensions can subscribe to. Enables richer integration with VS Code, JetBrains,
  Neovim, Obsidian, etc. -- cursor-aware insertion, language-aware formatting, and
  transcription events beyond what simulated keystrokes can offer.
- **Multi-machine database sync** - Support placing the SQLite DB on a NAS, Dropbox,
  Syncthing folder, or other shared location so settings, history, and metrics merge
  across devices. Stretch goal due to SQLite locking complexity over network
  filesystems.

## 9. Distribution & Project

- **Open source** - Fully open source, hosted on GitHub. MIT or similar permissive
  license (firm decision needed before first public commit).
- **Completely free** - No paid tiers, no premium features, no backers, no sponsors.
  The whole thing, forever.
- **Zero infrastructure** - No servers to run, no services to maintain. The project is
  the repo and nothing else.
- **GitHub Releases for builds** - Pre-built binaries for all platforms published via
  GitHub Actions and Releases. Users download directly from GitHub.
- **Platform-specific installers**:
  - *Windows* - `.msi` installer. WinGet manifest for `winget install echotype`.
  - *macOS* - `.dmg` disk image. Homebrew cask for `brew install --cask echotype`.
  - *Linux* - AppImage for universal compatibility. Flatpak / Flathub listing. `.deb`
    and `.rpm` packages where feasible.
- **Code signing and notarization** - Signed binaries for macOS (notarized for
  Gatekeeper) and Windows (Authenticode for SmartScreen). Unsigned apps trigger scary
  OS warnings that will kill adoption with non-technical users. Investigate free
  signing options for open source projects (e.g., SignPath Foundation).
- **Signed releases** - SHA256 checksums and GPG signatures published alongside every
  binary on GitHub Releases. Table stakes for security-conscious users downloading
  pre-built binaries from the internet.
- **GitHub Pages website** - Project website and documentation hosted on GitHub Pages.
  Custom domain pointing at it.
- **Self-updating** - App checks GitHub Releases for new versions via Tauri's built-in
  updater (static JSON manifest hosted on GitHub Pages or in the Release itself).
  User-selectable behavior: auto-update, download-and-prompt, or notify-only.
  Defaults to download-and-prompt for trust reasons -- auto-updating binaries is a
  supply chain concern for a privacy-focused tool.
- **Crash and diagnostic logging** - Local log file with configurable verbosity. A
  "Copy diagnostic info" button in settings that bundles OS version, EchoType version,
  active model, audio device info, and recent log entries into a pasteable block for
  GitHub issues. No logs leave the machine unless the user copies them. Includes a
  "test microphone" and "test transcription" self-check in settings for debugging.
- **Internationalization** - UI strings externalized for community translation.
  Framework TBD, but the architecture should support localized settings and
  documentation from the start.

---

## Design Principles

1. **Speed above all** - If it's not fast, nothing else matters.
2. **Privacy by default** - The user should never wonder if their voice data left
   their machine.
3. **Stay invisible** - The best UX is no UX. Hold a key, speak, release, done.
4. **Respect the user** - No accounts, no nags, no upsells, no dark patterns.
5. **Build for power users, accessible to everyone** - Simple out of the box,
   infinitely configurable underneath.

---

*This is a living document. It defines what EchoType should be, not how to build it.
Implementation details belong in the technical spec.*
