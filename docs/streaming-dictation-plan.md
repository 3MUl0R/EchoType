# EchoType - Streaming Dictation Plan

This document turns the current streaming discussion into an implementation plan. It
refines the streaming portions of `docs/milestones/m8-dictation-ux.md` and makes one
explicit product decision:

- EchoType will not do provisional text insertion into third-party apps.
- Streaming will be implemented as a separate dictation route that can be enabled or
  disabled in configuration.
- Live preview, when enabled, will appear only in EchoType-owned surfaces such as the
  overlay.

The goal is to improve perceived latency without introducing fragile replacement logic in
arbitrary target applications.

---

## 1. Product Decisions

### Confirmed

- Keep the existing dictation flow as the `classic` route.
- Add a new `streaming` route instead of rewriting the current path in place.
- Unify all dictation entry points so hotkey dictation and the in-app record button use
  the same session controller.
- Final insertion into the target app remains one-shot and final-only.
- Live preview, when enabled, appears in the overlay first.
- Preview text is informational only. It does not alter the target app.
- When `streaming` mode is selected, the UI should only expose providers and models that
  are actually compatible with that route.

### Explicit Non-Goals

- No in-app fuzzy insertion followed by replacement.
- No attempt to preserve undo stacks across arbitrary editors by mutating target text
  repeatedly.
- No requirement that every engine support true network streaming on day one.

---

## 2. What We Are Actually Trying to Achieve

There are two related but distinct improvements:

1. Reduce release-to-insert latency by doing more work while the user is still speaking.
2. Optionally show a live preview in the EchoType overlay so the user can see partial
   transcription progress without touching the target app.

These should be independently useful:

- `streaming` route with preview off:
  - Lower perceived latency.
  - No extra UI noise.
- `streaming` route with preview on:
  - Lower perceived latency.
  - Overlay shows provisional text during dictation.

This means "streaming" is not only a UI feature. It is primarily a new execution path for
the dictation pipeline.

---

## 3. Current State of the Codebase

The repo already has partial groundwork:

- A backend polling task exists in `src-tauri/src/dictation/streaming.rs`.
- The dictation manager can start that task when `streaming_enabled` is true.
- Partial results are emitted as `dictation:partial`.
- The overlay and main app do not currently consume those partial events.
- Final insertion is still a single blocking insert after the full transcription
  completes.
- The in-app record button uses its own capture/transcribe flow instead of the dictation
  manager.

In practice, the current system is "classic dictation plus an unused partial event path."

---

## 4. High-Level Architecture

### 4.1 Route Model

Introduce an explicit dictation route selection:

- `classic`
  - Current behavior.
  - Record full audio.
  - Run pipeline after stop.
  - Transcribe after stop.
  - Insert final text.
- `streaming`
  - Record audio.
  - Process audio incrementally during capture.
  - Optionally emit partial text during capture.
  - Finalize on release.
  - Insert final text once.

Recommended setting shape:

```json
{
  "dictation_route": "classic",
  "streaming_preview_enabled": false,
  "streaming_preview_surface": "overlay"
}
```

Recommended rollout:

- Ship with `classic` as the default route at first.
- Allow opting into `streaming` globally and per profile.
- Only revisit the default after latency and stability data are strong.

### 4.2 Unified Session Controller

Create a single session controller that owns:

- session lifecycle
- focus lock
- audio capture
- route selection
- state events
- partial events
- final insertion

Both of these should use that same controller:

- global hotkey dictation
- in-app record button

That removes the current split-brain behavior and keeps future features from landing in
only one path.

### 4.3 Keep Route Logic Separate

The new route should be a separate backend path, not a pile of conditionals inside the
classic flow. A likely shape is:

```text
src-tauri/src/dictation/
  mod.rs
  session.rs
  routes/
    mod.rs
    classic.rs
    streaming.rs
```

The current behavior becomes `classic.rs`. The new path becomes `streaming.rs`.

This separation preserves backward capability and makes rollback trivial.

---

## 5. How the Streaming Route Should Work

### 5.1 Session Lifecycle

The `streaming` route should behave like this:

1. User starts dictation.
2. EchoType creates a streaming session with a unique `session_id`.
3. Audio frames are captured continuously.
4. Audio is normalized incrementally while capture is in progress.
5. The route performs background transcription work during recording.
6. If preview is enabled, EchoType emits partial text updates to its own UI.
7. User releases the key.
8. The route finalizes the session and produces one final transcription.
9. EchoType restores focus and inserts the final text once.

### 5.2 Why This Should Be Faster

