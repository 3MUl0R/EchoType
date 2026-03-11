# EchoType

**Your voice, your machine, your rules.**

Hold a key, speak, release — your words appear at the cursor in any app. Local-first dictation powered by Whisper, with optional cloud engines when you want speed or scale.

No accounts. No telemetry. Verify it yourself — the codebase is the proof. Runs from source so you — or your AI — can fix, extend, and ship it.

## Why Run From Source

EchoType is designed to be cloned, built, and operated directly from source. This isn't a fallback — it's the primary workflow. Running from source means:

- Your AI coding agent can diagnose issues, apply fixes, run checks, and submit PRs autonomously
- You get the latest code, not a stale installer
- The full build pipeline is non-interactive and deterministic — clone, bootstrap, run

## Hand It to Your AI

```text
Clone https://github.com/3MUl0R/EchoType.git.
Read README.md and docs/ai-operator.md.
Bootstrap the project, run it from source, inspect logs, fix issues, and commit changes.
```

Agent scripts (`./scripts/agent/`):

| Script | Purpose |
|--------|---------|
| `bootstrap` | Install deps, verify toolchain |
| `dev` | Run from source |
| `check` | Lint, test, verify |
| `logs` | Print recent runtime logs |
| `fix` | Auto-fix formatting and lint issues |

All scripts are non-interactive, return deterministic exit codes, and are safe to run repeatedly.

## Prerequisites

Install these before bootstrapping:

| Dependency | Install | Verify |
|-----------|---------|--------|
| **Rust** (stable) | [rustup.rs](https://rustup.rs/) | `rustc --version` |
| **Bun** | [bun.sh](https://bun.sh/) | `bun --version` |
| **Tauri 2 system deps** | See below | — |

### Tauri 2 platform dependencies

**Windows:** Visual Studio Build Tools with "Desktop development with C++" workload, or full Visual Studio with MSVC.

**macOS:** `xcode-select --install`

**Linux (Debian/Ubuntu):**
```bash
sudo apt install libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev
```

Full list: [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/)

## Quick Start

```bash
git clone https://github.com/3MUl0R/EchoType.git
cd EchoType
bun install
bun run dev
```

Or use the agent bootstrap for a verified setup:

```bash
./scripts/agent/bootstrap
./scripts/agent/dev
```

> **Note for AI agents:** `./scripts/agent/dev` is a long-running blocking process (it starts the app with hot-reload). Run it in the background. The app is ready when Vite prints the local URL on stdout. To verify the app compiled, check for `Watching for file changes` in stderr or look for a recent log entry via `./scripts/agent/logs`.

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Runtime | Tauri 2 — Rust backend, webview frontend |
| Frontend | Svelte 5, Tailwind CSS v4, Vite |
| Local STT | whisper-rs (whisper.cpp bindings), fully offline |
| Cloud STT | Groq, OpenAI, Deepgram — bring your own API keys |
| Database | SQLite via rusqlite (bundled, zero-config) |
| Audio | cpal (capture), nnnoiseless (denoise), rubato (resample), rodio (playback) |
| Secrets | OS keychain via keyring crate — keys never touch disk |
| Package manager | Bun |

## What It Does

**Core workflow:**
- **Hold-to-dictate** — global hotkey works in any app, any platform
- **Local transcription** — Whisper models run offline after a one-time download
- **Cloud providers** — Groq, OpenAI, Deepgram with your API keys
- **Text insertion** — direct input, clipboard+paste, or clipboard-only

**Power features:**
- **Selection replacement** — dictate over selected text to replace it
- **Edit before insert** — review transcription before committing
- **Per-app profiles** — auto-switch settings based on focused application
- **Custom vocabulary** — word lists for domain-specific correction
- **Private mode** — skip history for sensitive sessions

**Diagnostics and operations:**
- **Latency profiling** — per-engine p50/p95 breakdowns so you can compare engines
- **System tray** — runs quietly, no taskbar clutter
- **Setup wizard** — guided first-run for mic, model download, and hotkey config
- **CLI modes** — headless transcription, model management, diagnostics

## Architecture — What's Not Obvious

Things an AI (or human) needs to know that aren't apparent from the code alone:

**Hotkey system:**
- Uses `tauri-plugin-global-shortcut` for registration with press + release events
- Windows uses a `WH_KEYBOARD_LL` hook to suppress letter key repeats during recording
- Never use `SendInput` with `KEYEVENTF_KEYUP` — it triggers `ShortcutState::Released` and breaks push-to-talk

**Windows platform specifics:**
- `windows-sys` v0.59 uses `*mut c_void` for handles (HWND, HHOOK, HINSTANCE), not `isize`
- `#![windows_subsystem = "windows"]` is always set in main.rs — not just release builds
- Focus capture/restore uses `GetForegroundWindow`/`SetForegroundWindow`

**Storage locations:**
- Database: `%APPDATA%\com.echotype.app\echotype.db` (Windows), `~/Library/Application Support/` (macOS), `$XDG_DATA_HOME/` (Linux)
- Logs: same base path under `logs/` — JSON-structured, machine-readable
- Models: `~/models/` — downloaded Whisper binaries verified by SHA256

**Build system:**
- `.env` is sourced by dev/build scripts (sets Cargo bin PATH)
- Vite builds three entry points: `index.html`, `edit-buffer.html`, `overlay.html`
- Rust deps are vendored in `src-tauri/vendor/` but vendoring is currently disabled in `.cargo/config.toml`
- Always use Bun, never npm — lockfile is `bun.lock`, `package-lock.json` is gitignored

**CLI modes:**
```bash
echotype --stdout [--once]     # Pipe transcription to stdout
echotype --list-models         # List installed Whisper models
echotype --set-model <ID>      # Switch active model
echotype --diagnostic          # Print diagnostic info
echotype --log-level <level>   # trace | debug | info | warn | error
```

## Project Layout

```
src/                    Svelte 5 frontend
  lib/components/       UI components (Settings, ModelManager, Dashboard, etc.)
  lib/i18n/             Internationalization
src-tauri/              Rust backend
  src/audio/            Capture, denoise, resample, playback, mute
  src/engine/           STT engines (whisper.rs + cloud/)
  src/dictation/        Dictation state machine, streaming, vocabulary
  src/models/           Model download, manifest, verification
  src/db/               SQLite: migrations, history, metrics, settings, profiles
  src/hotkey/           Global shortcut + Windows key suppression
  src/output/           Text insertion + selection replacement
  src/platform/         OS-specific: permissions, focus, window handling
  src/security/         Keyring integration for API keys
  vendor/               Vendored Rust crates
scripts/agent/          Non-interactive agent commands
docs/                   Product spec, tech stack, milestones, AI operator guide
```

## Docs

- [AI Operator Guide](docs/ai-operator.md) — full agent workflow contract
- [Product Spec](docs/product-spec.md) — feature specifications
- [Tech Stack](docs/tech-stack.md) — architecture details
- [Milestones](docs/milestones.md) — development history

## Contributing

Contributions welcome. Open an issue first for substantial changes.

AI-assisted and agent-authored contributions are encouraged — reviewable, tested, and well-explained commits only.

## License

MIT
