# EchoType

**Your voice, your machine, your rules.**

EchoType is a desktop dictation app that transcribes speech to text and inserts it where your cursor is.
It is designed to be local-first, fast, and predictable, with optional cloud engines when you want them.

Built with Rust and Tauri for a lightweight runtime footprint.

## Why EchoType

EchoType is for people who want:

- Local speech-to-text by default.
- Optional cloud transcription with bring-your-own API keys.
- Strong privacy controls and explicit cloud opt-in.
- Reliable low-latency dictation for daily use.
- A simple runtime experience with deep configuration when needed.

## Project Status

EchoType is in early development.

- Product spec: [docs/product-spec.md](docs/product-spec.md)
- Core architecture and implementation are in progress.
- Installable builds are not published yet.

## Planned Capabilities

### Dictation Workflow

- Global hotkey activation with hold-to-dictate, toggle, and VAD modes.
- Direct input, clipboard + paste, or clipboard-only output.
- Selection-aware replacement and focus lock for reliable insertion.
- Optional edit-before-insert buffer and auto-submit actions.

### Engines and Models

- Downloadable local models with checksum verification.
- Fully offline transcription after model download.
- Optional cloud providers (for example: OpenAI, Groq, Deepgram) using your own API keys.
- Multi-language support and optional GPU acceleration (CUDA, Metal, Vulkan).

### Privacy and Data Ownership

- No forced accounts.
- Local storage for settings, history, and metrics.
- Private mode for sessions that should not be retained.
- Cloud usage is explicit opt-in and controlled by you.

### Power User Controls

- Per-application profiles.
- Custom vocabulary and tuning controls.
- Streaming transcription with final replacement.
- CLI daemon/pipe modes for automation workflows.

## Distribution Plan

Initial releases are planned through GitHub Releases:

- **Windows**: `.msi` installer and WinGet package.
- **macOS**: `.dmg` disk image and Homebrew cask.
- **Linux**: AppImage, Flatpak, `.deb`, and `.rpm`.

## Contributing

Contributions are welcome.

Until a full contribution guide is published, open an issue first for substantial changes so we can align on scope and direction.
