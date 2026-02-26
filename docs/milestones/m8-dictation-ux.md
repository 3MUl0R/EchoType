# M8: Dictation UX

Streaming transcription, edit-before-insert buffer, selection-aware replacement, and
auto-submit. The dictation experience becomes refined and complete.

**Depends on:** M7 (all three dictation modes, noise suppression, raw/formatted modes)

**Delivers:** Streaming preview shows provisional text in real time. Edit buffer lets
you review before committing. Selected text is replaced by new dictation. Auto-submit
sends after insertion.

---

## Phase 1: Streaming Transcription with Replacement

Show partial transcription results in real time, then replace with the final version.

### Work Items

1. **Streaming transcription backend**
   - Modify the dictation state machine to support a `StreamingPartial` sub-state
   - **Streaming through the engine trait (engine-agnostic):**
     - Extend `SttEngine` with an optional streaming contract:
       ```rust
       async fn begin_streaming(&self, config: StreamConfig) -> Result<StreamSession>;
       // StreamSession emits: PartialResult | FinalResult | Error
       ```
     - Engines that support streaming implement it natively (cloud engines in
       M10 use server-side streaming). Engines that don't support streaming
       (like local Whisper) use a polling adapter.
     - **Whisper polling adapter:** periodically transcribe the accumulated
       audio buffer (every 1–2s). Each poll is a full re-decode of the buffer.
   - **Performance guardrails for polling-based streaming:**
     - **CPU budget:** if a poll transcription takes longer than the poll
       interval, skip the next poll (no queuing). Log a warning.
     - **Cancellation:** if dictation ends while a poll is in-flight, cancel
       the in-flight transcription (or ignore its result) and run the final
       transcription on the complete buffer.
     - **Backpressure:** emit at most one partial result per poll interval.
       Frontend discards stale partials if a newer one arrives.
     - **Max buffer re-decode:** for recordings longer than 30s, only
       re-transcribe the last 30s for partials (final transcription uses the
       full buffer). This caps CPU cost.
   - On dictation end: run final transcription on the complete buffer
   - Partial results are best-effort -- they may differ from the final result

2. **Streaming in VAD mode**
   - VAD mode buffers speech segments automatically
   - For streaming: emit partial results during the speech segment
   - On silence (segment end): emit final transcription for that segment

3. **Partial text display (overlay only -- no in-app partial insertion)**
   - **Architecture decision:** partial results are displayed ONLY in the
     EchoType overlay / edit buffer window. Text is NEVER inserted into the
     target application until the final transcription is ready.
   - Rationale: in-place partial insertion and replacement in arbitrary apps is
     fragile (undo stack corruption, selection loss, incompatible input fields).
     Overlay-only preview is reliable across all target apps.
   - During streaming: overlay shows live partial text with visual distinction
   - On dictation end: final transcription is inserted into the target app
     using the configured output method (direct input / clipboard)
   - If edit buffer is enabled: final text appears in the buffer for review
   - If edit buffer is disabled: final text is inserted immediately

4. **Partial text visual distinction**
   - Partial text displayed in the overlay or UI must be visually distinct:
     - Lighter color or reduced opacity
     - Italic or different font weight
     - Clear label: "Transcribing..."
   - User knows this text is provisional and may change

5. **Graceful failure**
   - If the final transcription fails to arrive (engine crash, timeout):
     - Keep the last partial result as the final text
     - Log the failure
     - Do not silently delete text the user has already seen
   - If streaming is disabled in settings: skip partial results, wait for final only

6. **Streaming toggle**
   - Setting: "Show streaming preview" (default: on)
   - When off: no partial results, wait for final transcription before any output
   - Some users prefer the simplicity of no streaming

7. **Verify:**
   - During dictation, partial transcription appears in real time
   - Partial text is visually distinct from final text
   - Final transcription replaces partial content
   - Engine failure preserves the last partial text
   - Streaming can be disabled in settings

---

## Phase 2: Edit-Before-Insert Buffer

Optional floating window where users can review and edit transcription before committing.

### Work Items

