# EchoType

**Your voice, your machine, your rules.**

EchoType is a desktop dictation tool that transcribes speech to text and pastes it wherever your cursor is. It runs locally by default, stays out of your way, and never phones home unless you tell it to.

Built with Rust and Tauri. Free and open source. No accounts, no servers, no nonsense.

---

## What It Does

Hold a key, speak, release. Your words appear where your cursor is. That's it.

- **Offline by default** -- download a model and go. No internet required.
- **Cross-platform** -- Windows, macOS, and Linux.
- **Multiple activation modes** -- hold-to-dictate, toggle on/off, or voice activity detection.
- **Direct input or clipboard** -- simulates keystrokes or uses the clipboard. Your choice.
- **Bring your own cloud** -- optionally use Groq, OpenAI, Deepgram, or others with your own API keys.
- **GPU accelerated** -- CUDA, Metal, and Vulkan support. CPU-only works too.
- **Multi-language** -- dozens of languages supported out of the box.

## Why

Every existing dictation tool is either cloud-only, closed source, expensive, abandoned, or all four. EchoType is none of those things.

Your voice data stays on your machine. There are no accounts to create, no subscriptions to manage, and no telemetry to opt out of. The app is the binary on your disk and nothing else.

## Features

**Core dictation** -- hold-to-dictate, toggle mode, or always-listening VAD. Text goes where your cursor is via simulated keystrokes or clipboard. Focus lock ensures text lands in the right app even if you switch windows during processing.

**Speech engines** -- built-in downloadable local models with one-click install. Optional cloud transcription with your own API keys. Models sourced from a public catalog with checksum verification.

**Per-app profiles** -- different modes, output methods, and vocabularies for different apps. Automatically switches based on the focused application.

**Privacy** -- offline-first, no accounts, no telemetry. Private mode for sensitive dictation that shouldn't be stored. All data lives on your machine.

**Metrics** -- words per minute, total words dictated, daily stats, streaks, and milestones. All local. Telemetry *for* you, not from you.

**Advanced** -- custom vocabulary, noise suppression, silence cutoff tuning, streaming transcription with live replacement, auto-submit for chat apps, and a CLI mode with daemon and pipe support.

## Status

EchoType is in early development. The [product spec](docs/product-spec.md) is complete and the technical implementation plan is next.

## Installation

Coming soon. EchoType will be available as:

- **Windows** -- `.msi` installer and `winget install echotype`
- **macOS** -- `.dmg` disk image and `brew install --cask echotype`
- **Linux** -- AppImage, Flatpak, `.deb`, and `.rpm`

## Building from Source

Coming soon.

## Contributing

Coming soon. We want contributors and will have a proper guide before the first public release.

## License

[MIT](LICENSE)
