# M5: Persistence & Settings

SQLite database, settings storage, dictation history, and settings UI. The app
remembers your preferences and past dictations.

**Depends on:** M4 (model management, engine switching)

**Delivers:** Settings persist across restarts. Dictation history with audio playback.
Configuration UI for all existing settings. GPU backend selection with hardware
detection.

---

## Phase 1: Database Setup

Initialize SQLite with schema migrations and the core tables.

### Work Items

1. **Database initialization**
   - Create `src-tauri/src/db/mod.rs`
   - Open SQLite database via `rusqlite` with `bundled` feature
   - Default location: Tauri app data directory / `echotype.db`
   - Create database file and parent directories on first run
   - Use WAL mode for concurrent read access

2. **Schema migrations**
   - Create `src-tauri/src/db/migrations.rs`
   - Use `rusqlite_migration` with `user_version` pragma
   - Initial migration (v1) creates tables for settings, history, and model preferences.
     Metrics tables are deferred to M11 (separate migration).

   **`settings` table:**
   - `key` TEXT PRIMARY KEY, `value` TEXT (JSON-encoded values)
   - Flexible key-value store for all app settings
   - The `active_model_id` setting is the **single source of truth** for active model

   **`dictation_history` table:**
   - `id` INTEGER PRIMARY KEY
   - `text` TEXT NOT NULL
   - `audio_path` TEXT (relative path to saved audio file within data root, nullable)
   - `duration_ms` INTEGER
   - `engine_id` TEXT
   - `language` TEXT
   - `words_per_minute` REAL
   - `created_at` TEXT NOT NULL (ISO 8601 UTC)
   - `is_private` BOOLEAN DEFAULT 0
   - **Index:** `CREATE INDEX idx_history_created_at ON dictation_history(created_at)`
     (supports retention sweep and paginated queries)

   **`model_preferences` table:**
   - `model_id` TEXT PRIMARY KEY
   - `language` TEXT (per-model preferred language)
   - `last_used_at` TEXT
   - Note: no `is_active` column here -- active model is tracked in `settings` table
     under the key `active_model_id` (single source of truth)

3. **Database access layer**
   - Create `src-tauri/src/db/settings.rs`: get/set settings by key
   - Create `src-tauri/src/db/history.rs`: insert, query, delete dictation entries
   - Create `src-tauri/src/db/models.rs`: model preference CRUD
   - All database access is synchronous (fine for single-user desktop app)
   - Database handle managed as Tauri state (`Arc<Mutex<Connection>>`)

4. **Verify:**
   - Database file is created on first launch
   - Migrations run without error
   - Settings can be saved and retrieved across app restarts
   - `cargo test` includes database tests (use in-memory SQLite for tests)

### Files Created

```
src-tauri/src/db/
  mod.rs            # init, connection management
  migrations.rs     # schema migrations
  settings.rs       # settings key-value store
  history.rs        # dictation history CRUD
  models.rs         # model preference CRUD
```

---

## Phase 2: Settings Storage and Retrieval

Persist all existing configurable values and wire them into the app.

### Work Items

1. **Settings keys and defaults**
   - Define an enum or constants for all settings keys:
     - `hotkey` (string, default: platform-appropriate shortcut)
     - `output_method` (enum: direct_input / clipboard_paste / clipboard_only)
     - `active_model_id` (string)
     - `language` (string, default: "en")
     - `gpu_backend` (enum constrained to backends compiled into this artifact,
       e.g., a Metal build shows only cpu/metal. Default: best available in the build.)
     - `denoise_enabled` (bool, default: true)
     - `log_level` (enum: error / warn / info / debug / trace)
   - Each setting has a typed default value
   - Settings are JSON-serialized for storage

2. **Settings Tauri commands**
   - `get_setting(key)`: return the value or default
   - `set_setting(key, value)`: validate and persist
   - `get_all_settings()`: return all settings as a JSON object
   - `reset_setting(key)`: delete override, return to default
   - `export_settings()`: export all settings to a JSON file (user picks location).
     Include a `schema_version` field in the exported JSON for forward compatibility.
   - `import_settings(path)`: import settings from a JSON file. Validate schema
     version, skip unsupported keys (with warning), apply valid settings in a single
     transaction (atomic rollback on failure).

