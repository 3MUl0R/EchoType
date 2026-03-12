<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";

  type DictationState =
    | "idle"
    | "recording"
    | "transcribing"
    | "editing"
    | "inserting"
    | "finalizing";

  interface DictationEvent {
    state: DictationState;
    text: string | null;
    error: string | null;
    latency_ms: number | null;
  }

  interface PartialResult {
    session_id: string;
    seq: number;
    text: string;
    is_final: boolean;
    stability: string;
    route: string;
  }

  let dictationState: DictationState = $state("idle");
  let visible = $state(false);

  // Streaming partial result state
  let partialText: string = $state("");
  let activeSessionId: string | null = $state(null);
  let lastSeq: number = $state(-1);
  let previewVisible: boolean = $state(false);

  // Show only the trailing ~200 characters
  let displayText = $derived(
    partialText.length > 200
      ? "\u2026" + partialText.slice(-200)
      : partialText,
  );

  // Fade out after inserting/idle
  let fadeTimeout: ReturnType<typeof setTimeout> | null = null;

  function showOverlay() {
    if (fadeTimeout) {
      clearTimeout(fadeTimeout);
      fadeTimeout = null;
    }
    visible = true;
  }

  function hideOverlay() {
    // Brief delay so the user sees the final state
    fadeTimeout = setTimeout(() => {
      visible = false;
    }, 600);
  }

  function clearPartialState() {
    partialText = "";
    activeSessionId = null;
    lastSeq = -1;
    previewVisible = false;
  }

  // Listen for dictation state changes
  $effect(() => {
    const unlisten = listen<DictationEvent>("dictation:state", (event) => {
      const state = event.payload.state;
      dictationState = state;

      if (
        state === "recording" ||
        state === "transcribing" ||
        state === "finalizing"
      ) {
        showOverlay();
      } else if (state === "idle") {
        clearPartialState();
        hideOverlay();
      }
      // "editing" and "inserting" — keep visible if already shown
    });

    return () => {
      unlisten.then((fn) => fn());
      if (fadeTimeout) clearTimeout(fadeTimeout);
    };
  });

  // Listen for streaming partial results
  $effect(() => {
    const unlisten = listen<PartialResult>("dictation:partial", (event) => {
      const partial = event.payload;

      // Set session from first partial received
      if (activeSessionId === null) {
        activeSessionId = partial.session_id;
        lastSeq = -1;
      }

      // Ignore partials from a different session
      if (partial.session_id !== activeSessionId) return;

      // Ignore out-of-order partials
      if (partial.seq < lastSeq) return;

      lastSeq = partial.seq;
      partialText = partial.text;
      previewVisible = true;
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  });

  // Make the window click-through (non-interactive)
  $effect(() => {
    const win = getCurrentWindow();
    win.setIgnoreCursorEvents(true).catch(() => {});
  });
</script>

<div
  class="overlay-container"
  class:visible
  class:recording={dictationState === "recording"}
  class:processing={dictationState === "transcribing" ||
    dictationState === "inserting" ||
    dictationState === "finalizing"}
>
  <div class="overlay-pill">
    {#if dictationState === "recording"}
      <div class="recording-dot"></div>
      <span class="label">Listening…</span>
    {:else if dictationState === "finalizing"}
      <div class="spinner"></div>
      <span class="label">Finalizing…</span>
    {:else if dictationState === "transcribing" || dictationState === "inserting"}
      <div class="spinner"></div>
      <span class="label">Processing…</span>
    {:else}
      <span class="label">Ready</span>
    {/if}
  </div>

  {#if previewVisible && partialText}
    <div class="preview-card">
      <div class="preview-header">Live Preview</div>
      <div class="preview-text">{displayText}</div>
    </div>
  {/if}
</div>

<style>
  .overlay-container {
    position: fixed;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    pointer-events: none;
    opacity: 0;
    transform: translateY(8px);
    transition:
      opacity 0.25s ease,
      transform 0.25s ease;
  }

  .overlay-container.visible {
    opacity: 1;
    transform: translateY(0);
  }

  .overlay-pill {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 18px;
    border-radius: 20px;
    /* Glassmorphism */
    background: rgba(30, 30, 30, 0.65);
    backdrop-filter: blur(16px) saturate(1.4);
    -webkit-backdrop-filter: blur(16px) saturate(1.4);
    border: 1px solid rgba(255, 255, 255, 0.12);
    box-shadow:
      0 4px 24px rgba(0, 0, 0, 0.3),
      0 0 0 1px rgba(255, 255, 255, 0.05) inset;
  }

  .recording .overlay-pill {
    border-color: rgba(239, 68, 68, 0.35);
    box-shadow:
      0 4px 24px rgba(239, 68, 68, 0.15),
      0 0 0 1px rgba(239, 68, 68, 0.1) inset;
  }

  .processing .overlay-pill {
    border-color: rgba(59, 130, 246, 0.35);
    box-shadow:
      0 4px 24px rgba(59, 130, 246, 0.15),
      0 0 0 1px rgba(59, 130, 246, 0.1) inset;
  }

  .label {
    font-family:
      -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif;
    font-size: 13px;
    font-weight: 500;
    color: rgba(255, 255, 255, 0.9);
    letter-spacing: 0.01em;
  }

  /* Pulsing red recording dot */
  .recording-dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: #ef4444;
    animation: pulse 1.5s ease-in-out infinite;
  }

  @keyframes pulse {
    0%,
    100% {
      opacity: 1;
      transform: scale(1);
    }
    50% {
      opacity: 0.5;
      transform: scale(0.85);
    }
  }

  /* Spinning indicator */
  .spinner {
    width: 14px;
    height: 14px;
    border: 2px solid rgba(255, 255, 255, 0.2);
    border-top-color: rgba(59, 130, 246, 0.9);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  /* Live preview card */
  .preview-card {
    max-width: 320px;
    max-height: 80px;
    padding: 8px 12px;
    border-radius: 12px;
    background: rgba(30, 30, 30, 0.65);
    backdrop-filter: blur(16px) saturate(1.4);
    -webkit-backdrop-filter: blur(16px) saturate(1.4);
    border: 1px solid rgba(255, 255, 255, 0.12);
    box-shadow:
      0 4px 24px rgba(0, 0, 0, 0.3),
      0 0 0 1px rgba(255, 255, 255, 0.05) inset;
    overflow: hidden;
    opacity: 0;
    transform: translateY(4px);
    animation: preview-in 0.2s ease forwards;
  }

  @keyframes preview-in {
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  .preview-header {
    font-family:
      -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif;
    font-size: 10px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.4);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    margin-bottom: 4px;
  }

  .preview-text {
    font-family:
      -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif;
    font-size: 12px;
    font-weight: 400;
    color: rgba(255, 255, 255, 0.75);
    line-height: 1.4;
    overflow: hidden;
    display: -webkit-box;
    -webkit-line-clamp: 3;
    -webkit-box-orient: vertical;
  }
</style>