The latency win comes from overlapping work with capture instead of waiting until the end
to start everything:

- denoise during recording instead of only after stop
- resample during recording instead of only after stop
- optionally accumulate WAV or engine-ready chunks during recording
- optionally run partial inference during recording
- optionally stream audio to engines that support true live sessions

Even if preview is off, the route is still valuable because the system is "closer to
done" when the user releases the key.

### 5.3 Engine Reality

Not all engines can truly stream today.

- Local Whisper:
  - No true network streaming.
  - Can still benefit from incremental preprocessing plus periodic partial inference.
- OpenAI and Groq audio transcription in the current implementation:
  - Batch request/response only.
  - No true live session yet.
- Deepgram:
  - Best candidate for a future true streaming transport.
  - Current implementation is still batch.

This leads to a practical design rule:

- The route must support both:
  - batch engines with a streaming adapter
  - engines with future native streaming sessions

### 5.4 Provider and Model Filtering

Streaming mode should be capability-driven, not provider-driven.

Recommended UI rule:

- when `dictation_route = classic`:
  - show the full set of supported local and cloud engines
- when `dictation_route = streaming`:
  - show only engines/models that are known to work with the streaming route

Initial allowlist recommendation:

- OpenAI models that support realtime transcription for the route
- Deepgram models that support realtime streaming for the route

Do not show batch-only models in the streaming picker unless they are being surfaced
through a route-compatible adapter and have passed validation.

This keeps the settings UI honest and avoids offering combinations that cannot actually
deliver streaming behavior.

### 5.5 WhisperLiveKit as an Experimental Adapter

WhisperLiveKit is a strong candidate for an experimental streaming adapter layer.

Why it is interesting:

- it is designed as a real-time transcription server
- it exposes a WebSocket-based streaming flow
- it explicitly aims to make Whisper-family models usable in low-latency settings
- it supports multiple backend implementations instead of a single local stack

What its README currently documents:

- local/backend options include `mlx-whisper`, `faster-whisper`, `whisper`, and
  `openai-api`
- it also supports `voxtral-mlx` and `voxtral`
- the `openai-api` backend is documented as `LocalAgreement only`

Recommended product posture:

- treat WhisperLiveKit as an experimental sidecar path, not the primary streaming
  architecture
- use it to evaluate whether Whisper-family models can provide a good enough streaming
  route for EchoType
- do not assume Groq compatibility until a proof-of-concept confirms it

Important limitation:

- the README does not document a native Groq backend
- it also does not document Groq explicitly anywhere in the main README
- because of that, Groq via WhisperLiveKit is currently only a hypothesis

Practical implication:

- WhisperLiveKit may still be valuable for:
  - local streaming with Whisper-family backends
  - possible OpenAI-compatible backend experiments
- but Groq support should be treated as a spike with exit criteria, not as a committed
  implementation path

Recommended implementation approach:

- Phase 1: keep the current `SttEngine::transcribe()` contract intact and build the
  `streaming` route above it.
- Phase 2: add an optional native streaming interface only when there is a real engine
  ready to use it.

That keeps the first version smaller and avoids prematurely complicating the engine trait.

---

## 6. Preview Model

### 6.1 Overlay First

The overlay is the right first surface for live preview because:

- it is already part of the dictation lifecycle
- it does not affect the target app
- it can be shown or hidden independently of final insertion
- it avoids edit-buffer focus complexity in the first pass

Recommended behavior:

- preview off:
  - current overlay behavior remains
  - listening and transcribing states only
- preview on:
  - while recording, overlay expands to show trailing partial text
  - on release, overlay switches to a finalizing state
  - after final insertion, overlay closes as normal

### 6.2 Visual Design Constraints

The preview should stay compact and non-disruptive:

- show only the most recent 2-4 lines
- auto-scroll to the newest text
- clip older content rather than growing indefinitely
- keep the window anchored near the current overlay position
- remain click-through and non-activating

Recommended first visual treatment:

- keep the current pill when there is no text yet
- expand into a narrow preview card once partial text arrives
- style preview text with slightly reduced opacity
- add a small label such as `Live Preview`
- avoid italic if readability suffers

The preview does not need to look "fuzzy" on the first version. Distinct and readable is
more important than theatrical styling.

### 6.3 Failure Behavior

If final transcription fails:

- do not silently clear the preview text
- preserve the last partial in the overlay long enough for the user to understand what
  happened
- emit a clear error state in logs and UI
- do not insert any provisional text into the target app

---

## 7. Recommended Events and Session State