3. **Wire settings into existing modules**
   - Hotkey module reads hotkey from settings (instead of hardcoded)
   - Output module reads output method from settings
   - Engine manager reads active model from settings
   - Audio pipeline reads denoise toggle from settings
   - All modules watch for settings changes and react at runtime

4. **GPU backend selection**
   - **Build-time flavors:** GPU backends are compiled in via feature flags at build time
     (CPU, Metal, CUDA, Vulkan). Each release artifact contains a fixed set of backends.
     The setting `gpu_backend` selects among backends available *in this build*, not all
     possible backends. A CPU-only build only shows "CPU".
   - Detect available backends at startup by querying compiled features
   - macOS Metal build: show CPU + Metal options
   - CUDA build: show CPU + CUDA options
   - Setting controls which compiled backend is used for inference
   - Note: changing GPU backend may require app restart (document in UI)
   - **No runtime feature flag toggling** -- the binary has the backends it was built with

5. **Verify:**
   - Change a setting, restart the app, verify it persists
   - Export settings to JSON file, import on a fresh install, verify they apply
   - GPU backend detection reports correctly on the current machine

### Files Created/Modified

```
src-tauri/src/db/settings.rs   # modified: add typed settings with defaults
src-tauri/src/settings.rs      # new: settings service layer (reads from db, caches)
```

---

## Phase 3: Dictation History

Store transcription results and audio recordings for later review.

### Work Items

1. **History recording**
   - After each transcription (in the dictation state machine):
     - Save the transcribed text to `dictation_history`
     - Save the audio buffer to a WAV file in data root / `audio/`
     - Store the audio path as relative to the data root (e.g., `audio/2026-02-25_1.wav`)
     - Record metadata: duration, engine used, language, WPM
     - If private mode is active (M9), skip history entirely
   - WAV writing: use a minimal WAV encoder (hound crate, or manual header + raw PCM)

2. **History retention policy**
   - Enforce retention on two triggers: (a) after each new history entry, and
     (b) immediately when the user changes retention settings (retroactive enforcement)
   - Retention modes:
     - Default: keep last 5 entries with audio
     - Count-based: user sets max count (1–unlimited)
     - Time-based ceiling (optional): delete entries older than N days
     - Disable: no new history stored. Existing entries remain until user clears them
       (changing to "disabled" does not silently purge existing history).
   - **Time semantics:** use UTC timestamps for all retention calculations. `created_at`
     is stored as ISO 8601 UTC. Cutoff = now_utc - N days (exclusive boundary).
   - **Atomicity:** delete the audio file first, then the DB row in the same
     transaction. If audio file deletion fails, log a warning but still remove the
     DB row (orphaned audio files are cleaned up on next startup sweep).
   - Retention settings stored in the settings table

3. **History Tauri commands**
   - `get_history(limit, offset)`: paginated history list
   - `get_history_entry(id)`: single entry with full metadata
   - `play_history_audio(id)`: play the saved audio via rodio
   - `copy_history_text(id)`: copy text to clipboard
   - `delete_history_entry(id)`: delete entry and associated audio
   - `clear_history()`: delete all entries

4. **Verify:**
   - Dictation stores text and audio in the database / filesystem
   - History persists across app restarts
   - Retention policy enforces the count limit
   - Deleting an entry also deletes the audio file
   - Audio playback works from history

### Files Created/Modified

```
src-tauri/src/dictation/mod.rs   # modified: save to history after transcription
src-tauri/src/audio/wav.rs       # WAV file encoder (or use hound crate)
```

---

## Phase 4: Settings UI

Settings page where users configure the app.

### Work Items

