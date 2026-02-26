# M13: Packaging & Release

Platform-specific packages, package manager listings, code signing, release signing,
project website, and diagnostic tooling. The app is ready for public distribution.

**Depends on:** M12 (first-run wizard, auto-updater)

**Delivers:** Downloadable signed packages on all platforms. Package manager installs
work. GitHub Pages site live. Diagnostic self-checks in settings.

---

## Phase 1: Platform Installers

Build and publish installers for each platform.

### Work Items

1. **CI build pipeline**
   - Expand `.github/workflows/ci.yml` (or create a release workflow):
   - Build matrix:
     - macOS: aarch64 (Apple Silicon), x86_64 (Intel)
     - Windows: x64
     - Linux: x64
   - Each platform produces its installer artifacts
   - Use `tauri-apps/tauri-action@v0` for builds and bundling
   - Artifacts uploaded to GitHub Releases on tagged commits

2. **macOS: DMG**
   - Tauri produces a `.dmg` disk image by default
   - Contains the `.app` bundle with drag-to-Applications layout
   - Universal binary (aarch64 + x86_64) or separate builds per architecture
   - App icon, background image for the DMG window

3. **Windows: MSI**
   - Tauri produces an `.msi` installer via WiX
   - Installer includes: app binary, tray icon assets, audio assets
   - Start menu shortcut, optional desktop shortcut
   - Uninstaller registered in Add/Remove Programs

4. **Linux: AppImage, DEB, RPM**
   - Tauri produces `.deb` and `.AppImage` by default
   - `.rpm` may require additional configuration or a post-build step
   - AppImage: self-contained, runs on most distributions
   - `.deb`: for Debian/Ubuntu (largest Linux desktop share)
   - `.rpm`: for Fedora/RHEL

5. **GPU build flavors**
   - **GA (required for release):** CPU builds are the default for all platforms.
     These are the primary distribution artifacts.
   - **macOS Metal:** Metal is the default GPU backend on macOS (no extra SDK
     needed at build time). The macOS CPU build can include Metal support
     natively, making it the default macOS artifact.
   - **Optional/experimental (not required for GA):** CUDA and Vulkan builds
     require specific SDKs/toolkits on CI runners:
     - CUDA: requires NVIDIA CUDA toolkit. CI runner must have it installed
       or use a Docker image with CUDA. Only for Windows and Linux.
     - Vulkan: requires Vulkan SDK. Only for Windows and Linux.
   - **Strategy:** ship CPU (+ Metal on macOS) as the GA package. GPU builds
     are published as additional artifacts on GitHub Releases with clear
     "experimental" labeling. They do NOT need to be in package managers
     initially.
   - Artifact naming convention: `echotype-{os}-{arch}.{ext}` (default/CPU)
     and `echotype-{flavor}-{os}-{arch}.{ext}` (GPU variants)
   - CI matrix entries per GPU flavor only when runner toolchain is available

6. **Verify:**
   - All installer types build in CI
   - macOS DMG installs and launches correctly
   - Windows MSI installs, creates shortcuts, uninstalls cleanly
   - Linux AppImage runs, DEB installs on Ubuntu
   - GPU flavors build with correct feature flags

---

## Phase 2: Code Signing and Release Signing

Sign binaries and releases for trust.

### Work Items

1. **macOS code signing and notarization**
   - **Prerequisite:** Apple Developer Program membership ($99/year). This is
     a hard requirement for Gatekeeper-approved distribution. Without it,
     macOS shows "unidentified developer" warning which kills adoption.
   - **Required for GA:** signing certificate obtained and configured in CI.
   - **Interim (pre-GA):** if certificate is not yet available, document the
     manual workaround for users (right-click → Open → bypass Gatekeeper)
     and track certificate acquisition as a blocking release issue.
   - Tauri action handles signing when `APPLE_CERTIFICATE` and related secrets
     are set
   - Notarization: submit to Apple's notary service via `notarytool`
   - Certificate options: Apple Developer ID, or SignPath Foundation for OSS

