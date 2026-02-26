# EchoType - Tech Stack

This document specifies the technologies, crates, and tools used to build EchoType.
It is a reference for contributors and a foundation for the implementation plan.

---

## Core Platform

| Layer | Technology | Notes |
|-------|-----------|-------|
| Language | **Rust** | Backend, audio pipeline, STT inference, all core logic |
| Desktop framework | **Tauri 2** | Webview-based desktop shell with Rust backend |
| Frontend framework | **Svelte 5** | Plain Svelte with Vite (not SvelteKit) |
| Styling | **Tailwind CSS v4** | Via `@tailwindcss/vite` plugin -- no PostCSS config |
| Database | **SQLite** | Via `rusqlite` with bundled SQLite |

### Why plain Svelte (not SvelteKit)

SvelteKit is designed for web apps with SSR, server endpoints, and file-based routing.
None of that applies to a Tauri desktop app. Plain Svelte with Vite gives us components,
Svelte 5 runes reactivity, and a fast dev server with zero framework friction.

---

## Rust Dependencies

### Speech-to-Text

| Crate | Version | Purpose |
|-------|---------|---------|
| `whisper-rs` | 0.15.x | Rust bindings for whisper.cpp |

Whisper is the only STT engine we support. GPU acceleration is enabled via feature
flags on `whisper-rs`:

| Feature | Backend | Platforms |
|---------|---------|-----------|
| `metal` | Apple Metal | macOS |
| `cuda` | NVIDIA CUDA | Windows, Linux |
| `vulkan` | Vulkan | Windows, Linux |

CPU-only is the default. GPU backends require the respective SDKs on the build machine.

The `whisper-rs` crate compiles whisper.cpp from source via `whisper-rs-sys`. Use the
`tracing_backend` feature to route whisper.cpp logs into our tracing pipeline.

### Audio

| Crate | Version | Purpose |
|-------|---------|---------|
| `cpal` | 0.17.x | Cross-platform audio capture (mic input) |
| `rodio` | 0.22.x | Audio playback for feedback sounds (built on cpal) |
| `voice_activity_detector` | 0.2.x | Silero VAD v5 via ONNX Runtime |
| `nnnoiseless` | 0.5.x | Noise suppression (pure Rust port of RNNoise) |

**Platform backends for `cpal`:**
- macOS: CoreAudio
- Windows: WASAPI
- Linux: ALSA (default), PulseAudio and JACK optional

**Audio pipeline note:** `nnnoiseless` operates on 48kHz audio in 480-sample (10ms)
frames. Whisper expects 16kHz. The pipeline should capture at 48kHz, denoise, then
downsample to 16kHz -- avoids double resampling.

### Tauri Plugins

| Crate | Version | Purpose |
|-------|---------|---------|
| `tauri-plugin-global-shortcut` | 2.3.x | Global hotkey registration (press + release events) |
| `tauri-plugin-updater` | 2.9.x | Auto-update via signed GitHub Releases |

System tray is built into Tauri 2 core via the `tray-icon` feature flag on the `tauri`
crate. No plugin needed.

### Input Simulation

| Crate | Version | Purpose |
|-------|---------|---------|
| `enigo` | 0.6.x | Cross-platform keystroke and text simulation |

**Platform behavior:**
- macOS: CGEvent (requires Accessibility permission)
- Windows: SendInput
- Linux: X11 (default), Wayland support via feature flags (experimental)

The product spec includes clipboard+paste as a fallback for platforms where direct input
is unreliable. On macOS, the first-run wizard handles Accessibility permission. If
permission is missing, the app auto-switches to clipboard mode.

### Database

| Crate | Version | Purpose |
|-------|---------|---------|
| `rusqlite` | 0.32.x | SQLite bindings (use `bundled` feature) |
| `rusqlite_migration` | 2.4.x | Schema migrations via `user_version` pragma |

**Why rusqlite (not sqlx, sea-orm, or tauri-plugin-sql):**
- Synchronous is fine for a single-user desktop app. Async adds complexity for zero
  benefit here.
- `bundled` feature compiles SQLite from source -- consistent version across platforms,
  no system dependency issues.
- Database logic belongs in the Rust backend, not the frontend JS layer.

### Networking

| Crate | Version | Purpose |
|-------|---------|---------|
| `reqwest` | 0.12.x | HTTP client for model downloads and cloud API calls |

Use `stream` feature for streaming downloads with progress reporting. Use `rustls-tls`
for consistent cross-platform TLS without OpenSSL dependency.

Resumable downloads use the `Range` HTTP header. Check for partial files on disk, send
`Range: bytes={file_size}-`, and append.

### Security

| Crate | Version | Purpose |
|-------|---------|---------|
| `keyring` | 3.6.x | Platform keychain for API key storage |

**Platform backends:**
- macOS: Keychain Services (`apple-native` feature)
- Windows: Credential Manager (`windows-native` feature)
- Linux: Secret Service / GNOME Keyring (`linux-native` feature)

All three platform features must be enabled explicitly -- `keyring` has no default
features.

### Clipboard

| Crate | Version | Purpose |
|-------|---------|---------|
| `arboard` | 3.4.x | Cross-platform clipboard read/write |

Used directly in Rust rather than through a Tauri plugin, since the clipboard workflow
(save, write transcription, simulate paste, restore) is backend logic. Enable
`wayland-data-control` feature for Wayland support on Linux.

