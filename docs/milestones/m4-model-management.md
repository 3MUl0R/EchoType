# M4: Model Management

Model catalog, download manager, and management UI. Users can browse, download, and
switch between Whisper models without touching the filesystem.

**Depends on:** M3 (core dictation loop, engine trait with Whisper implementation)

**Delivers:** Model management page in the UI. Browse available models with metadata
labels, download with progress bar, verify checksums, switch active model, delete
unused models.

---

## Phase 1: Model Manifest

Define the model catalog format and create the initial manifest.

### Work Items

1. **Manifest schema**
   - Create `models/manifest.json` in the repo root
   - Schema per model entry:
     ```json
     {
       "id": "whisper-tiny-en",
       "name": "Whisper Tiny (English)",
       "engine": "whisper",
       "file": "ggml-tiny.en.bin",
       "size_bytes": 77704715,
       "sha256": "...",
       "url": "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin",
       "languages": ["en"],
       "speed_tier": "fast",
       "accuracy_tier": "basic",
       "description": "Smallest English-only model. Fast on any hardware."
     }
     ```
   - Include tiers: tiny, base, small, medium (English and multilingual variants)
   - `speed_tier`: fast / moderate / slow
   - `accuracy_tier`: basic / good / best
   - Large-v3 is the biggest practical model -- include it but label clearly

2. **Manifest loader**
   - Create `src-tauri/src/models/manifest.rs`
   - Load manifest from the repo (bundled in the app) or fetch from GitHub raw URL
   - Parse with `serde_json`
   - Version field in manifest for future updates
   - Fallback: if fetch fails, use bundled manifest
   - **Manifest trust:** the bundled manifest is the trust anchor (it ships with the
     signed app binary). Remote manifests are used for catalog freshness only -- they
     cannot override checksums of already-installed models. If a remote manifest entry
     changes the `sha256` for an existing model ID, treat it as an update (not silent
     replacement). Consider adding a manifest signature field for future remote trust.
   - **URL validation rules:**
     - HTTPS only (reject HTTP URLs)
     - Allowlisted hosts only: `huggingface.co` (expandable list)
     - Filename sanitization: reject path traversal (`../`), null bytes, or
       absolute paths in the `file` field
     - Do not follow redirects to non-allowlisted hosts

3. **Verify:** Manifest parses correctly, model entries have all required fields.

### Files Created

```
models/manifest.json
src-tauri/src/models/
  mod.rs
  manifest.rs
```

---

## Phase 2: Download Manager

Download models from Hugging Face with progress reporting, resumable downloads, and
integrity verification.

### Work Items

1. **Add networking dependencies** (if not already present)
   - `reqwest` 0.13.x should already be in Cargo.toml from M1 or add now
   - Enable `stream` and `rustls-tls` features
   - Re-run `cargo vendor vendor/`

2. **Download manager**
   - Create `src-tauri/src/models/download.rs`
   - **Pre-download checks:**
     - Check available disk space before starting (require at least model size + margin)
     - Prevent duplicate concurrent downloads of the same model (download lock per model ID)
     - Clean up stale `.partial` files on startup (older than 24 hours)
   - Download to a `.partial` temp file, rename on completion
   - Stream the response body, write chunks to disk
   - Progress reporting: emit Tauri events with bytes downloaded / total bytes
   - **Resumable downloads:**
     - Check for existing `.partial` file
     - Store `ETag` and `Content-Length` alongside `.partial` file (in a `.meta` sidecar)
     - On resume: send `Range: bytes={size}-` + `If-Range: {etag}` header
     - If server returns 206: append. If 200 (content changed): discard partial, restart
     - If `Content-Range` in response doesn't match expectations: discard and restart
   - Cancel support: accept a cancellation token, clean up partial file on cancel

3. **Checksum verification**
   - After download completes, compute SHA-256 of the file (`sha2` crate)
   - Compare against the manifest's `sha256` field
   - If mismatch: delete the file, report error to frontend
   - If match: rename `.partial` → final filename

4. **Model storage**
   - Store models in Tauri app data directory / `models/`
   - Create the directory on first use
   - Track installed models: scan the models directory and match against manifest

5. **Model update workflow**
   - Compare installed file checksum against manifest checksum
   - If manifest has a newer checksum for the same model ID: flag as "update available"
   - Update = re-download (no incremental patching)

