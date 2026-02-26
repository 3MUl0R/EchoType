# M10: Cloud Engines

Cloud transcription providers with bring-your-own API keys. Strictly opt-in, user
controls everything. Cloud providers implement the same engine trait as local Whisper.

**Depends on:** M9 (profiles, vocabulary, private mode, CLI)

**Delivers:** Users can add API keys, select a cloud provider, and transcribe via cloud
instead of local Whisper. Engine selection UI shows local and cloud options side by side.

---

## Phase 1: Cloud Engine Implementations

Implement cloud STT providers behind the existing engine trait.

### Work Items

1. **Cloud engine trait adapter**
   - The `SttEngine` trait (from M2) takes raw audio samples
   - Cloud providers need audio in a specific format (usually WAV, FLAC, or raw PCM)
   - Create a shared adapter: encode audio buffer to the required format before sending
   - Audio encoding: WAV (simplest, universally accepted) using the encoder from M5

2. **Provider contract table**

   Each provider has a distinct API contract. Implementations MUST match these
   exactly (do not assume providers are interchangeable):

   | Field | Groq | OpenAI | Deepgram |
   |-------|------|--------|----------|
   | Endpoint | `POST https://api.groq.com/openai/v1/audio/transcriptions` | `POST https://api.openai.com/v1/audio/transcriptions` | `POST https://api.deepgram.com/v1/listen` |
   | Auth header | `Authorization: Bearer {key}` | `Authorization: Bearer {key}` | `Authorization: Token {key}` |
   | Content-Type | `multipart/form-data` | `multipart/form-data` | `audio/wav` (raw body) |
   | Audio delivery | Multipart field `file` (WAV, with filename `audio.wav`) | Multipart field `file` (WAV, with filename `audio.wav`) | Raw audio bytes in request body |
   | Model param | Multipart field `model` = `whisper-large-v3` | Multipart field `model` = `whisper-1` (or configurable) | Query param `model=nova-2` (or configurable) |
   | Language param | Multipart field `language` (ISO 639-1) | Multipart field `language` (ISO 639-1) | Query param `language` (BCP-47) |
   | Response text path | `$.text` | `$.text` | `$.results.channels[0].alternatives[0].transcript` |
   | Streaming support | No | No (for audio transcription) | Yes (WebSocket, future) |

   **Important:** Deepgram uses `Token` auth (not `Bearer`). Do NOT reuse
   a shared auth header function across all providers without parameterizing
   the scheme.

3. **Groq cloud engine**
   - Create `src-tauri/src/engine/cloud/groq.rs`
   - Implement `SttEngine` for `GroqEngine`
   - Use `reqwest::multipart::Form` with field `file` (Part with filename
     `audio.wav` and mime `audio/wav`) and field `model`
   - Use `reqwest` with `rustls-tls` for the HTTP call
   - Parse response: extract `$.text` from JSON
   - Streaming: Groq does not support streaming transcription; wait for full response

4. **OpenAI cloud engine**
   - Create `src-tauri/src/engine/cloud/openai.rs`
   - Implement `SttEngine` for `OpenAiEngine`
   - Same multipart format as Groq (OpenAI-compatible API)
   - Model: configurable, default `whisper-1`. Allow users to specify a model
     name in settings to support newer models as OpenAI releases them.
   - Support language parameter

5. **Deepgram cloud engine**
   - Create `src-tauri/src/engine/cloud/deepgram.rs`
   - Implement `SttEngine` for `DeepgramEngine`
   - **Auth:** `Authorization: Token {key}` (NOT Bearer)
   - Request: raw audio bytes in request body with `Content-Type: audio/wav`
   - Parameters passed as query string: `?model=nova-2&language=en`
   - **Response parsing:** Deepgram returns a nested JSON structure. Extract
     transcript from `results.channels[0].alternatives[0].transcript`. Do NOT
     assume a flat `text` field like OpenAI/Groq.
   - Support language and model parameters