1. **Edit buffer window**
   - Create a small, floating, always-on-top window
   - Appears after transcription completes (or during streaming, showing live text)
   - Contains:
     - Editable text area with the transcription result
     - "Insert" button (keyboard shortcut: Cmd/Ctrl+Enter)
     - "Discard" button (keyboard shortcut: Escape)
     - "Copy" button (copy to clipboard without inserting)
   - **Window placement:** position near the last known cursor location if
     available (via platform accessibility APIs or cached position). Fall back
     to a configurable fixed screen position (e.g., bottom-center).
   - **Focus policy (two phases):**
     - **Phase A (preview, non-activating):** while recording/streaming, the
       buffer shows live text but does NOT take focus. The target app retains
       focus. Platform implementation:
       - macOS: `NSPanel` with `NSWindowStyleMask.nonactivatingPanel`
       - Windows: `WS_EX_NOACTIVATE` extended window style
       - Linux X11: `_NET_WM_STATE_ABOVE` + no focus request
       - Linux Wayland: layer-shell overlay (if available) or regular window
         with `set_accept_focus(false)` (best-effort, compositor-dependent)
     - **Phase B (editing, activating):** when the user explicitly clicks the
       buffer or presses a "focus buffer" hotkey (e.g., Cmd/Ctrl+Shift+E),
       the buffer takes focus and becomes editable. The target app loses focus.
     - **On Insert:** return focus to the target app (re-activate focus lock
       target), then insert text. Focus return must happen BEFORE insertion.

2. **Edit buffer flow**
   - Dictation completes → edit buffer appears with transcription text
   - User can:
     - Edit the text (fix errors, rephrase)
     - Press Insert: text is inserted at the original cursor position (focus lock)
     - Press Discard: nothing is inserted, buffer closes
     - Press Copy: text copied to clipboard, buffer closes
   - If streaming is on: buffer shows partial text live, user can start editing
     even before final transcription arrives

3. **Edit buffer setting**
   - Setting: "Edit before insert" (default: off)
   - When off: transcription is inserted immediately (existing behavior)
   - When on: edit buffer appears after every transcription
   - Keyboard-driven: the buffer must be fully usable without a mouse

4. **Integration with output methods**
   - After the user confirms in the edit buffer:
     - Re-activate the focus lock target
     - Insert using the configured output method (direct input / clipboard+paste)
   - If the target app has moved or closed: warn the user, offer to copy instead

5. **Verify:**
   - Edit buffer appears after transcription with editable text
   - User can edit text and insert with keyboard shortcut
   - Discard closes the buffer without inserting
   - Edit buffer does not steal focus from the target app
   - Streaming text appears live in the buffer during dictation
   - Buffer is fully keyboard-navigable

### Files Created

```
src/lib/components/
  EditBuffer.svelte          # floating edit window
src-tauri/src/dictation/
  edit_buffer.rs             # edit buffer state, window management
```

---

## Phase 3: Selection-Aware Replacement

If the user has text selected when dictation starts, replace the selection with the
transcription.

### Work Items

1. **Selection detection (best-effort)**
   - On dictation start (hotkey press or VAD trigger):
     - Attempt to detect if the target application has text selected
     - Method: save clipboard, simulate Cmd/Ctrl+C, check if clipboard changed
     - **Three-state result:**
       - `Selected(text)`: clipboard changed to new text content → selection detected
       - `NoSelection`: clipboard unchanged → no selection (normal insert)
       - `Unknown`: clipboard changed but to non-text content, or detection
         timed out, or secure input field blocked the copy → treat as no
         selection (safe fallback)
     - Restore original clipboard after detection
   - **Reliability limitations (documented, not hidden):**
     - If the user's clipboard already contains the same text as the selection,
       detection reports `NoSelection` (false negative). This is acceptable --
       the result is normal insert, which is always safe.
     - Some apps (password fields, DRM-protected content) block Cmd/Ctrl+C.
       Detection returns `Unknown` → normal insert.
     - Detection adds ~50–100ms latency to dictation start. This is measured
       and logged as a structured event.
   - **Wayland:** clipboard access may be restricted by the compositor.
     Selection detection is disabled on Wayland (always returns `Unknown`).

