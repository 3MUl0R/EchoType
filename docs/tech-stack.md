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

**Note:** The `whisper-rs` GitHub repo (tazz4843/whisper-rs) is archived. Active
development continues on Codeberg. Crate releases on crates.io are still current.

### Audio

| Crate | Version | Purpose |
|-------|---------|---------|
| `cpal` | 0.17.x | Cross-platform audio capture (mic input) |
| `rodio` | 0.22.x | Audio playback for feedback sounds (built on cpal) |
| `voice_activity_detector` | 0.2.x | Silero VAD v5 via ONNX Runtime (see build note below) |
| `nnnoiseless` | 0.5.x | Noise suppression (pure Rust port of RNNoise) |
| `rubato` | 0.16.x | Asynchronous audio resampling (48kHz to 16kHz) |

**Platform backends for `cpal`:**
- macOS: CoreAudio
- Windows: WASAPI
- Linux: ALSA (default), PulseAudio and JACK optional

**ONNX Runtime provisioning:** `voice_activity_detector` depends on the `ort` crate
which by default downloads prebuilt ONNX Runtime binaries from Microsoft at build time.
For reproducible CI builds, pin the ORT version and cache the downloaded binary. The
`ort` crate supports a `ORT_LIB_LOCATION` environment variable to point at a
pre-downloaded copy, avoiding network access during builds.

**Audio pipeline note:** `nnnoiseless` operates on 48kHz audio in 480-sample (10ms)
frames. Whisper expects 16kHz. The pipeline should capture at 48kHz, denoise, then
downsample to 16kHz via `rubato` -- avoids double resampling. `rubato` is a pure Rust
high-quality resampler with no C dependencies.

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
| `rusqlite` | 0.38.x | SQLite bindings (use `bundled` feature) |
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
| `reqwest` | 0.13.x | HTTP client for model downloads and cloud API calls |

Use `stream` feature for streaming downloads with progress reporting. Use `rustls-tls`
for consistent cross-platform TLS without OpenSSL dependency.

Resumable downloads use the `Range` HTTP header. Check for partial files on disk, send
`Range: bytes={file_size}-`, and append.

### Security

| Crate | Version | Purpose |
|-------|---------|---------|
| `keyring` | 3.6.x | Platform keychain for API key storage |
| `sha2` | 0.10.x | SHA-256 checksums for model download integrity verification |
| `hex` | 0.4.x | Hex encoding for checksum comparison |

**Platform backends:**
- macOS: Keychain Services (`apple-native` feature)
- Windows: Credential Manager (`windows-native` feature)
- Linux: Secret Service / GNOME Keyring (`linux-native` feature)

All three platform features must be enabled explicitly -- `keyring` has no default
features.

### Focused Window Detection

Per-app profiles and focus lock require identifying the currently focused application.
There is no single cross-platform crate that covers this reliably, so we use platform
APIs directly via conditional compilation:

| Platform | API | Identifier |
|----------|-----|------------|
| macOS | `NSWorkspace` / Core Graphics (`CGWindowListCopyWindowInfo`) | Bundle ID (e.g., `com.apple.Terminal`) |
| Windows | `GetForegroundWindow` + `GetWindowThreadProcessId` via `windows` crate | Executable path |
| Linux (X11) | `xcb` or `x11rb` -- `_NET_ACTIVE_WINDOW` property | Window class / executable path |
| Linux (Wayland) | Limited -- no standard protocol for querying focused app from another process | Best-effort via compositor extensions |

This is a thin platform abstraction layer we write ourselves. No third-party crate
needed -- the platform calls are straightforward and we avoid an unnecessary dependency.

**Wayland limitation:** Wayland's security model intentionally prevents apps from
inspecting other windows. Per-app profile switching may be unavailable or require
compositor-specific extensions (e.g., `wlr-foreign-toplevel-management` on wlroots-based
compositors). The app should detect this at runtime and fall back to manual profile
selection.

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

### CLI and IPC

| Crate | Version | Purpose |
|-------|---------|---------|
| `clap` | 4.x | Command-line argument parsing (daemon mode, pipe mode, flags) |
| `interprocess` | 2.x | Cross-platform IPC (Unix domain sockets + Windows named pipes) |

`clap` handles CLI parsing for `echotype --stdout`, `echotype --daemon`, and any other
flags. Use the `derive` feature for declarative argument definitions.

`interprocess` provides the transport for daemon mode IPC. The GUI app and CLI clients
communicate over Unix domain sockets (macOS/Linux) or named pipes (Windows). Messages
are serialized with `serde_json` over the socket.

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

**Playwright is the primary E2E tool.** It runs against the Vite dev server with mocked
Tauri IPC, testing the full UI flow without requiring a built Tauri app. This covers the
vast majority of E2E scenarios and runs on all platforms.

`tauri-driver` is optional, used only for a small set of native smoke tests that verify
the real app launches and basic IPC works. It requires WebDriver support, which is
available on Linux (WebKitWebDriver) and Windows (Edge Driver) only -- macOS does not
support WebDriver for WKWebView.

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

### GPU Build Strategy

Ship **CPU-only as the default** artifact on all platforms. GPU-accelerated variants are
built as separate flavors:

