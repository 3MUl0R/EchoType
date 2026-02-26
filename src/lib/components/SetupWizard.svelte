<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n/index.js";
  import WizardProgress from "./WizardProgress.svelte";
  import WizardStep from "./WizardStep.svelte";

  interface Props {
    onComplete: () => void;
  }

  let { onComplete }: Props = $props();

  const TOTAL_STEPS = 7;
  let step = $state(0);

  // Readiness state
  let micGranted = $state(false);
  let platformGranted = $state(false);
  let modelDownloaded = $state(false);
  let testPassed = $state(false);

  // Step-specific state
  let micLevel = $state(0);
  let micTesting = $state(false);
  let modelDownloading = $state(false);
  let modelProgress = $state(0);
  let selectedModel = $state("ggml-base.en");
  let currentHotkey = $state("");
  let testRecording = $state(false);
  let testText = $state("");
  let permissions = $state({ accessibility: false, microphone: false });
  let platform = $state("unknown");

  // Track active timers/captures for cleanup
  let activeInterval: ReturnType<typeof setInterval> | null = null;
  let activeTimeout: ReturnType<typeof setTimeout> | null = null;
  let captureActive = false;

  interface ModelOption {
    id: string;
    label: string;
    description: string;
    size: string;
  }

  const modelOptions: ModelOption[] = [
    {
      id: "ggml-tiny.en",
      label: t("model.tiny_label"),
      description: t("model.tiny_desc"),
      size: "~75 MB",
    },
    {
      id: "ggml-base.en",
      label: t("model.base_label"),
      description: t("model.base_desc"),
      size: "~142 MB",
    },
    {
      id: "ggml-small.en",
      label: t("model.small_label"),
      description: t("model.small_desc"),
      size: "~466 MB",
    },
  ];

  const stepLabels = [
    t("wizard.step_welcome"),
    t("wizard.step_mic"),
    t("wizard.step_permissions"),
    t("wizard.step_model"),
    t("wizard.step_hotkey"),
    t("wizard.step_test"),
    t("wizard.step_done"),
  ];

  $effect(() => {
    loadInitialState();

    return () => {
      // Cleanup on unmount: cancel timers and stop any active capture
      if (activeInterval !== null) clearInterval(activeInterval);
      if (activeTimeout !== null) clearTimeout(activeTimeout);
      if (captureActive) {
        invoke("stop_capture").catch(() => {});
        captureActive = false;
      }
    };
  });

  async function loadInitialState() {
    try {
      permissions = await invoke("check_permissions");
      micGranted = permissions.microphone;
      platformGranted = permissions.accessibility;

      const settings = await invoke<Record<string, unknown>>("get_all_settings");
      currentHotkey = (settings.hotkey as string) ?? "Cmd+Shift+Space";

      // Check if any models are installed
      const models = await invoke<{ installed: boolean }[]>(
        "list_available_models",
      );
      modelDownloaded = models.some((m) => m.installed);

      // Detect platform
      const ua = navigator.userAgent.toLowerCase();
      if (ua.includes("mac")) platform = "macos";
      else if (ua.includes("win")) platform = "windows";
      else platform = "linux";
    } catch (e) {
      console.error("Failed to load wizard state:", e);
    }
  }

  function next() {
    if (step < TOTAL_STEPS - 1) step++;
  }
  function back() {
    if (step > 0) step--;
  }
  function skip() {
    next();
  }

  async function testMicrophone() {
    micTesting = true;
    try {
      await invoke("start_capture");
      captureActive = true;
      // Simulate mic level monitoring for a brief period
      let elapsed = 0;
      activeInterval = setInterval(() => {
        micLevel = Math.random() * 80 + 20;
        elapsed += 100;
        if (elapsed >= 2000 && activeInterval !== null) {
          clearInterval(activeInterval);
          activeInterval = null;
        }
      }, 100);

      await new Promise<void>((r) => {
        activeTimeout = setTimeout(r, 2000);
      });
      activeTimeout = null;
      await invoke("stop_capture");
      captureActive = false;
      micGranted = true;
      micLevel = 0;
    } catch (e) {
      console.error("Mic test failed:", e);
      micGranted = false;
    } finally {
      micTesting = false;
    }
  }

  async function requestPermissions() {
    try {
      await invoke("open_permission_settings", {
        permission: "accessibility",
      });
    } catch (e) {
      console.error("Failed to open permission settings:", e);
    }
  }

  async function refreshPermissions() {
    try {
      permissions = await invoke("check_permissions");
      platformGranted = permissions.accessibility;
    } catch (e) {
      console.error("Failed to refresh permissions:", e);
    }
  }

  async function downloadModel() {
    modelDownloading = true;
    modelProgress = 0;
    try {
      await invoke("download_model", { modelId: selectedModel });
      // Poll for progress (simplified — actual progress comes via events)
      activeInterval = setInterval(async () => {
        try {
          const models = await invoke<{ id: string; installed: boolean; downloading: boolean }[]>(
            "list_available_models",
          );
          const target = models.find((m) => m.id === selectedModel);
          if (target?.installed) {
            if (activeInterval !== null) {
              clearInterval(activeInterval);
              activeInterval = null;
            }
            modelDownloaded = true;
            modelDownloading = false;
            modelProgress = 100;
            // Activate the model
            await invoke("set_active_model", { modelId: selectedModel });
          } else if (!target?.downloading) {
            if (activeInterval !== null) {
              clearInterval(activeInterval);
              activeInterval = null;
            }
            modelDownloading = false;
          } else {
            modelProgress = Math.min(modelProgress + 5, 95);
          }
        } catch {
          if (activeInterval !== null) {
            clearInterval(activeInterval);
            activeInterval = null;
          }
          modelDownloading = false;
        }
      }, 1000);
    } catch (e) {
      console.error("Failed to download model:", e);
      modelDownloading = false;
    }
  }

  async function testDictation() {
    testRecording = true;
    testText = "";
    try {
      await invoke("start_capture");
      captureActive = true;
      await new Promise<void>((r) => {
        activeTimeout = setTimeout(r, 3000);
      });
      activeTimeout = null;
      await invoke("stop_capture");
      captureActive = false;
      const result = await invoke<{ text: string }>("transcribe_audio", {
        language: null,
      });
      testText = result.text;
      testPassed = testText.trim().length > 0;
    } catch (e) {
      testText = String(e);
      testPassed = false;
    } finally {
      testRecording = false;
    }
  }

  async function completeWizard() {
    try {
      await invoke("set_setting", {
        key: "wizard_completed",
        value: "true",
      });
    } catch (e) {
      console.error("Failed to save wizard completion:", e);
    }
    onComplete();
  }