The existing event model is too small for a proper streaming route. Add session-aware
events.

### 7.1 Dictation State

Keep `dictation:state`, but make sure it is session-aware internally. Likely visible states:

- `idle`
- `recording`
- `finalizing`
- `editing`
- `inserting`
- `error`

`transcribing` can remain as a UI label for classic mode, but `finalizing` is a better
backend concept for the streaming route because transcription work has already been
running.

### 7.2 Partial Event

Replace or extend the current partial payload to include:

```json
{
  "session_id": "uuid-or-counter",
  "seq": 12,
  "text": "partial text",
  "is_final": false,
  "stability": "low",
  "route": "streaming"
}
```

Minimum required fields:

- `session_id`
- `seq`
- `text`
- `is_final`

Why this matters:

- prevents stale partials from an old session being rendered after stop
- lets the frontend discard out-of-order updates
- gives room for future stability styling

### 7.3 Frontend Rules

Frontend handling should follow these rules:

- ignore partials whose `session_id` does not match the active session
- ignore partials with a `seq` older than the latest rendered value
- clear preview state when the session ends
- never treat a partial as something to insert into the target app

---

## 8. Audio Pipeline Changes

The route should not wait until release to do all audio processing.

### Required Work

- split capture into append-only frame flow
- process frames into a normalized rolling buffer during capture
- keep the final full buffer for history/finalization
- keep a bounded preview buffer for partial transcription work

Recommended buffers:

- full session buffer:
  - complete audio for final transcription and history
- processed rolling buffer:
  - normalized audio for partial work
- preview window:
  - capped recent audio for partial re-decode, such as the current 30 second limit

### Guardrails

- no queued partial jobs
- if a partial job overruns the poll interval, skip the next cycle
- after stop, ignore any late partial result from the just-ended session
- cap preview re-decode duration
- log poll duration, skip counts, and finalization time

The existing `streaming.rs` already has some of these guardrails, but the new route should
make them session-aware and deterministic.

---

## 9. Configuration Plan

To preserve backward compatibility, configuration should distinguish route selection from
preview selection.

### New Settings

- `dictation_route`
  - `classic`
  - `streaming`
- `streaming_preview_enabled`
  - `true | false`
- `streaming_preview_surface`
  - `overlay`
  - future: `edit_buffer`
- `streaming_provider_filter`
  - derived from capability metadata rather than user-entered freeform values

### Recommended Defaults

- `dictation_route = classic`
- `streaming_preview_enabled = false`
- `streaming_preview_surface = overlay`

### Scope

These settings should be supported:

- globally
- per application profile

This allows a conservative rollout:

- keep `classic` for fragile workflows
- enable `streaming` only where the user wants faster completion

---

## 10. Phase Plan

### Phase 1: Foundation and Unification

Objective:
Create one dictation session controller and preserve the current behavior as the
`classic` route.

Work:

- extract current dictation path into an explicit classic route
- create a route selector in settings
- make the global hotkey use the unified controller
- make the in-app record button use the unified controller
- ensure insertion, history, focus restore, and edit buffer still behave exactly like
  today on the classic route

Acceptance:

- classic route behavior is unchanged
- record button and hotkey use the same backend session flow
- route can be switched in configuration without code changes

### Phase 2: Streaming Route Without Preview

Objective:
Get the low-latency route working before adding new UI.

Work:

- create a streaming session object with `session_id`
- process audio incrementally during capture
- reuse the existing partial polling concept as a background worker
- make finalization session-aware
- ignore late partials after stop
- keep the target-app insertion behavior final-only
- add route-aware structured logging and latency metrics

Acceptance:

- streaming route can be enabled with preview disabled
- final insertion still happens only once
- release-to-insert latency is measurably better than classic on supported setups
- stale partials do not leak into later sessions

### Phase 3: Overlay Live Preview

Objective:
Expose the streaming work to the user through the overlay.

Work:

- extend overlay UI to consume partial events
- keep current small status overlay when preview is disabled
- expand overlay into a compact preview card when preview is enabled
- render the trailing portion of the provisional text with auto-scroll
- show a finalizing state after key release
- clear preview when the session ends

Acceptance:

- preview is visible only in EchoType UI
- overlay remains click-through and does not steal focus
- preview can be toggled off independently of route choice
- final insertion behavior is unchanged

### Phase 4: Route Fallback and Safety

Objective:
Make the route safe to ship as an opt-in feature.

Work:

- if the streaming route fails during a session, fall back safely without corrupting the
  target app