2. **Selection replacement**
   - If a selection was detected:
     - The selected text is already "selected" in the target app
     - After transcription: simply type or paste the new text
     - The OS replaces the selection with the new content automatically
     - No need to explicitly delete the selection first
   - If no selection (or `Unknown`): normal insert behavior (existing flow)
   - **Selection staleness risk:** if the edit buffer is enabled, the user may
     spend time editing, during which the target app's selection could be lost
     (user clicked elsewhere, app closed, etc.). Mitigation:
     - Capture a snapshot of the selection state at dictation start (immutable)
     - On insertion: attempt the replacement optimistically. If the target app
       no longer has a selection (e.g., focus was lost and regained), the text
       is inserted at the current cursor position instead.
     - Log a `selection_stale` event when this fallback triggers.
     - Do NOT re-detect selection at insertion time (that would be confusing
       if the user selected different text in the meantime).

3. **Edge cases**
   - Selection in a non-text field: ignore, treat as no selection
   - Very large selection: warn in edit buffer if enabled
   - Target app doesn't support selection replacement: fall back to normal insert

4. **Verify:**
   - Select text in a text editor, dictate, selection is replaced with transcription
   - No selection: text is inserted at cursor (unchanged behavior)
   - Clipboard is not clobbered by selection detection

---

## Phase 4: Auto-Submit After Insertion

Optionally send a keypress after text is inserted.

### Work Items

1. **Auto-submit implementation**
   - After text insertion completes, optionally simulate a keypress:
     - Enter
     - Ctrl+Enter
     - Cmd+Enter (macOS)
   - Small delay between insertion and submit (configurable, default: 100ms)
     to allow the target app to process the text

2. **Auto-submit settings**
   - Setting: "Auto-submit after insertion" (default: off)
   - Setting: "Submit key" (Enter / Ctrl+Enter / Cmd+Enter)
   - Setting: "Submit delay" (50ms - 500ms)
   - Use cases: chat apps, search bars, terminal input

3. **Integration with edit buffer**
   - If edit buffer is enabled: auto-submit fires after the user confirms (not
     after transcription completes)
   - If edit buffer is disabled: auto-submit fires after text insertion

4. **Verify:**
   - Enable auto-submit with Enter, dictate in a chat app, message sends automatically
   - Ctrl+Enter auto-submit works in apps that use it (e.g., Slack)
   - Auto-submit does not fire when edit buffer is discarded
   - Delay is respected between insertion and submit

---

## Acceptance Criteria

M8 is complete when all of the following are true:

- [ ] Streaming transcription shows partial results in real time during dictation
- [ ] Partial text is visually distinct (lighter, italic, or labeled)
- [ ] Final transcription replaces partial content in the overlay (not in-app)
- [ ] Engine failure preserves the last partial text (no silent deletion)
- [ ] Streaming can be toggled off in settings
- [ ] Edit buffer appears after transcription with editable text
- [ ] Edit buffer supports Insert, Discard, and Copy actions via keyboard
- [ ] Edit buffer does not steal focus from the target application
- [ ] Streaming text appears live in the edit buffer
- [ ] Selection-aware replacement works: selected text replaced by transcription
- [ ] No selection: normal insert behavior is unchanged
- [ ] Clipboard is preserved during selection detection
- [ ] Selection detection returns `Unknown` gracefully (treated as no selection)
- [ ] Selection staleness handled: stale selection falls back to insert at cursor
- [ ] Auto-submit sends configurable keypress after insertion
- [ ] Auto-submit integrates correctly with edit buffer flow
- [ ] Streaming goes through SttEngine trait (not Whisper-specific)
- [ ] Streaming CPU budget enforced: poll skipped if previous poll still running
- [ ] Partial transcription capped at last 30s for long recordings
- [ ] Edit buffer focus policy works per-platform (non-activating preview)
- [ ] All new UI strings externalized in i18n resource files
- [ ] All new UI components are keyboard navigable and screen-reader accessible
- [ ] Selection detection latency and streaming latency logged as structured events
- [ ] `./scripts/agent/check` passes
- [ ] CI builds pass on all platforms