### Logging

| Crate | Version | Purpose |
|-------|---------|---------|
| `tracing` | 0.1.x | Instrumentation macros |
| `tracing-subscriber` | 0.3.x | Log formatting and routing |
| `tracing-appender` | 0.2.x | File appender with rolling rotation |

Use `tracing-subscriber` with `env-filter` feature for configurable verbosity.

**Log rotation:** `tracing-appender::rolling::daily` creates one log file per day. We
add our own cleanup to delete log files older than 3 days -- `tracing-appender` does
not handle deletion of old files automatically.

**Non-blocking I/O:** Use `tracing_appender::non_blocking` to avoid log writes blocking
the dictation pipeline. The returned `WorkerGuard` must be held for the app's lifetime.

### Serialization

| Crate | Version | Purpose |
|-------|---------|---------|
| `serde` | 1.x | Serialization/deserialization framework |
| `serde_json` | 1.x | JSON support (model manifest, settings export, IPC) |
| `toml` | 0.8.x | TOML support (settings export/import, if we support TOML format) |

---

## Frontend Dependencies

| Package | Purpose |
|---------|---------|
| `@tauri-apps/api` | Tauri IPC, events, and core APIs |
| `@tauri-apps/plugin-global-shortcut` | JS bindings for global shortcut plugin |
| `@tauri-apps/plugin-updater` | JS bindings for auto-updater |
| `tailwindcss` | Utility-first CSS |
| `@tailwindcss/vite` | Tailwind v4 Vite plugin (replaces PostCSS setup) |

### Tailwind v4 Setup

No `tailwind.config.js` or `postcss.config.js` needed. Tailwind v4 uses a Vite plugin
and CSS-based configuration:

```css
/* src/app.css */
@import "tailwindcss";

@theme {
  /* custom theme values go here */
}
```

---

## Testing

### Rust

| Tool | Purpose |
|------|---------|
| `cargo test` | Unit and integration tests |
| `tauri::test` | Mock runtime for testing Tauri commands without a webview |

Enable the `test` feature on the `tauri` crate for access to `mock_builder()`,
`mock_context()`, and `MockRuntime`.

### Frontend

| Tool | Purpose |
|------|---------|
| Vitest | Test runner (integrates with Vite) |
| `@testing-library/svelte` | Component rendering and interaction |
| `@tauri-apps/api/mocks` | Mock Tauri IPC calls in frontend tests |

### End-to-End

| Tool | Purpose |
|------|---------|
| Playwright | E2E tests against the Vite dev server with mocked IPC |
| `tauri-driver` | WebDriver-based E2E against the real app (Linux and Windows only) |

Playwright runs against the Vite dev server with mocked Tauri IPC. This tests the full
UI flow without requiring a built Tauri app. For native integration testing,
`tauri-driver` provides WebDriver support on Linux (WebKitWebDriver) and Windows
(Edge Driver). macOS does not have WebDriver support for WKWebView.

---

## Build and CI

### GitHub Actions

| Tool | Purpose |
|------|---------|
| `tauri-apps/tauri-action@v0` | Official Tauri action for cross-platform builds and GitHub Releases |
| `dtolnay/rust-toolchain@stable` | Rust toolchain setup |
| `swatinem/rust-cache@v2` | Cargo build caching |

The Tauri action builds the app, bundles platform-specific installers, and publishes
them to GitHub Releases. It supports a build matrix for all three platforms and both
macOS architectures (aarch64 and x86_64).

### Auto-Update Signing

The Tauri updater requires a signing keypair. Generate once:

```
npx @tauri-apps/cli signer generate -w ~/.tauri/echotype.key
```

Public key goes in `tauri.conf.json`. Private key is a CI secret
(`TAURI_SIGNING_PRIVATE_KEY`). Signature verification is mandatory and cannot be
disabled.

### Code Signing

| Platform | Mechanism | Notes |
|----------|-----------|-------|
| macOS | Apple Developer certificate + notarization | Required to avoid Gatekeeper warnings |
| Windows | Authenticode certificate | Required to avoid SmartScreen warnings |
| Linux | N/A | No OS-level code signing requirement |

Code signing certificates are passed as CI secrets. The Tauri action handles the signing
process when the environment variables are set. Exact certificate acquisition (Apple
Developer Program, SignPath Foundation for OSS, etc.) is a project setup task.

---

## Project Structure

```
echotype/
├── package.json
├── vite.config.ts
├── tsconfig.json
├── index.html
├── src/                          # Frontend
│   ├── main.ts
│   ├── app.css                   # Tailwind import + theme
│   ├── App.svelte
│   └── lib/
│       └── components/
├── src-tauri/                    # Rust backend
│   ├── Cargo.toml
│   ├── build.rs
│   ├── tauri.conf.json
│   ├── capabilities/
│   │   └── default.json
│   ├── icons/
│   └── src/
│       ├── main.rs               # Desktop entry point
│       └── lib.rs                # App logic and commands
├── docs/
│   ├── product-spec.md
│   └── tech-stack.md             # This document
├── tests/                        # E2E tests
├── LICENSE
└── README.md
```

---

*This document specifies what we build with. The product spec defines what we build.
The implementation plan (next) defines the order we build it in.*
