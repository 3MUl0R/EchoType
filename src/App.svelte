<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { t } from "$lib/i18n/index.js";
  import RecordButton from "$lib/components/RecordButton.svelte";
  import TranscriptionDisplay from "$lib/components/TranscriptionDisplay.svelte";

  type DictationState =
    | "idle"
    | "recording"
    | "transcribing"
    | "inserting";

  interface DictationEvent {
    state: DictationState;
    text: string | null;
    error: string | null;
    latency_ms: number | null;
  }

  let transcriptionText = $state("");
  let transcriptionDuration = $state<number | undefined>(undefined);
  let errorMessage = $state("");
  let dictationState: DictationState = $state("idle");

  function handleTranscription(text: string, durationMs: number) {
    transcriptionText = text;
    transcriptionDuration = durationMs;
    errorMessage = "";
  }

  function handleError(error: string) {
    errorMessage = error;
  }

  // Fetch initial dictation state on mount
  $effect(() => {
    invoke<DictationState>("get_dictation_state").then((state) => {
      dictationState = state;
    });
  });

  // Listen for dictation state events from the global hotkey flow
  $effect(() => {
    const unlisten = listen<DictationEvent>(
      "dictation:state",
      (event) => {
        const data = event.payload;
        dictationState = data.state;

        if (data.text !== null) {
          transcriptionText = data.text;
          errorMessage = "";
        }
        if (data.error !== null) {
          errorMessage = data.error;
        }
        if (data.latency_ms !== null) {
          transcriptionDuration = data.latency_ms;
        }
      },
    );

    return () => {
      unlisten.then((fn) => fn());
    };
  });
</script>

<div class="dark min-h-screen bg-bg-primary text-text-primary">
  <header class="border-b border-border px-6 py-4">
    <div class="flex items-center justify-between">
      <h1 class="text-xl font-semibold">{t("app.title")}</h1>
      {#if dictationState !== "idle"}
        <span
          class="rounded-full px-3 py-1 text-xs font-medium
            {dictationState === 'recording'
            ? 'bg-status-recording/20 text-status-recording'
            : dictationState === 'transcribing'
              ? 'bg-status-processing/20 text-status-processing'
              : 'bg-status-success/20 text-status-success'}"
          aria-live="polite"
        >
          {t(`dictation.${dictationState}`)}
        </span>
      {/if}
    </div>
  </header>

  <nav class="border-b border-border px-6 py-2" aria-label={t("nav.label")}>
    <ul class="flex gap-4">
      <li>
        <button
          class="rounded px-2 py-1 text-accent hover:text-accent-hover focus:outline-none focus:ring-2 focus:ring-accent"
        >
          {t("nav.dictation")}
        </button>
      </li>
      <li>
        <button
          class="rounded px-2 py-1 text-text-secondary hover:text-text-primary focus:outline-none focus:ring-2 focus:ring-accent"
        >
          {t("nav.settings")}
        </button>
      </li>
    </ul>
  </nav>

  <main class="mx-auto max-w-2xl px-6 py-8">
    <p class="mb-8 text-center text-text-secondary">{t("app.description")}</p>

    <div class="mb-8 flex justify-center">
      <RecordButton
        onTranscription={handleTranscription}
        onError={handleError}
      />
    </div>

    <TranscriptionDisplay
      text={transcriptionText}
      durationMs={transcriptionDuration}
      error={errorMessage}
    />
  </main>
</div>
