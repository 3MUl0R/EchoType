<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";

  type DictationState =
    | "idle"
    | "recording"
    | "transcribing"
    | "editing"
    | "inserting";

  interface DictationEvent {
    state: DictationState;
    text: string | null;
    error: string | null;
    latency_ms: number | null;
  }

  let dictationState: DictationState = $state("idle");
  let visible = $state(false);

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

  $effect(() => {
    const unlisten = listen<DictationEvent>("dictation:state", (event) => {
      const state = event.payload.state;
      dictationState = state;

      if (state === "recording" || state === "transcribing") {
        showOverlay();
      } else if (state === "idle") {
        hideOverlay();
      }
      // "editing" and "inserting" — keep visible if already shown
    });

    return () => {
      unlisten.then((fn) => fn());
      if (fadeTimeout) clearTimeout(fadeTimeout);
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
    dictationState === "inserting"}
>
  <div class="overlay-pill">
    {#if dictationState === "recording"}
      <div class="recording-dot"></div>
      <span class="label">Listening…</span>
    {:else if dictationState === "transcribing" || dictationState === "inserting"}
      <div class="spinner"></div>
      <span class="label">Processing…</span>
    {:else}
      <span class="label">Ready</span>
    {/if}
  </div>
</div>

<style>
  .overlay-container {
    position: fixed;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
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
</style>
