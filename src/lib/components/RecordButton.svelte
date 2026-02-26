<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n/index.js";

  type RecordState = "idle" | "recording" | "processing";

  interface Props {
    onTranscription?: (_text: string, _durationMs: number) => void;
    onError?: (_error: string) => void;
  }

  let { onTranscription, onError }: Props = $props();

  let state: RecordState = $state("idle");
  let elapsedSeconds = $state(0);
  let timerInterval: ReturnType<typeof setInterval> | null = $state(null);

  function startTimer() {
    elapsedSeconds = 0;
    timerInterval = setInterval(() => {
      elapsedSeconds += 1;
    }, 1000);
  }

  function stopTimer() {
    if (timerInterval) {
      clearInterval(timerInterval);
      timerInterval = null;
    }
  }

  // Clean up timer on component destroy
  $effect(() => {
    return () => {
      stopTimer();
    };
  });

  function formatTime(seconds: number): string {
    const m = Math.floor(seconds / 60);
    const s = seconds % 60;
    return `${m}:${s.toString().padStart(2, "0")}`;
  }

  async function handleClick() {
    if (state === "processing") return;

    if (state === "idle") {
      await startRecording();
    } else if (state === "recording") {
      await stopRecording();
    }
  }

  async function startRecording() {
    try {
      await invoke("start_capture");
      state = "recording";
      startTimer();
    } catch (e) {
      onError?.(String(e));
    }
  }

  async function stopRecording() {
    stopTimer();
    state = "processing";

    try {
      await invoke<number>("stop_capture");
      const result = await invoke<{ text: string; duration_ms: number }>(
        "transcribe_audio",
        { language: null },
      );
      state = "idle";
      onTranscription?.(result.text, result.duration_ms);
    } catch (e) {
      state = "idle";
      onError?.(String(e));
    }
  }
</script>

<div class="flex flex-col items-center gap-4">
  <button
    onclick={handleClick}
    disabled={state === "processing"}
    aria-pressed={state === "recording"}
    aria-label={state === "idle"
      ? t("record.idle")
      : state === "recording"
        ? t("record.stop")
        : t("record.processing")}
    class="relative flex h-20 w-20 items-center justify-center rounded-full
      transition-all duration-200 focus:outline-none focus:ring-4 focus:ring-accent/50
      {state === 'idle'
      ? 'bg-accent hover:bg-accent-hover cursor-pointer'
      : state === 'recording'
        ? 'bg-status-recording animate-pulse cursor-pointer'
        : 'bg-status-processing cursor-wait'}"
  >
    {#if state === "idle"}
      <!-- Microphone icon -->
      <svg
        class="h-8 w-8 text-white"
        fill="none"
        viewBox="0 0 24 24"
        stroke="currentColor"
        stroke-width="2"
        aria-hidden="true"
      >
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          d="M12 1a3 3 0 00-3 3v8a3 3 0 006 0V4a3 3 0 00-3-3z"
        />
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          d="M19 10v2a7 7 0 01-14 0v-2M12 19v4M8 23h8"
        />
      </svg>
    {:else if state === "recording"}
      <!-- Stop icon -->
      <svg
        class="h-8 w-8 text-white"
        fill="currentColor"
        viewBox="0 0 24 24"
        aria-hidden="true"
      >
        <rect x="6" y="6" width="12" height="12" rx="2" />
      </svg>
    {:else}
      <!-- Spinner -->
      <svg
        class="h-8 w-8 animate-spin text-white"
        fill="none"
        viewBox="0 0 24 24"
        aria-hidden="true"
      >
        <circle
          class="opacity-25"
          cx="12"
          cy="12"
          r="10"
          stroke="currentColor"
          stroke-width="4"
        />
        <path
          class="opacity-75"
          fill="currentColor"
          d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"
        />
      </svg>
    {/if}
  </button>

  <span class="text-sm text-text-secondary" aria-live="polite">
    {#if state === "idle"}
      {t("record.idle")}
    {:else if state === "recording"}
      <span aria-label={t("record.timer")}>{formatTime(elapsedSeconds)}</span>
      — {t("record.recording")}
    {:else}
      {t("record.processing")}
    {/if}
  </span>
</div>