6. **Cloud engine common utilities**
   - Create `src-tauri/src/engine/cloud/mod.rs`
   - Shared HTTP error handling: timeout, auth failure (401), rate limit (429),
     server error (5xx)
   - **Error classification:**
     - Transient (retryable): timeout, network error, 502/503/504
     - Permanent (not retryable): 401 (bad key), 403 (forbidden), 400 (bad request)
     - Rate-limited: 429 — respect `Retry-After` header if present
   - Retry logic: retry once on transient errors with exponential backoff
     (1s, then 2s) + jitter. No retry on permanent errors or 429 without
     `Retry-After`.
   - Request timeout: configurable, default 30 seconds
   - Cloud request logging: log request duration, provider, status code,
     success/failure (never log the audio content or API key)

7. **Verify:**
   - Each cloud engine transcribes a test audio buffer (requires valid API key)
   - HTTP errors are handled gracefully (auth, timeout, rate limit)
   - Cloud engines return `Transcription` structs compatible with the trait

### Files Created

```
src-tauri/src/engine/cloud/
  mod.rs          # cloud engine utilities, error handling
  groq.rs         # Groq API implementation
  openai.rs       # OpenAI API implementation
  deepgram.rs     # Deepgram API implementation
```

---

## Phase 2: API Key Management

Secure storage and management of user-provided API keys.

### Work Items

1. **API key storage**
   - Use `keyring` crate to store API keys in the platform keychain:
     - macOS: Keychain Services
     - Windows: Credential Manager
     - Linux: Secret Service / GNOME Keyring
   - Key naming: `echotype:groq`, `echotype:openai`, `echotype:deepgram`
   - Never store API keys in the SQLite database or settings JSON
   - Never log API keys (mask in all log output)

2. **API key Tauri commands**
   - `set_api_key(provider, key)`: store in keychain. Key is received from the
     UI, stored, and immediately dropped from memory (not cached in Rust state).
   - `get_api_key_status(provider)`: return status only — `{ has_key: bool,
     masked_last4: Option<String>, last_validated: Option<DateTime> }`. The raw
     key is NEVER returned through IPC to the frontend. Cloud engine code
     reads the key from the keychain directly when making API calls.
   - `delete_api_key(provider)`: remove from keychain
   - `validate_api_key(provider)`: make a lightweight API call to verify the key
     works. Returns success/failure status. The key is read from keychain,
     used for the test call, then dropped — never sent to the frontend.

3. **API key management UI**
   - Create `src/lib/components/ApiKeyManager.svelte`
   - List of supported providers with status: configured / not configured
   - For each provider:
     - Input field for API key (password-masked)
     - "Save" button → store in keychain
     - "Test" button → validate the key with a test request
     - "Remove" button → delete from keychain
     - Status indicator: valid / invalid / not set
   - Add to Settings page under a "Cloud Providers" section

4. **Verify:**
   - API keys are stored in the platform keychain (not in files)
   - Keys are masked in the UI (show only last 4 characters)
   - Validation makes a real API call and reports success/failure
   - Removing a key actually deletes it from the keychain

### Files Created

```
src-tauri/src/security/
  mod.rs
  keyring.rs      # API key storage via keyring crate
src/lib/components/
  ApiKeyManager.svelte
```

---

## Phase 3: Engine Selection UI

Unified interface for choosing between local and cloud engines.

### Work Items

1. **Engine selection UI**
   - Create `src/lib/components/EngineSelector.svelte`
   - Two sections: "Local Models" and "Cloud Providers"
   - Local models: list installed models (from M4), active badge, switch button
   - Cloud providers: list available providers, configured status, switch button
   - Active engine clearly indicated
   - Switching engines takes effect immediately (next dictation uses the new engine)