2. **Windows Authenticode signing**
   - **Required for GA:** Authenticode certificate configured in CI. Without
     it, SmartScreen shows "unknown publisher" warning.
   - **Interim (pre-GA):** if certificate not yet available, document the
     SmartScreen bypass and track as blocking release issue.
   - Tauri action handles signing when `WINDOWS_CERTIFICATE` secrets are set
   - Certificate options: paid CA (e.g., DigiCert), or SignPath Foundation for OSS

3. **Release signing (checksums + GPG + Tauri updater)**
   - For each release on GitHub Releases:
     - Generate SHA-256 checksums for all artifacts
     - Publish a `checksums.txt` file alongside the binaries
     - GPG-sign the checksums file (requires a project GPG key)
   - CI step: compute checksums, sign with GPG key from CI secret
   - Users can verify: `sha256sum -c checksums.txt` and `gpg --verify checksums.txt.sig`
   - **Tauri updater signatures:** the Tauri updater requires its own signing
     key (separate from code signing). The private key
     (`TAURI_SIGNING_PRIVATE_KEY`) is a CI secret. The public key is embedded
     in `tauri.conf.json`. Each release artifact includes a `.sig` file that
     the updater verifies before applying. This is mandatory and handled by
     the Tauri build action.
   - **Key lifecycle:** document the key generation, secure backup, and
     rotation process. If the signing key is compromised: revoke, generate
     new key, publish a signed advisory, and ship a manual-install update
     with the new key.

4. **Verify:**
   - macOS app installs without Gatekeeper warnings (when signed)
   - Windows app installs without SmartScreen warnings (when signed)
   - Checksums file matches all release artifacts
   - GPG signature verifies correctly

---

## Phase 3: Package Manager Listings

Make EchoType installable via popular package managers.

### Work Items

1. **WinGet (Windows)**
   - **PackageIdentifier:** `EchoType.EchoType` (Publisher.Package format
     required by WinGet). If an org is established, use `OrgName.EchoType`.
   - Create a WinGet manifest (YAML format) with required fields:
     PackageIdentifier, PackageVersion, Installers (URL, SHA256, type MSI)
   - Validate locally with `winget validate --manifest <path>`
   - Submit PR to `microsoft/winget-pkgs` repository
   - **Update process:** on each GitHub Release, a CI step or
     `wingetcreate update` command generates a new manifest version and
     submits a PR to winget-pkgs.

2. **Homebrew (macOS)**
   - Create a Homebrew cask for `brew install --cask echotype`
   - Cask must pass `brew audit --cask echotype` and `brew style` checks
   - Submit PR to `homebrew/homebrew-cask`
   - Cask points to the `.dmg` on GitHub Releases with SHA256 verification
   - **Update process:** on each release, submit a bump PR (automated via
     `brew bump-cask-pr` in CI, or manually). Homebrew requires the cask
     to be updated via PR -- it does not auto-track GitHub Releases.

3. **Flatpak / Flathub (Linux)**
   - Create a Flatpak manifest for EchoType
   - **App ID:** use a verified domain-based ID (e.g., `io.github.echotype.EchoType`
     if using GitHub Pages, or update if a custom domain is configured later).
     Flathub requires reverse-DNS IDs tied to a domain the project controls.
   - Submit to Flathub via PR to the Flathub repository
   - Flatpak packages the app with its own runtime (sandboxed)
   - **Portal permissions required:**
     - `--device=all` or `--device=dri` (for audio devices)
     - PulseAudio / PipeWire access for microphone
     - `org.freedesktop.portal.GlobalShortcuts` for hotkey registration
   - **Update process:** Flathub does NOT automatically track GitHub Releases.
     Each release requires a PR updating the Flathub manifest with the new
     version URL and checksum. Automate this with a CI step that creates the
     Flathub PR after a GitHub Release is published.

4. **Verify:**
   - `winget install echotype` installs the app on Windows
   - `brew install --cask echotype` installs the app on macOS
   - `flatpak install` installs the app on Linux
   - Package manager versions match the latest GitHub Release

---

## Phase 4: Project Website

GitHub Pages site for documentation and download links.

### Work Items

1. **GitHub Pages setup**
   - Enable GitHub Pages on the repo (deploy from `gh-pages` branch or `/docs`)
   - Custom domain: configure DNS for the project domain (TBD)
   - Static site generator: minimal (plain HTML + CSS, or a lightweight tool)