- decide whether fallback means:
  - switch to classic for the current session, or
  - abort cleanly with user-visible error
- add route-specific diagnostics
- make settings UI explain the tradeoff clearly

Acceptance:

- failure in streaming mode does not produce repeated insertion or broken state
- users can always switch back to classic
- logs make it obvious which route handled the session

### Phase 5: Native Engine Streaming

Objective:
Add true live audio-to-engine transport where it is actually supported.

Work:

- introduce an optional engine streaming session interface
- implement it only for engines that benefit from it
- first candidate: Deepgram WebSocket streaming
- keep batch adapter behavior for engines that remain request/response only

Acceptance:

- native streaming engines use live transport
- batch engines still work through the streaming route adapter
- the route abstraction does not change user-facing behavior

### Phase 6: Experimental Adapter Track

Objective:
Evaluate whether WhisperLiveKit is a viable sidecar for Whisper-family streaming and
whether it can be made to work with Groq-compatible flows.

Work:

- create a spike branch or prototype integration against WhisperLiveKit
- test it as an external sidecar process rather than embedding it into the main app first
- measure:
  - startup cost
  - end-to-end latency
  - transcript quality drift versus final batch transcription
  - operational complexity on Windows, macOS, and Linux
- verify whether its `openai-api` backend can be used against Groq in practice
- if Groq works, document the exact constraints and supported model set
- if Groq does not work, keep WhisperLiveKit scoped to local/self-hosted backends only

Acceptance:

- EchoType has a clear yes/no decision on WhisperLiveKit adoption
- Groq compatibility is proven or explicitly rejected based on testing
- packaging and deployment costs are documented before any product commitment

### Phase 7: Edit Buffer Integration

Objective:
Decide how much live streaming should appear in the edit buffer.

Recommendation:

- do not block the overlay plan on this
- keep edit buffer behavior final-only in the first shipping version
- revisit live edit-buffer preview after overlay streaming is stable

Possible future work:

- seed the edit buffer with the latest partial before final text arrives
- allow the user to promote the preview surface from overlay to edit buffer

---

## 11. Metrics and Validation

This feature should be judged on measurable latency improvement, not just "it feels
cooler."

Track at minimum:

- route used: `classic` or `streaming`
- dictation duration
- partial poll count
- partial poll overruns
- finalization time after stop
- total release-to-insert latency
- preview enabled or disabled
- engine name

Success criteria for the streaming route:

- lower median release-to-insert latency than classic
- no regression in insertion correctness
- no increase in focus restore failures
- no stale partial rendering across sessions

---

## 12. Recommended File Areas

Likely backend touch points:

- `src-tauri/src/dictation/mod.rs`
- `src-tauri/src/dictation/streaming.rs`
- `src-tauri/src/commands.rs`
- `src-tauri/src/lib.rs`
- `src-tauri/src/audio/*`
- `src-tauri/src/engine/*`
- `src-tauri/src/settings.rs`

Likely frontend touch points:

- `src/App.svelte`
- `src/lib/components/Overlay.svelte`
- `src/lib/components/RecordButton.svelte`
- `src/lib/components/Settings.svelte`
- `src/lib/components/TranscriptionDisplay.svelte`
- `src/lib/i18n/en.json`

Likely new backend files:

- `src-tauri/src/dictation/session.rs`
- `src-tauri/src/dictation/routes/mod.rs`
- `src-tauri/src/dictation/routes/classic.rs`
- `src-tauri/src/dictation/routes/streaming.rs`

---

## 13. Open Questions

These are the remaining product questions, with recommended defaults.

### Should preview text look "fuzzy"?

Recommended default:

- no special blur or unstable animation
- just make it clearly provisional through label and opacity

Reason:

- easier to read
- easier to implement
- less likely to feel gimmicky

### How much text should the overlay show?

Recommended default:

- last 2-4 lines only

Reason:

- enough to confirm correctness
- does not dominate the screen

### Should preview be on by default once the streaming route exists?

Recommended default:

- no

Reason:

- route performance and preview noise should be evaluated separately

### Should the streaming route become the default immediately?

Recommended default:

- no

Reason:

- keep rollback simple
- gather metrics first

---

## 14. Summary

The recommended path is:

1. keep the current dictation flow as `classic`
2. add a new `streaming` route
3. unify hotkey and record-button dictation behind one controller
4. use the streaming route first to reduce post-release latency
5. add overlay live preview as a separate optional layer
6. never mutate third-party app text provisionally

This gives EchoType the main user benefit of streaming while avoiding the most fragile
part of the problem.
