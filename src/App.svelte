<script lang="ts">
  import { t } from "$lib/i18n/index.js";
  import RecordButton from "$lib/components/RecordButton.svelte";
  import TranscriptionDisplay from "$lib/components/TranscriptionDisplay.svelte";

  let transcriptionText = $state("");
  let transcriptionDuration = $state<number | undefined>(undefined);
  let errorMessage = $state("");

  function handleTranscription(text: string, durationMs: number) {
    transcriptionText = text;
    transcriptionDuration = durationMs;
    errorMessage = "";
  }

  function handleError(error: string) {
    errorMessage = error;
  }
</script>

<div class="dark min-h-screen bg-bg-primary text-text-primary">
  <header class="border-b border-border px-6 py-4">
    <h1 class="text-xl font-semibold">{t("app.title")}</h1>
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