2. **Website content**
   - Landing page: tagline, key features, download buttons per platform
   - Download page: links to latest release artifacts, checksums, GPG key
   - Documentation: link to docs/ in the repo (or render them on the site)
   - Getting started guide (mirrors the first-run wizard flow)

3. **Verify:**
   - Website loads at the configured domain
   - Download links point to the latest release
   - Documentation is up to date

---

## Phase 5: Diagnostic Tooling

In-app self-checks and diagnostic info collection for bug reports.

### Work Items

1. **"Copy Diagnostic Info" button**
   - In Settings, add a "Copy Diagnostic Info" button
   - Collects and formats:
     - OS name and version
     - EchoType version, build flavor, and git commit hash
     - Active model name and size
     - Audio input device name and driver
     - GPU backend in use
     - Platform permissions: mic (granted/denied), Accessibility (macOS),
       global hotkey portal (Wayland)
     - Update channel and last update check time
     - Last 20 lines of the log file
     - Last panic/crash info if available (from panic hook or log)
   - **Redaction:** API keys, file paths containing usernames, and any
     content from dictation history are NEVER included in diagnostic output.
     Redact automatically before copying.
   - Copies to clipboard as a formatted text block (Markdown-compatible)
   - Also available via CLI: `echotype --diagnostic` prints the same info
     to stdout (useful for bug reports from headless/daemon mode)
   - Designed for pasting into GitHub issues

2. **Test microphone self-check**
   - In Settings: "Test Microphone" button
   - Records 3 seconds of audio
   - Displays: input level, device name, sample rate, channel count
   - Plays back the recording
   - Reports: pass (audio captured) or fail (silence, no device)

3. **Test transcription self-check**
   - In Settings: "Test Transcription" button
   - Records a short clip, runs it through the active engine
   - Displays: transcription result, time taken, engine used
   - Reports: pass (got text) or fail (engine error)

4. **Internationalization completion**
   - Review all UI strings for externalization coverage
   - Set up translation file structure: `src/lib/i18n/{locale}.json`
   - Document the translation contribution workflow
   - Ship with English only, but infrastructure supports adding languages

5. **Verify:**
   - Diagnostic info includes all expected fields
   - Diagnostic output is copy-pasteable into a GitHub issue
   - Mic test reports correctly for working and disconnected mics
   - Transcription test completes with a result
   - All UI strings are externalized

### Files Created

```
src/lib/components/
  DiagnosticInfo.svelte
  SelfCheck.svelte
```

---

## Acceptance Criteria

M13 is complete when all of the following are true:

- [ ] macOS DMG builds and installs correctly (both architectures)
- [ ] Windows MSI builds, installs, and uninstalls correctly
- [ ] Linux AppImage, DEB, and RPM build and install correctly
- [ ] CPU builds are GA-ready on all platforms
- [ ] macOS Metal build included in default macOS artifact
- [ ] CUDA and Vulkan builds published as experimental artifacts (when runner available)
- [ ] macOS binary is signed and notarized (blocking for GA release)
- [ ] Windows binary is Authenticode-signed (blocking for GA release)
- [ ] SHA-256 checksums published alongside every release
- [ ] GPG signature on checksums file
- [ ] Tauri updater `.sig` files included for all release artifacts
- [ ] Tauri signing key generation and backup process documented
- [ ] WinGet manifest submitted with validated PackageIdentifier
- [ ] Homebrew cask submitted and passes `brew audit`
- [ ] Flathub manifest submitted via PR with correct app ID and portal permissions
- [ ] Package manager update automation in CI (WinGet/Homebrew/Flathub PRs on release)
- [ ] GitHub Pages site live with download links and documentation
- [ ] "Copy Diagnostic Info" collects all expected fields including commit hash and build flavor
- [ ] Diagnostic output redacts API keys and user-identifying paths
- [ ] `echotype --diagnostic` CLI command works for headless mode
- [ ] "Test Microphone" self-check works
- [ ] "Test Transcription" self-check works
- [ ] All UI strings externalized, translation infrastructure in place
- [ ] `./scripts/agent/check` passes
- [ ] CI builds pass on all platforms
