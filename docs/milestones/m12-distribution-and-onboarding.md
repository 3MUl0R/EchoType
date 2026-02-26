# M12: Distribution & Onboarding

First-run wizard, auto-updater, and update channel management. The app guides new users
through setup and keeps itself current.

**Depends on:** M11 (metrics, themes, accessibility audit)

**Delivers:** First-run wizard guides new users through setup in under two minutes.
Auto-updater checks for and applies updates from GitHub Releases. Update channels
separate CPU and GPU builds.

---

## Phase 1: First-Run Setup Wizard

Guided onboarding flow that runs on first launch.

### Work Items

1. **First-run detection and readiness tracking**
   - Check a flag in the database: `wizard_completed` setting
   - If not set (or database doesn't exist yet): launch the wizard
   - Wizard can be re-run from Settings ("Run Setup Wizard Again")
   - **Per-capability readiness flags** (stored alongside `wizard_completed`):
     - `mic_permission_granted` (bool)
     - `platform_permissions_granted` (bool — Accessibility on macOS, portal on Wayland)
     - `model_downloaded` (bool — at least one local model available)
     - `hotkey_configured` (bool)
     - `test_dictation_passed` (bool)
   - Wizard sets `wizard_completed = true` at Step 7 regardless of skips.
   - **Post-wizard readiness check:** if critical capabilities are missing
     (e.g., no model AND no cloud key), show a persistent but dismissible
     "Setup incomplete" banner in the main UI with quick-fix links:
     "Download a model" / "Configure cloud provider" / "Grant mic access".
   - This prevents users from being stuck in a non-functional state with
     no guidance after skipping wizard steps.

2. **Wizard steps**
   - Create `src/lib/components/SetupWizard.svelte`
   - Step flow with progress indicator and back/next navigation:

   **Step 1: Welcome**
   - Brief introduction to EchoType
   - "Let's get you set up" message

   **Step 2: Microphone Permission**
   - Request microphone access
   - Platform-specific guidance if denied:
     - macOS: deep link to Privacy → Microphone settings
     - Windows: deep link to Privacy → Microphone settings
     - Linux: explain ALSA/PulseAudio access (usually no permission needed)
   - Test mic: show input level meter to confirm it's working
   - "Skip" option with warning that dictation won't work

   **Step 3: Platform Permissions**
   - **macOS — Accessibility:**
     - Explain why Accessibility is needed (direct input mode)
     - "Open System Preferences" button (deep link)
     - Poll for permission grant, update UI when granted
     - "Skip" option: app will use clipboard mode instead
   - **Linux Wayland — Global Hotkey Portal:**
     - Explain that Wayland requires compositor portal permission for global
       hotkeys. Without it, hotkey-based modes (hold/toggle) won't work.
     - Attempt to register a test hotkey. If it succeeds: proceed.
     - If it fails: explain the limitation, suggest using VAD mode or
       switching to X11, offer manual profile switching as fallback.
     - "Skip" option: hotkey modes unavailable, VAD still works.
   - Steps are conditionally shown based on the current platform.

   **Step 4: Download a Model**
   - Show recommended model(s) with size and speed labels
   - "Recommended" option: download `ggml-base.en.bin` (good balance)
   - "Smaller" option: `ggml-tiny.en.bin` (fastest, lower accuracy)
   - "Better" option: `ggml-small.en.bin` (higher accuracy, slower)
   - Download progress bar
   - Option to skip if user already has a model or wants to use cloud.
     If skipped: show a mini-step "Configure Cloud Provider" with a link to
     the API key settings page. The test dictation step (Step 6) will use
     the cloud engine if a key is configured and no local model exists.

   **Step 5: Set Hotkey**
   - Show the default hotkey, explain hold-to-dictate
   - "Customize" option to capture a different hotkey
   - Test the hotkey: prompt user to press it, confirm it triggers

   **Step 6: Test Dictation**
   - "Try it now" - hold the hotkey, speak, see the transcription
   - Shows the full dictation flow in the wizard window
   - Confirms everything works end-to-end

   **Step 7: Done**
   - "You're all set!" summary
   - Quick links: Settings, Model Management, Dashboard
   - Set `wizard_completed` flag

3. **Wizard UX**
   - Wizard is a full-page flow (not a modal)
   - Fully keyboard navigable
   - All text externalized for i18n
   - The entire flow should take under two minutes (excluding model download
     time, which varies by network speed. The two-minute target applies to
     the interactive steps: granting permissions, configuring hotkey, running
     the test dictation. Measured with the smallest model pre-cached.)
   - Each step can be skipped (wizard is never blocking)

4. **Verify:**
   - Fresh install launches the wizard automatically
   - Each step works: permission grants, model download, hotkey test
   - Skipping steps works without breaking the app
   - Wizard can be re-run from settings
   - Wizard is keyboard navigable and screen reader compatible

### Files Created

```
src/lib/components/
  SetupWizard.svelte
  WizardStep.svelte
  WizardProgress.svelte
```

---

## Phase 2: Auto-Updater

Check for new versions and apply updates from GitHub Releases.

### Work Items

1. **Tauri updater setup**
   - Enable `tauri-plugin-updater` 2.9.x (already in tech stack)
   - Configure in `tauri.conf.json`:
     - Update endpoint: GitHub Releases JSON manifest URL
     - Public key for signature verification (generated with Tauri signer)
   - Signature verification is mandatory (Tauri enforces this)

2. **Update endpoint**
   - Tauri's updater expects a JSON response at a URL with the latest version info
   - Options:
     - GitHub Releases API (Tauri action generates the manifest automatically)
     - Static JSON file on GitHub Pages (more control)
   - Use the Tauri action's built-in update manifest generation for simplicity

3. **Update channels**
   - **Channel dimensions:** each update channel is identified by three axes:
     - Platform + architecture: `macos-aarch64`, `macos-x86_64`, `windows-x64`,
       `linux-x64`
     - GPU flavor: `cpu`, `metal`, `cuda`, `vulkan`
   - **Endpoint URL pattern:**
     `https://.../{platform}-{arch}/{flavor}/latest.json`
     (e.g., `https://.../macos-aarch64/metal/latest.json`)
   - The app knows its own platform, arch, and flavor at build time
     (compiled in via env vars or Cargo feature flags)
   - **Misrouting prevention:** the updater MUST verify that the update
     manifest's target triple and flavor match the running binary's compiled
     values before applying. Reject any mismatch.
   - A CPU user never receives a CUDA update and vice versa

4. **Update behavior setting**
   - Setting: "Update behavior" with three options:
     - Auto-update: download and install silently on next restart
     - Download-and-prompt (default): download in background, prompt to install
     - Notify-only: show that an update is available, user decides when to download
   - Default is download-and-prompt for trust reasons (privacy-focused tool)
   - **Active dictation protection:** NEVER restart or apply an update while
     dictation is active (recording, transcribing, or edit buffer open). If
     the user clicks "Install Now" during an active session, defer the
     restart until the session completes. Show: "Update will be installed
     after your current dictation."
   - **Update failure handling:**
     - Offline / GitHub API unavailable: silently skip update check, retry
       on next scheduled check. No error shown (offline is normal).
     - Download failure / corrupt download: retry once with backoff, then
       show "Update download failed, will retry later."
     - Signature verification failure: reject the update, log the failure,
       show "Update signature invalid — skipping this update for safety."
     - Low disk space: detect before download, show "Not enough space for
       update (need X MB)."

5. **Update UI**
   - Notification when an update is available (non-intrusive, e.g., badge on tray icon)
   - Update dialog: shows version number, changelog summary, "Install Now" / "Later"
   - Download progress if not already downloaded
   - Post-update: "EchoType has been updated to vX.Y.Z" message on next launch

6. **Verify:**
   - App checks for updates on startup (or on a schedule)
   - Update notification appears when a new version is available
   - Download and install works (test with a staged release)
   - Signature verification rejects tampered updates
   - Update channels route to the correct flavor
   - Update behavior setting controls the flow

### Files Created/Modified

```
src-tauri/tauri.conf.json       # modified: updater configuration
src-tauri/src/updater.rs        # update check, channel routing, behavior setting
src/lib/components/
  UpdateNotification.svelte
  UpdateDialog.svelte
```

---

## Acceptance Criteria

M12 is complete when all of the following are true:

- [ ] First-run wizard launches automatically on fresh install
- [ ] Wizard guides through: mic permission, platform permissions, model download, hotkey, test
- [ ] Linux Wayland: wizard tests global hotkey portal, shows fallback guidance if unavailable
- [ ] Each wizard step can be skipped without breaking the app
- [ ] Skipping critical steps shows persistent "Setup incomplete" banner with fix links
- [ ] Per-capability readiness flags stored (mic, permissions, model, hotkey, test)
- [ ] Wizard takes under two minutes (excluding model download time)
- [ ] Wizard is keyboard navigable and screen reader compatible
- [ ] Wizard can be re-run from settings
- [ ] Cloud-first path works: skip model → configure API key → test with cloud engine
- [ ] Auto-updater checks GitHub Releases for new versions
- [ ] Update signature verification works (rejects tampered updates)
- [ ] Update channels use platform + arch + flavor dimensions
- [ ] Channel misrouting prevented: manifest target triple verified before applying
- [ ] Update behavior setting: auto / download-and-prompt / notify-only
- [ ] Update never restarts during active dictation (deferred until session ends)
- [ ] Update failures handled gracefully (offline, corrupt download, bad signature, low disk)
- [ ] Update notification is non-intrusive (tray badge, not popup)
- [ ] All new UI strings externalized in i18n resource files
- [ ] `./scripts/agent/check` passes
- [ ] CI builds pass on all platforms
