# EchoType

**Your voice, your machine, your rules.**

EchoType is a desktop dictation app that transcribes speech to text and inserts it where your cursor is. Hold a hotkey, speak, release, and your words appear at the cursor in any application.

Local-first, fast, and private by default, with optional cloud engines when you want them. Built with Rust and Tauri 2 for a stable lightweight runtime footprint.

## Features

### Dictation

- **Hold-to-dictate** — global hotkey works in any application, on any platform
- **Multiple dictation modes** — hold-to-dictate, toggle, and voice activity detection (VAD)
- **Text insertion** — direct input, clipboard + paste, or clipboard-only output
- **Selection-aware replacement** — dictate over selected text to replace it
- **Edit-before-insert** — review and edit transcription before committing
- **Focus lock** — captures your target window so text lands in the right place
- **Low latency** — optimized audio pipeline with latency profiling per engine

### Engines and Models

- **Local transcription** — Whisper models via `whisper-rs`, fully offline after download
- **Model management** — browse, download, verify, and switch models from the UI
- **Cloud providers** — Groq, OpenAI, and Deepgram with bring-your-own API keys
- **Engine comparison** — latency profiling tracks processing, transcription, and insertion time per engine so you can compare local models against cloud providers
- **Multi-language support** with per-model language selection

### Privacy and Data

- **No accounts required** — everything is stored locally in SQLite
- **Private mode** — skip history storage for sensitive sessions
- **Cloud is opt-in** — local transcription is the default; cloud requires explicit key setup
- **API keys in OS keychain** — stored via the platform's native credential manager

### Customization

- **Per-application profiles** — auto-switch settings based on the focused application
- **Custom vocabulary** — word lists for domain-specific correction
- **Themes** — dark (default), light, and high-contrast; respects OS text scaling
- **Configurable hotkeys** — set your preferred activation key and mode per profile
- **Audio muting** — optionally mute system audio during dictation

### Metrics and History

- **Dictation history** — browse, search, copy, and replay past transcriptions
- **Usage dashboard** — daily/weekly stats, hourly activity patterns, words per minute tracking
- **Latency profiling** — per-engine breakdown of processing, network, transcription, and insertion time with p50/p95 percentiles
- **Streaks and milestones** — fun usage badges

### System Integration

- **System tray** — runs from the tray with no taskbar clutter
- **First-run setup wizard** — guided mic permission, model download, and hotkey configuration
- **Auto-updater** — checks for updates via signed GitHub Releases
- **Diagnostic tools** — copy diagnostic info, test microphone, test transcription
- **CLI modes** — daemon and pipe modes for automation workflows

## AI-First Development

EchoType is built as an **AI-operable project**. A coding agent can clone, bootstrap, run, inspect, fix, test, and commit — all with non-interactive commands. This is a core project requirement, not a side workflow.

- AI operator guide: [docs/ai-operator.md](docs/ai-operator.md)
- Agent scripts: `./scripts/agent/bootstrap`, `dev`, `check`, `logs`, `fix`

### Hand it to your agent

```text
Clone https://github.com/3MUl0R/EchoType.git.
Read README.md and docs/ai-operator.md.
Set up the project, run it from source, inspect logs, fix issues, run checks, and commit changes with a clear summary.
```

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Runtime | [Tauri 2](https://v2.tauri.app/) (Rust backend, webview frontend) |
| Frontend | Svelte 5, Tailwind CSS v4, Vite |
| Local STT | [whisper-rs](https://github.com/tazz4843/whisper-rs) (whisper.cpp bindings) |
| Cloud STT | Groq, OpenAI, Deepgram APIs |
| Database | SQLite via `rusqlite` (bundled) |
| Audio | `cpal` (capture), `nnnoiseless` (denoise), `rubato` (resample), `rodio` (playback) |
| Package manager | Bun |

## Getting Started

### Prerequisites

- [Rust](https://rustup.rs/) (stable)
- [Bun](https://bun.sh/)
- Platform prerequisites for Tauri 2 ([see Tauri docs](https://v2.tauri.app/start/prerequisites/))

### Run from source

```bash
git clone https://github.com/3MUl0R/EchoType.git
cd EchoType
bun install
bun run tauri dev
```

### Build

```bash
bun run tauri build
```

## Project Status

EchoType has completed its core implementation through 13 milestones covering the full feature set from audio capture through distribution tooling. The app is functional and feature-complete for daily use.

- Product spec: [docs/product-spec.md](docs/product-spec.md)
- Tech stack: [docs/tech-stack.md](docs/tech-stack.md)
- Milestones: [docs/milestones.md](docs/milestones.md)

### Current focus

- Packaging signed installers for all platforms
- Publishing to package managers (WinGet, Homebrew, Flatpak)
- Community testing and bug fixes
- Translation contributions

## Distribution

Releases are distributed through GitHub Releases with platform-specific packages:

- **Windows**: `.msi` installer, WinGet package
- **macOS**: `.dmg` disk image, Homebrew cask
- **Linux**: AppImage, Flatpak, `.deb`, `.rpm`

## Contributing

Contributions are welcome. Open an issue first for substantial changes so we can align on scope and direction.

AI-assisted and agent-authored contributions are encouraged when changes are reviewable and tested.