2. **Cloud opt-in confirmation**
   - First time a user switches to a cloud engine:
     - Show a confirmation dialog explaining:
       - "Your audio will be sent to [Provider] for transcription"
       - "This requires an internet connection"
       - "Your API key will be used and usage may be billed"
       - "EchoType's Private Mode prevents local storage but cannot control
         what the cloud provider retains. Check your provider's data retention
         policy."
     - User must confirm before the switch happens
   - Subsequent switches between cloud providers: no re-confirmation needed
   - Switching back to local: no confirmation needed
   - The confirmation is stored (don't re-ask after first opt-in)

3. **Cloud usage indicators**
   - When a cloud engine is active:
     - Tray icon shows a small cloud badge or different icon
     - Settings shows "Cloud: [Provider]" prominently
     - Dictation overlay shows cloud indicator
   - Make it unmistakably clear when audio is leaving the machine

4. **Fallback to local**
   - **Transient failures** (network down, timeout, 5xx): auto-fallback to the
     local model if the setting is enabled. Show a brief notification:
     "Cloud unavailable, using local model for this dictation."
   - **Permanent failures** (401 auth, 403 forbidden, 429 rate limit): do NOT
     auto-fallback silently. Show an actionable error:
     - 401: "API key invalid or expired. Check your [Provider] key in Settings."
     - 403: "Access denied by [Provider]. Check your account status."
     - 429: "Rate limit exceeded. Try again later or switch to local."
   - **No local model available:** if fallback is needed but no local model is
     installed, show a clear error: "Cloud transcription failed and no local
     model is installed. Download a model in Settings → Models."
   - Setting: "Fall back to local on cloud failure" (default: on). This setting
     only applies to transient failures. Permanent errors always show an error.
   - Log all fallback events with the failure reason

5. **Integration with profiles**
   - Per-app profiles can specify an engine preference:
     - Use local model X
     - Use cloud provider Y
     - Use default (whatever is set globally)
   - Engine switching per profile works with the profile auto-switch logic from M9

6. **Verify:**
   - Engine selection shows local and cloud options side by side
   - Switching to a cloud engine shows opt-in confirmation on first use
   - Cloud indicator visible in tray and overlay when cloud is active
   - Cloud failure falls back to local when setting is enabled
   - Engine preference works in per-app profiles

### Files Created

```
src/lib/components/
  EngineSelector.svelte
  CloudOptInDialog.svelte
```

---

## Acceptance Criteria

M10 is complete when all of the following are true:

- [ ] Groq, OpenAI, and Deepgram engines implement the SttEngine trait
- [ ] Each provider uses correct auth header (Bearer for Groq/OpenAI, Token for Deepgram)
- [ ] Groq/OpenAI use multipart with correct field names (`file`, `model`, `language`)
- [ ] Deepgram uses raw body with correct Content-Type and query params
- [ ] Deepgram response parsed from nested JSON path (not flat `text` field)
- [ ] Transient cloud errors (timeout, network, 5xx) trigger auto-fallback to local
- [ ] Permanent cloud errors (401, 403, 429) show actionable error messages (no silent fallback)
- [ ] Fallback when no local model installed shows clear guidance to download one
- [ ] Retry uses exponential backoff + jitter; 429 respects `Retry-After`
- [ ] API keys stored in platform keychain (not in files, database, or logs)
- [ ] Raw API keys never returned through IPC to the frontend (status-only commands)
- [ ] API key UI: add, test, remove keys with masked display (last 4 chars only)
- [ ] Engine selection UI shows local and cloud options side by side
- [ ] First cloud engine switch shows opt-in confirmation with privacy disclaimer
- [ ] Cloud usage clearly indicated (tray badge, settings, overlay)
- [ ] Per-app profiles can specify engine preference
- [ ] OpenAI model name is configurable (not hardcoded to `whisper-1`)
- [ ] Cloud request duration and provider logged as structured events
- [ ] All new UI strings externalized in i18n resource files
- [ ] All new UI components are keyboard navigable and screen-reader accessible
- [ ] `./scripts/agent/check` passes
- [ ] CI builds pass on all platforms
