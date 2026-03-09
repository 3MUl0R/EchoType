<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { t } from "$lib/i18n/index.js";
  import RecordButton from "$lib/components/RecordButton.svelte";
  import TranscriptionDisplay from "$lib/components/TranscriptionDisplay.svelte";
  import ModelManager from "$lib/components/ModelManager.svelte";
  import History from "$lib/components/History.svelte";
  import Settings from "$lib/components/Settings.svelte";
  import Dashboard from "$lib/components/Dashboard.svelte";
  import SetupWizard from "$lib/components/SetupWizard.svelte";
  import SetupBanner from "$lib/components/SetupBanner.svelte";
  import UpdateNotification from "$lib/components/UpdateNotification.svelte";
  import { initTheme } from "$lib/theme/index.js";

  type DictationState =
    | "idle"
    | "recording"
    | "transcribing"
    | "editing"
    | "inserting";

  type Page = "dictation" | "models" | "history" | "dashboard" | "settings";

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
  let currentPage: Page = $state("dictation");
  let showWizard = $state(false);
  let wizardChecked = $state(false);
  let bannerDismissed = $state(false);

  function handleTranscription(text: string, durationMs: number) {
    transcriptionText = text;
    transcriptionDuration = durationMs;
    errorMessage = "";
  }

  function handleError(error: string) {
    errorMessage = error;
  }

  // Fetch initial dictation state, theme, and wizard status on mount
  $effect(() => {
    invoke<DictationState>("get_dictation_state").then((state) => {
      dictationState = state;
    });
    initTheme();

    // Check if wizard needs to run
    invoke<string>("get_setting", { key: "wizard_completed" }).then((raw) => {
      try {
        const completed = JSON.parse(raw);
        showWizard = !completed;
      } catch {
        showWizard = true;
      }
      wizardChecked = true;
    }).catch(() => {
      showWizard = true;
      wizardChecked = true;
    });
  });

  // Listen for navigation events from the system tray
  $effect(() => {
    const unlisten = listen<string>("navigate", (event) => {
      const page = event.payload as Page;
      if (["dictation", "models", "history", "dashboard", "settings"].includes(page)) {
        currentPage = page;
      }
    });

    return () => {
      unlisten.then((fn) => fn());
    };
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

{#if showWizard && wizardChecked}
  <SetupWizard onComplete={() => (showWizard = false)} />
{:else if wizardChecked}
<div class="flex h-screen flex-col overflow-hidden bg-bg-primary text-text-primary">
  {#if !bannerDismissed}
    <SetupBanner
      onNavigate={(page) => { currentPage = page as Page; bannerDismissed = true; }}
      onDismiss={() => (bannerDismissed = true)}
    />
  {/if}
  <UpdateNotification />
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
          onclick={() => (currentPage = "dictation")}
          class="rounded px-2 py-1 focus:outline-none focus:ring-2 focus:ring-accent
            {currentPage === 'dictation'
            ? 'text-accent hover:text-accent-hover'
            : 'text-text-secondary hover:text-text-primary'}"
          aria-current={currentPage === "dictation" ? "page" : undefined}
        >
          {t("nav.dictation")}
        </button>
      </li>
      <li>
        <button
          onclick={() => (currentPage = "models")}
          class="rounded px-2 py-1 focus:outline-none focus:ring-2 focus:ring-accent
            {currentPage === 'models'
            ? 'text-accent hover:text-accent-hover'
            : 'text-text-secondary hover:text-text-primary'}"
          aria-current={currentPage === "models" ? "page" : undefined}
        >
          {t("nav.models")}
        </button>
      </li>
      <li>
        <button
          onclick={() => (currentPage = "history")}
          class="rounded px-2 py-1 focus:outline-none focus:ring-2 focus:ring-accent
            {currentPage === 'history'
            ? 'text-accent hover:text-accent-hover'
            : 'text-text-secondary hover:text-text-primary'}"
          aria-current={currentPage === "history" ? "page" : undefined}
        >
          {t("nav.history")}
        </button>
      </li>
      <li>
        <button
          onclick={() => (currentPage = "dashboard")}
          class="rounded px-2 py-1 focus:outline-none focus:ring-2 focus:ring-accent
            {currentPage === 'dashboard'
            ? 'text-accent hover:text-accent-hover'
            : 'text-text-secondary hover:text-text-primary'}"
          aria-current={currentPage === "dashboard" ? "page" : undefined}
        >
          {t("nav.dashboard")}
        </button>
      </li>
      <li>
        <button
          onclick={() => (currentPage = "settings")}
          class="rounded px-2 py-1 focus:outline-none focus:ring-2 focus:ring-accent
            {currentPage === 'settings'
            ? 'text-accent hover:text-accent-hover'
            : 'text-text-secondary hover:text-text-primary'}"
          aria-current={currentPage === "settings" ? "page" : undefined}
        >
          {t("nav.settings")}
        </button>
      </li>
    </ul>
  </nav>

  {#if currentPage === "dictation"}
    <main class="flex-1 overflow-y-auto">
      <div class="mx-auto max-w-2xl px-6 py-8">
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
      </div>
    </main>
  {:else if currentPage === "models"}
    <main class="flex-1 overflow-y-auto">
      <ModelManager />
    </main>
  {:else if currentPage === "history"}
    <main class="flex-1 overflow-y-auto">
      <History />
    </main>
  {:else if currentPage === "dashboard"}
    <main class="flex-1 overflow-y-auto">
      <Dashboard />
    </main>
  {:else if currentPage === "settings"}
    <main class="flex-1 overflow-hidden">
      <Settings onRunWizard={() => { invoke("set_setting", { key: "wizard_completed", value: "false" }); showWizard = true; }} />
    </main>
  {/if}
</div>
{/if}
