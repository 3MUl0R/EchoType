# M1: Project Foundation

Tauri 2 + Svelte 5 project scaffold with build system, vendored dependencies, CI
pipeline, agent scripts, and structured logging.

**Depends on:** Nothing. This is the starting point.

**Delivers:** App launches (empty window), builds pass in CI on all platforms, agent
scripts work end-to-end, cross-cutting standards are in place.

---

## Phase 1: Project Scaffold

Initialize the Tauri 2 + Svelte 5 + Vite project and get a window on screen.

### Work Items

1. **Initialize Tauri 2 project**
   - Use `create-tauri-app` or manual setup with Svelte 5 + Vite template
   - Target Tauri 2 stable (not v1)
   - Ensure `src-tauri/` and `src/` directory structure matches tech stack spec

2. **Svelte 5 + Vite frontend**
   - Plain Svelte 5 with runes reactivity (not SvelteKit)
   - `vite.config.ts` with Svelte plugin
   - `index.html` → `src/main.ts` → `App.svelte` entry chain
   - Minimal `App.svelte` that renders a placeholder page

3. **Tailwind CSS v4 setup**
   - Install `tailwindcss` and `@tailwindcss/vite`
   - Add Tailwind Vite plugin to `vite.config.ts`
   - Create `src/app.css` with `@import "tailwindcss"` and `@theme {}` block
   - Import `app.css` in `main.ts`
   - Verify Tailwind utility classes render in the placeholder page

4. **Tauri configuration**
   - `src-tauri/tauri.conf.json`: app name, identifier, window config
   - `src-tauri/capabilities/default.json`: minimal default permissions
   - Window config: single main window, reasonable default size, title "EchoType"
   - Disable dock icon on macOS (`"visible": false` in dock config) -- placeholder
     for full tray-only behavior in M6. Until then, the app is accessible via
     `Cmd+Tab` and the window itself; this just reduces visual clutter early.

5. **Verify:** `bun run tauri dev` launches a window with the Tailwind-styled
   placeholder page.

### Files Created

```
package.json
package-lock.json
vite.config.ts
tsconfig.json
index.html
src/
  main.ts
  app.css
  App.svelte
  lib/
    components/          # empty, structure only
src-tauri/
  Cargo.toml
  build.rs
  tauri.conf.json
  capabilities/
    default.json
  icons/                 # default Tauri icons (replace later)
  src/
    main.rs
    lib.rs
```

---

## Phase 2: Rust Foundation

Set up the Rust crate, vendor dependencies, configure structured logging, and
create the CLI skeleton.

### Work Items

1. **Cargo dependencies** (single crate, not a Cargo workspace)
   - `src-tauri/Cargo.toml` with initial dependencies:
     - `tauri` (with `tray-icon` feature for later use)
     - `tracing`, `tracing-subscriber` (with `env-filter`), `tracing-appender`
     - `clap` (with `derive` feature)
     - `serde`, `serde_json`
   - No audio, STT, or database crates yet -- those arrive in later milestones

2. **Vendor dependencies**
   - Run `cargo vendor vendor/` from repo root
     (or `cargo vendor --manifest-path src-tauri/Cargo.toml vendor/`)
   - Creates repo-root `vendor/` directory per tech stack spec
   - Create `.cargo/config.toml` at repo root with vendored source configuration:
     ```toml
     [source.crates-io]
     replace-with = "vendored-sources"

     [source.vendored-sources]
     directory = "vendor"
     ```
   - Verify `cargo build` works entirely from vendored sources (no network)
   - Add `vendor/` to the repo (this is intentional per dependency management policy)

3. **Structured logging**
   - Initialize `tracing-subscriber` in `main.rs` with:
     - JSON format for machine-readable output
     - `env-filter` for configurable verbosity (`ECHOTYPE_LOG` env var)
     - `tracing-appender::rolling::daily` for file output
     - `tracing_appender::non_blocking` wrapper (hold `WorkerGuard` for app lifetime)
   - Log file location: Tauri app data dir / `logs/`
   - Log cleanup: on startup, delete log files older than 3 days
   - Log to both file (JSON) and stderr (human-readable, for dev)
   - Required JSON fields per log entry: `timestamp` (ISO 8601), `level`, `target`
     (subsystem/module), `message`. Additional structured fields are encouraged.
   - Add startup log entries: app version, platform, log path

4. **CLI skeleton with clap**
   - Define a `Cli` struct with `clap::Parser` derive
   - Support `--version` and `--help` (clap handles these automatically)
   - Placeholder flags for future: `--daemon`, `--stdout`, `--log-level`
   - CLI parsing happens before Tauri app startup
   - If `--daemon` or `--stdout` are passed, print "not yet implemented" and exit