1. **Settings page component**
   - Create `src/lib/components/Settings.svelte`
   - Organized in sections matching the setting categories:

   **Dictation section:**
   - Hotkey configuration (display current, button to change)
   - Output method dropdown (Direct Input / Clipboard+Paste / Clipboard Only)

   **Engine section:**
   - Active model display (link to model management page)
   - Language dropdown
   - GPU backend selector (show only available backends, label each)
   - Denoise toggle

   **History section:**
   - Retention count input (numeric, with "unlimited" and "disabled" options)
   - Optional time ceiling (days)
   - Clear all history button (with confirmation)
   - Database location display (with "Change" button)

   **Advanced section:**
   - Log level dropdown
   - Export settings button
   - Import settings button

2. **Dictation history page**
   - Create `src/lib/components/History.svelte`
   - List view of past dictations: text preview, timestamp, duration, engine
   - Click to expand: full text, play audio button, copy text button, delete button
   - Pagination or infinite scroll for large histories

3. **Navigation update**
   - Extend navigation from M4: add Settings and History tabs/pages
   - Navigation: Dictation (main) | Models | History | Settings

4. **Hotkey configuration UI**
   - "Press a key combination" capture mode
   - Display the captured key combo
   - Save to settings, re-register the global hotkey
   - Validate: warn if the combo conflicts with common OS shortcuts

5. **Verify:**
   - All settings are editable through the UI
   - Changes take effect immediately (or on restart where noted)
   - History shows past dictations with playback
   - Settings export produces a valid JSON file
   - Settings import restores configuration
   - All strings externalized, keyboard navigable

### Files Created

```
src/lib/components/
  Settings.svelte
  History.svelte
  HistoryEntry.svelte
  HotkeyCapture.svelte
```

---

## Phase 5: User-Selectable Database Location

Allow the user to move the database file to a custom location.

### Work Items

1. **Data root relocation**
   - Settings UI: "Change Data Location" button
   - Opens a native directory picker dialog (requires `tauri-plugin-dialog`)
   - **Relocates the entire data root**, not just the DB: database file, audio
     recordings directory, and any other data files
   - Audio paths in `dictation_history` must be stored as **relative paths** (relative
     to the data root), not absolute paths, so they survive relocation
   - Copy the data root contents to the new location
   - **Bootstrap config:** store the custom data root path in a small JSON file
     (`data-root.json`) in the Tauri app config directory (NOT inside the SQLite DB).
     On startup, read this file first to find the DB location. This avoids the
     circular dependency of storing the DB path inside the DB being moved.
   - Reopen the database connection at the new path
   - If the target already has a data directory: warn and ask to overwrite or merge
   - On next startup: read `data-root.json` → open DB from that location
   - If the custom location is unavailable: fall back to default with a warning,
     update `data-root.json` to remove the stale path

2. **Verify:**
   - Move the data root to a custom directory, app continues to work
   - Audio playback from history still works after relocation (relative paths)
   - Restart the app, database loads from the custom location via `data-root.json`
   - If the custom location is unavailable: fall back to default with a warning

---

## Acceptance Criteria

M5 is complete when all of the following are true:

- [ ] SQLite database is created on first launch with schema migrations
- [ ] Schema has proper indexes (history `created_at`) and constraints
- [ ] Settings persist across app restarts (hotkey, output method, model, language, etc.)
- [ ] GPU backend selector shows only backends compiled into the current build artifact
- [ ] Active model tracked in a single source of truth (`settings.active_model_id`)
- [ ] Dictation history stores text and audio for each transcription
- [ ] Audio paths stored as relative paths (survive data root relocation)
- [ ] History retention enforces count limit retroactively on setting change
- [ ] History retention supports unlimited, disabled, and time-based ceiling (UTC)
- [ ] History UI: browse, replay audio, copy text, delete entries
- [ ] Settings UI covers all existing configuration options
- [ ] Hotkey capture UI allows changing the global hotkey
- [ ] Settings export/import works with JSON files (includes schema version)
- [ ] Import validates and applies atomically (rollback on failure)
- [ ] Data root location is user-selectable (DB + audio move together)
- [ ] Data root path stored in external `data-root.json` (not inside the DB)
- [ ] Navigation includes: Dictation, Models, History, Settings
- [ ] Settings UI is keyboard navigable and screen-reader accessible
- [ ] `./scripts/agent/check` passes
- [ ] CI builds pass on all platforms