</script>

<div class="flex min-h-screen flex-col items-center justify-center bg-bg-primary px-6 py-8 text-text-primary">
  <WizardProgress {stepLabels} currentStep={step} totalSteps={TOTAL_STEPS} />

  {#if step === 0}
    <!-- Step 1: Welcome -->
    <WizardStep
      title={t("wizard.welcome_title")}
      description={t("wizard.welcome_desc")}
      showBack={false}
      showSkip={false}
      nextLabel={t("wizard.get_started")}
      onNext={next}
    >
        <div class="flex flex-col items-center gap-4">
          <div class="flex h-20 w-20 items-center justify-center rounded-2xl bg-accent/10">
            <svg class="h-10 w-10 text-accent" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
              <path stroke-linecap="round" stroke-linejoin="round" d="M12 18.75a6 6 0 006-6v-1.5m-6 7.5a6 6 0 01-6-6v-1.5m6 7.5v3.75m-3.75 0h7.5M12 15.75a3 3 0 01-3-3V4.5a3 3 0 116 0v8.25a3 3 0 01-3 3z" />
            </svg>
          </div>
          <p class="text-center text-sm text-text-muted">
            {t("wizard.welcome_subtitle")}
          </p>
        </div>
    </WizardStep>

  {:else if step === 1}
    <!-- Step 2: Microphone Permission -->
    <WizardStep
      title={t("wizard.mic_title")}
      description={t("wizard.mic_desc")}
      onBack={back}
      onSkip={skip}
      onNext={next}
      nextDisabled={!micGranted}
    >
        <div class="flex flex-col items-center gap-4">
          {#if micGranted}
            <div class="flex items-center gap-2 text-status-success">
              <svg class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
              </svg>
              <span class="text-sm font-medium">{t("wizard.mic_granted")}</span>
            </div>
          {:else}
            <button
              onclick={testMicrophone}
              disabled={micTesting}
              class="rounded bg-accent px-4 py-2 text-sm text-white hover:bg-accent-hover disabled:opacity-50"
            >
              {micTesting ? t("wizard.mic_testing") : t("wizard.mic_test")}
            </button>
          {/if}
          {#if micTesting}
            <div class="h-2 w-full max-w-xs rounded-full bg-bg-surface">
              <div
                class="h-2 rounded-full bg-status-success transition-all"
                style="width: {micLevel}%"
              ></div>
            </div>
            <p class="text-xs text-text-muted">{t("wizard.mic_speak_now")}</p>
          {/if}
        </div>
    </WizardStep>

  {:else if step === 2}
    <!-- Step 3: Platform Permissions -->
    <WizardStep
      title={t("wizard.permissions_title")}
      description={platform === "macos" ? t("wizard.permissions_desc_mac") : platform === "linux" ? t("wizard.permissions_desc_linux") : t("wizard.permissions_desc_win")}
      onBack={back}
      onSkip={skip}
      onNext={next}
      nextDisabled={false}
    >
        <div class="flex flex-col items-center gap-4">
          {#if platformGranted}
            <div class="flex items-center gap-2 text-status-success">
              <svg class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
              </svg>
              <span class="text-sm font-medium">{t("wizard.permissions_granted")}</span>
            </div>
          {:else}
            <div class="flex flex-col items-center gap-2">
              {#if platform === "macos"}
                <button
                  onclick={requestPermissions}
                  class="rounded bg-accent px-4 py-2 text-sm text-white hover:bg-accent-hover"
                >
                  {t("wizard.permissions_open_settings")}
                </button>
                <button
                  onclick={refreshPermissions}
                  class="text-xs text-accent hover:text-accent-hover"
                >
                  {t("wizard.permissions_refresh")}
                </button>
              {:else}
                <p class="text-sm text-text-muted">{t("wizard.permissions_not_needed")}</p>
              {/if}
            </div>
          {/if}
          <p class="text-xs text-text-muted">
            {t("wizard.permissions_skip_note")}
          </p>
        </div>
    </WizardStep>

  {:else if step === 3}
    <!-- Step 4: Download a Model -->
    <WizardStep
      title={t("wizard.model_title")}
      description={t("wizard.model_desc")}
      onBack={back}
      onSkip={skip}
      onNext={next}
      nextDisabled={false}
    >
        <div class="flex flex-col gap-3">
          {#if modelDownloaded}
            <div class="flex items-center justify-center gap-2 text-status-success">
              <svg class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
              </svg>
              <span class="text-sm font-medium">{t("wizard.model_ready")}</span>
            </div>
          {:else}
            {#each modelOptions as model (model.id)}
              <button
                onclick={() => (selectedModel = model.id)}
                class="flex items-center justify-between rounded-lg border p-3 text-left
                  {selectedModel === model.id ? 'border-accent bg-accent/10' : 'border-border bg-bg-surface hover:bg-bg-secondary'}"
                disabled={modelDownloading}
              >
                <div>
                  <p class="text-sm font-medium">{model.label}</p>
                  <p class="text-xs text-text-muted">{model.description}</p>
                </div>
                <span class="text-xs text-text-muted">{model.size}</span>
              </button>
            {/each}

            {#if modelDownloading}
              <div class="mt-2">
                <div class="h-2 w-full rounded-full bg-bg-surface">
                  <div
                    class="h-2 rounded-full bg-accent transition-all"
                    style="width: {modelProgress}%"
                  ></div>
                </div>
                <p class="mt-1 text-center text-xs text-text-muted">
                  {t("wizard.model_downloading")}
                </p>
              </div>
            {:else}
              <button
                onclick={downloadModel}
                class="mt-2 rounded bg-accent px-4 py-2 text-sm text-white hover:bg-accent-hover"
              >
                {t("wizard.model_download")}
              </button>
            {/if}
          {/if}
        </div>
    </WizardStep>

  {:else if step === 4}
    <!-- Step 5: Set Hotkey -->
    <WizardStep
      title={t("wizard.hotkey_title")}
      description={t("wizard.hotkey_desc")}
      onBack={back}
      onSkip={skip}
      onNext={next}
    >
        <div class="flex flex-col items-center gap-4">
          <div class="rounded-lg border border-border bg-bg-surface px-6 py-3">
            <p class="text-center text-lg font-mono font-medium">{currentHotkey}</p>
          </div>
          <p class="text-xs text-text-muted">
            {t("wizard.hotkey_change_later")}
          </p>
        </div>
    </WizardStep>

  {:else if step === 5}
    <!-- Step 6: Test Dictation -->
    <WizardStep
      title={t("wizard.test_title")}
      description={t("wizard.test_desc")}
      onBack={back}
      onSkip={skip}
      onNext={next}
      nextDisabled={false}
    >
        <div class="flex flex-col items-center gap-4">
          <button
            onclick={testDictation}
            disabled={testRecording}
            class="rounded bg-accent px-6 py-3 text-sm font-medium text-white hover:bg-accent-hover disabled:opacity-50"
          >
            {testRecording ? t("wizard.test_recording") : t("wizard.test_try")}
          </button>
          {#if testText}
            <div class="w-full rounded-lg border border-border bg-bg-surface p-3">
              <p class="text-sm">{testText}</p>
            </div>
            {#if testPassed}
              <div class="flex items-center gap-2 text-status-success">
                <svg class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
                </svg>
                <span class="text-sm font-medium">{t("wizard.test_passed")}</span>
              </div>
            {/if}
          {/if}
        </div>
    </WizardStep>

  {:else if step === 6}
    <!-- Step 7: Done -->
    <WizardStep
      title={t("wizard.done_title")}
      description={t("wizard.done_desc")}
      showBack={false}
      showSkip={false}
      nextLabel={t("wizard.done_finish")}
      onNext={completeWizard}
    >
        <div class="flex flex-col items-center gap-4">
          <div class="flex h-16 w-16 items-center justify-center rounded-full bg-status-success/20">
            <svg class="h-8 w-8 text-status-success" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
            </svg>
          </div>
          <div class="grid grid-cols-2 gap-3 text-center">
            <div class="rounded-lg border border-border p-3">
              <p class="text-xs text-text-muted">{t("wizard.status_mic")}</p>
              <p class="text-sm font-medium {micGranted ? 'text-status-success' : 'text-text-muted'}">
                {micGranted ? t("wizard.status_ok") : t("wizard.status_skipped")}
              </p>
            </div>
            <div class="rounded-lg border border-border p-3">
              <p class="text-xs text-text-muted">{t("wizard.status_permissions")}</p>
              <p class="text-sm font-medium {platformGranted ? 'text-status-success' : 'text-text-muted'}">
                {platformGranted ? t("wizard.status_ok") : t("wizard.status_skipped")}
              </p>
            </div>
            <div class="rounded-lg border border-border p-3">
              <p class="text-xs text-text-muted">{t("wizard.status_model")}</p>
              <p class="text-sm font-medium {modelDownloaded ? 'text-status-success' : 'text-text-muted'}">
                {modelDownloaded ? t("wizard.status_ok") : t("wizard.status_skipped")}
              </p>
            </div>
            <div class="rounded-lg border border-border p-3">
              <p class="text-xs text-text-muted">{t("wizard.status_test")}</p>
              <p class="text-sm font-medium {testPassed ? 'text-status-success' : 'text-text-muted'}">
                {testPassed ? t("wizard.status_ok") : t("wizard.status_skipped")}
              </p>
            </div>
          </div>
        </div>
    </WizardStep>
  {/if}
</div>