5. **Tauri command scaffold**
   - Register an empty `greet` or `ping` command to verify IPC works
   - Frontend calls the command on load, displays the result
   - This proves the Rust ↔ JS bridge is functional

6. **Verify:**
   - `cargo build` succeeds from vendored sources with no network
   - App starts and writes structured JSON logs to the expected path
   - `echotype --version` prints the version and exits
   - Frontend successfully calls the Rust command via IPC

### Files Created/Modified

```
.cargo/config.toml           # vendored source config
vendor/                      # vendored crate sources (repo root, per tech stack)
src-tauri/src/main.rs         # CLI parsing, logging init, Tauri launch
src-tauri/src/lib.rs          # Tauri command registration, ping command
```

---

## Phase 3: Testing and Cross-Cutting Foundations

Set up the testing infrastructure and establish the cross-cutting patterns that all
future milestones build on. This phase runs before agent scripts so the `check` script
has real tests to call.

### Work Items

1. **Rust testing scaffold**
   - Add `#[cfg(test)]` module in `lib.rs` with a trivial passing test
   - Verify `cargo test` runs and passes
   - Enable the `test` feature on the `tauri` crate for `MockRuntime` access

2. **Frontend testing with Vitest**
   - Install `vitest` and `@testing-library/svelte` as dev dependencies
   - Create `vitest.config.ts` (or configure in `vite.config.ts`)
   - Write a trivial `App.test.ts` that renders `App.svelte` and checks it mounts
   - Install `@tauri-apps/api/mocks` for mocking Tauri IPC in tests
   - Verify `bun run test` runs and passes

3. **Playwright E2E scaffold**
   - Install `@playwright/test` as a dev dependency
   - Create `playwright.config.ts`: test against `http://localhost:1420` (Vite dev server)
   - Create `tests/e2e/app.spec.ts`: navigate to dev server, verify page loads
   - Add `"test:e2e"` script to `package.json`
   - Note: E2E tests run against Vite dev server with mocked IPC, not the full Tauri app

4. **Linting**
   - Rust: `cargo fmt` and `cargo clippy`
   - Frontend: configure ESLint or equivalent for Svelte/TS
   - Add lint commands to `package.json`

5. **String externalization (i18n scaffolding)**
   - Choose a lightweight approach for Svelte string externalization
   - Create `src/lib/i18n/` with:
     - `en.json`: English strings (start with app title, placeholder text)
     - `index.ts`: helper function to look up strings by key
   - Use in `App.svelte` to demonstrate the pattern
   - This is the pattern all future UI work follows -- no hardcoded user-facing strings

6. **Accessibility baseline**
   - `App.svelte` shell layout uses semantic HTML (`<main>`, `<nav>`, `<header>`)
   - Add `lang="en"` to `index.html`
   - Set up logical tab order in the shell layout
   - Document the accessibility contract in a code comment or `CONTRIBUTING.md` note:
     every UI component must support keyboard navigation and screen reader

7. **Scripts in package.json**
   - `"dev"`: `tauri dev`
   - `"build"`: `tauri build`
   - `"test"`: `vitest run`
   - `"lint"`: ESLint command configured above

8. **Verify:**
   - `cargo test` passes in `src-tauri/`
   - `bun run test` (Vitest) passes
   - `bun run test:e2e` (Playwright) passes against dev server
   - `bun run lint` passes
   - i18n helper resolves string keys correctly
   - Tab key navigates through the shell layout logically

### Files Created/Modified

```
src-tauri/src/lib.rs          # add test module
vitest.config.ts              # (or inline in vite.config.ts)
src/App.test.ts
tests/e2e/app.spec.ts
playwright.config.ts
src/lib/i18n/
  en.json
  index.ts
```

---

## Phase 4: Agent Scripts and CI

Create the agent command contract scripts and the GitHub Actions CI pipeline. All
testing tools from Phase 3 are now in place, so the `check` script can exercise
everything from the start.

### Work Items