| Flavor | Platforms | CI Requirement |
|--------|-----------|----------------|
| CPU (default) | All | None |
| Metal | macOS only | Xcode (already present on macOS runners) |
| CUDA | Windows, Linux | CUDA toolkit on runner |
| Vulkan | Windows, Linux | Vulkan SDK on runner |

Each GPU flavor is a separate CI matrix entry with its own `whisper-rs` feature flag.
The macOS Metal build can be the default macOS artifact since Metal is available on all
supported Macs and requires no additional SDK. CUDA and Vulkan produce separate
downloadable artifacts (e.g., `echotype-cuda-windows-x64.msi`).

The auto-updater should use separate update channels per flavor so a CPU user does not
accidentally receive a CUDA build.

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

## AI-First Developer Experience

EchoType treats AI coding agents as first-class operators. Development tooling must be
usable by both humans and local agents with the same command surface.

### Command Entry Points (Contract)

The repository should provide stable non-interactive wrappers:

| Command | Purpose |
|---------|---------|
| `./scripts/agent/bootstrap` | Install toolchains and project dependencies |
| `./scripts/agent/dev` | Start the app from source |
| `./scripts/agent/check` | Run lint, tests, and validation checks |
| `./scripts/agent/logs` | Print recent logs and diagnostics |

These wrappers can call `cargo`, `npm`, and Tauri commands internally, but the public
entry points should stay stable so user prompts and AI workflows do not break.

### Automation Requirements

- Commands must run non-interactively by default.
- Exit codes must be deterministic and meaningful.
- Validation output should be parseable (`--json` modes where practical).
- Setup should avoid hidden manual steps that an agent cannot infer.
- Logs must be available by CLI and stored in predictable local paths.

## Dependency Management

### Principles

EchoType's binary runs with full system access -- microphone, keystrokes, clipboard,
file system. Every external dependency that compiles into that binary is attack surface.
We treat dependency management as a security concern, not a convenience concern.

- **Update deliberately, not automatically.** Dependencies are updated by a human who
  reviews the diff, not by a bot that opens PRs on a schedule.
- **Vendor the high-risk side.** Rust crates compile into the native binary and run with
  full system privileges. npm packages compile into sandboxed JS in a webview. We vendor
  the Rust side.
- **Minimize transitive dependencies.** Prefer crates with small dependency trees. Prefer
  pure Rust crates over those with C bindings where the tradeoff is reasonable.

### Rust: Vendored Dependencies

All Rust crate sources are checked into the repository via `cargo vendor`. Builds never
fetch from crates.io or any other registry.

```
cargo vendor vendor/
```

This creates a `vendor/` directory containing the full source of every direct and
transitive dependency. A `.cargo/config.toml` at the repo root tells Cargo to use it:

```toml
[source.crates-io]
replace-with = "vendored-sources"

[source.vendored-sources]
directory = "vendor"
```

**Updating dependencies:**
1. Edit version in `Cargo.toml` as needed.
2. Run `cargo vendor vendor/` to refresh the vendor directory.
3. Review the diff -- `git diff vendor/` shows exactly what changed.
4. Commit the update with a message describing why the update was made.

**ONNX Runtime exception:** The `ort` crate (used by `voice_activity_detector`) downloads
a prebuilt ONNX Runtime binary from Microsoft at build time. This binary is cached in CI
and pointed to via the `ORT_LIB_LOCATION` environment variable, so builds do not make
network requests. The ORT binary version is pinned and its checksum verified.

### npm: Lockfile-Pinned, Not Vendored

npm packages are lower risk for this project -- they compile into JS that runs inside
Tauri's webview sandbox. The JS layer can only call Rust backend functions that we
explicitly expose as Tauri commands. It cannot access the file system, network, or OS
APIs directly.

npm dependencies are pinned via `package-lock.json` (exact versions, integrity hashes).
We do not vendor `node_modules/`. The lockfile is committed and CI uses `npm ci` (which
installs from the lockfile exactly, failing if it's out of sync with `package.json`).

### CI Auditing

| Tool | Runs on | Purpose |
|------|---------|---------|
| `cargo audit` | Every PR and push to main | Check vendored Rust crates against RustSec advisory database |
| `npm audit` | Every PR and push to main | Check npm packages against known vulnerabilities |

`cargo audit` runs against the vendored source, not the registry. It flags known CVEs
in any crate in the dependency tree. A flagged advisory does not necessarily block the
build -- some advisories are informational or not applicable -- but it ensures we are
aware and can make an informed decision.

### Supply Chain Summary

| Layer | Strategy | Fetches at build time? | Ships in binary? |
|-------|----------|----------------------|-----------------|
| Rust crates | Vendored in repo | No | Yes (compiled) |
| whisper.cpp | Compiled from vendored C source via `whisper-rs-sys` | No | Yes |
| SQLite | Compiled from vendored C source via `rusqlite` bundled feature | No | Yes |
| ONNX Runtime | Prebuilt binary from Microsoft, cached and pinned | No (cached) | Yes (linked) |
| npm packages | Lockfile-pinned, installed from registry | Yes | No (sandboxed JS in webview) |
| Silero VAD model | ONNX weights bundled by `voice_activity_detector` crate | No (vendored) | Yes (embedded) |

---

*This document specifies what we build with. The product spec defines what we build.
The implementation plan (next) defines the order we build it in.*