6. **Error handling and offline UX**
   - **Download errors:** timeout, network failure, server error → show clear error
     message with retry button. No silent failures.
   - **Checksum mismatch:** delete the download, show error explaining the file was
     corrupted, offer retry
   - **Disk full:** detect and show "not enough disk space" with required space info
   - **Offline with no models installed:** show prominent message explaining a model
     is required for dictation, with instructions to connect to the internet
   - **Offline with models installed:** app works fully offline, manifest refresh
     silently skipped

7. **Verify:**
   - Download a model from Hugging Face, progress events fire
   - Interrupt a download, resume it, verify it completes correctly
   - Checksum verification catches a corrupted file (test with tampered hash)
   - Downloaded model loads in WhisperEngine successfully
   - Cancel mid-download cleans up partial file

### Files Created

```
src-tauri/src/models/
  download.rs     # streaming download, resume, cancel, progress events
```

---

## Phase 3: Model Management UI

Frontend page for browsing, downloading, and managing models.

### Work Items

1. **Model management page**
   - Create `src/lib/components/ModelManager.svelte`
   - Two sections: "Available Models" (from manifest) and "Installed Models"
   - Each model card shows: name, size, speed tier, accuracy tier, languages, description
   - Available models: "Download" button, progress bar during download, cancel button
   - Installed models: "Active" badge on current model, "Delete" button, size on disk
   - Models with updates available: "Update" badge

2. **Tauri commands for model management**
   - `list_available_models`: return manifest entries with install status
   - `list_installed_models`: return installed models with metadata
   - `download_model(model_id)`: start download, return immediately (progress via events)
   - `cancel_download(model_id)`: cancel an active download
   - `delete_model(model_id)`: delete the model file
   - `set_active_model(model_id)`: switch the engine to use this model
   - `get_active_model`: return the currently active model ID

3. **Engine switching**
   - When `set_active_model` is called:
     1. **Check if dictation is active** -- if so, reject the switch with an error
        (do not unload a model mid-transcription)
     2. Unload current model from WhisperEngine
     3. Load the new model file
     4. Update engine manager
     5. Persist the selection (stored in memory for now, M5 adds SQLite persistence)
   - Switching is not instant -- show loading indicator in UI
   - **Delete guard:** `delete_model` must refuse to delete the currently active model.
     User must switch to another model first. UI disables the delete button on the
     active model.

4. **Language selection**
   - Multilingual models support language selection
   - Show a language dropdown when a multilingual model is active
   - Pass selected language to the engine on transcription
   - English-only models: language dropdown hidden or disabled

5. **Routing / navigation**
   - Add basic page navigation: main dictation page vs. model management page
   - Simple tab or sidebar navigation (will be extended in later milestones)
   - Keep it minimal -- full settings UI comes in M5

6. **Verify:**
   - Browse model catalog in the UI with metadata labels
   - Download a model, see progress bar, verify it completes
   - Switch active model, perform a transcription with the new model
   - Delete an unused model
   - Language dropdown works with multilingual models
   - All strings externalized

### Files Created

```
src/lib/components/
  ModelManager.svelte
  ModelCard.svelte
src/lib/i18n/en.json          # modified: add model management strings
```

---

## Acceptance Criteria

M4 is complete when all of the following are true:

- [ ] Model manifest defines available Whisper models with metadata (size, speed, accuracy, languages)
- [ ] Manifest loads from bundled file with optional refresh from GitHub
- [ ] Manifest URL validation: HTTPS only, allowlisted hosts, sanitized filenames
- [ ] Download manager streams models from Hugging Face with progress reporting
- [ ] Downloads are resumable (with ETag/Content-Range validation)
- [ ] Cancel mid-download cleans up partial files
- [ ] SHA-256 checksum verification runs after every download
- [ ] Corrupted downloads are detected and rejected
- [ ] Disk space checked before download starts
- [ ] Model management UI shows available and installed models
- [ ] Model cards display size, speed tier, accuracy tier, and supported languages
- [ ] Models can be downloaded, deleted, and switched via the UI
- [ ] Active model cannot be deleted (UI prevents it)
- [ ] Model switching blocked during active transcription
- [ ] Update-available indicator appears when manifest has newer checksum
- [ ] Language selection dropdown works for multilingual models
- [ ] Engine switching unloads old model and loads new one via the engine trait
- [ ] Offline with no models: clear message guiding user to connect and download
- [ ] Model management UI is keyboard navigable and screen-reader accessible
- [ ] Vendor directory updated with any new crates
- [ ] `./scripts/agent/check` passes
- [ ] CI builds pass on all platforms