1. **Agent scripts**
   - Create `scripts/agent/` directory
   - All scripts are Bash (`#!/usr/bin/env bash`), executable
   - **Windows note:** Bash is required. On Windows, use Git Bash (ships with Git
     for Windows) or WSL. This is a documented requirement in the README/CONTRIBUTING.
   - Each script exits with deterministic codes: 0 = success, non-zero = failure
   - All output must be machine-parseable: structured status lines on stdout,
     free-form diagnostics on stderr only
   - All scripts must be non-interactive (no prompts, no user input required)

   **`scripts/agent/bootstrap`:**
   - Check for required toolchains: `rustc`, `cargo`, `bun`
   - Check for Rust components: `rustfmt`, `clippy` (install via `rustup component add`)
   - Check for platform-specific Tauri build dependencies:
     - Linux: `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev`, etc.
     - macOS: Xcode Command Line Tools
     - Windows: WebView2 (usually pre-installed on Windows 10+)
   - Print clear error messages if anything is missing, with install hints
   - Run `bun install --frozen-lockfile` to install frontend dependencies from lockfile
   - Run `cargo check --manifest-path src-tauri/Cargo.toml` to verify Rust compilation
   - Idempotent: safe to run repeatedly; re-running changes nothing if already set up
   - Exit 0 if everything succeeds

   **`scripts/agent/dev`:**
   - Run `bun run tauri dev` (or equivalent)
   - Pass through any arguments
   - Exit with the child process exit code

   **`scripts/agent/check`:**
   - Run `cargo fmt --check --manifest-path src-tauri/Cargo.toml`
   - Run `cargo clippy --manifest-path src-tauri/Cargo.toml`
   - Run `cargo test --manifest-path src-tauri/Cargo.toml`
   - Run `bun run lint`
   - Run `bun run test` (Vitest)
   - Print summary: X passed, Y failed
   - Exit non-zero if any check fails

   **`scripts/agent/logs`:**
   - Find and print the most recent log file from the app data directory
   - Platform-aware: detect the Tauri app data path per OS
   - If no logs exist, print a helpful message and exit 0
   - Support `--tail N` flag to limit output (default: last 50 lines)

   **`scripts/agent/fix`:**
   - Run `cargo fmt --manifest-path src-tauri/Cargo.toml` (auto-fix formatting)
   - Run `bun run lint -- --fix`
   - Print what was changed
   - Exit 0 if fixes applied, exit 1 if unfixable issues remain

2. **GitHub Actions CI workflow**
   - `.github/workflows/ci.yml`
   - Trigger: push to main, pull requests
   - Build matrix: macOS (aarch64, x86_64), Windows (x64), Linux (x64)
   - Steps per platform:
     1. Checkout repo
     2. Setup Rust toolchain (`dtolnay/rust-toolchain@stable`)
     3. Rust cache (`swatinem/rust-cache@v2`)
     4. Setup Node.js
     5. Install platform-specific system dependencies (apt/brew as needed)
     6. Run `./scripts/agent/bootstrap`
     7. Run `./scripts/agent/check`
     8. Build with `tauri-action` (handles platform packaging + macOS universal builds)
   - Separate audit job (can run on one platform):
     - `cargo audit` (install `cargo-audit` first)
     - `bun pm audit`
   - **Windows CI note:** Agent scripts run in Git Bash (default shell for bash steps
     in GitHub Actions on Windows)

3. **Verify:**
   - All five agent scripts run successfully on the local machine
   - `./scripts/agent/bootstrap && ./scripts/agent/check` exits 0
   - Each script produces parseable output and deterministic exit codes
   - Scripts require no interactive input
   - CI workflow definition is valid YAML and covers all platforms

### Files Created

```
scripts/agent/bootstrap
scripts/agent/dev
scripts/agent/check
scripts/agent/logs
scripts/agent/fix
.github/workflows/ci.yml
```

---

## Acceptance Criteria

M1 is complete when all of the following are true:

- [ ] `bun run tauri dev` launches a window with a Tailwind-styled page
- [ ] `cargo build --manifest-path src-tauri/Cargo.toml` succeeds from vendored sources (no network)
- [ ] Structured JSON logs are written to the app data directory on startup
- [ ] Each log entry contains required fields: `timestamp`, `level`, `target`, `message`
- [ ] Log files older than 3 days are cleaned up on startup
- [ ] `echotype --version` prints the version and exits
- [ ] `echotype --help` prints usage and exits
- [ ] Frontend successfully calls a Rust command via Tauri IPC
- [ ] All five agent scripts (`bootstrap`, `dev`, `check`, `logs`, `fix`) work
- [ ] Agent scripts are non-interactive (no prompts, no stdin reads)
- [ ] Agent scripts produce parseable output on stdout, diagnostics on stderr
- [ ] Agent scripts have deterministic exit codes (0 = success, non-zero = failure)
- [ ] `./scripts/agent/check` exits 0 (fmt, clippy, cargo test, lint, vitest all pass)
- [ ] `./scripts/agent/logs` prints recent log output (or helpful empty-state message)
- [ ] `./scripts/agent/bootstrap` is idempotent (re-running succeeds without side effects)
- [ ] GitHub Actions CI builds on macOS, Windows, and Linux
- [ ] `cargo audit` and `bun pm audit` run in CI
- [ ] Vitest renders App.svelte and passes
- [ ] Playwright loads the dev server and passes
- [ ] UI strings use the i18n helper (no hardcoded user-facing text)
- [ ] Shell layout uses semantic HTML and supports keyboard navigation
- [ ] `.cargo/config.toml` at repo root points at vendored sources
- [ ] `vendor/` directory at repo root is committed to the repo
