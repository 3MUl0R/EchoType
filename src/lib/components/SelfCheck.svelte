<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n/index.js";

  // Mic test state
  let micTesting = $state(false);
  let micResult: "idle" | "pass" | "fail" = $state("idle");
  let micDevice = $state("");

  // Transcription test state
  let transcribeTesting = $state(false);
  let transcribeResult: "idle" | "pass" | "fail" = $state("idle");
  let transcribeText = $state("");
  let transcribeTime = $state(0);

  // Track active operations for cleanup (reactive so $effect cleanup sees updates)
  let captureActive = $state(false);
  let activeTimeout = $state<ReturnType<typeof setTimeout> | null>(null);

  $effect(() => {
    return () => {
      if (activeTimeout !== null) clearTimeout(activeTimeout);
      if (captureActive) {
        invoke("stop_capture").catch(() => {});
      }
    };
  });

  async function testMicrophone() {
    micTesting = true;
    micResult = "idle";
    micDevice = "";
    try {
      // Get device info
      const devices = await invoke<{ name: string; is_default: boolean }[]>(
        "list_audio_devices",
      );
      const defaultDev = devices.find((d) => d.is_default);
      micDevice = defaultDev?.name ?? t("selfcheck.no_device");

      // Record 3 seconds
      await invoke("start_capture");
      captureActive = true;

      await new Promise<void>((r) => {
        activeTimeout = setTimeout(r, 3000);
      });
      activeTimeout = null;

      await invoke("stop_capture");
      captureActive = false;

      micResult = "pass";
    } catch (e) {
      if (captureActive) {
        await invoke("stop_capture").catch(() => {});
        captureActive = false;
      }
      micResult = "fail";
      micDevice = String(e);
    } finally {
      micTesting = false;
    }
  }

  async function testTranscription() {
    transcribeTesting = true;
    transcribeResult = "idle";
    transcribeText = "";
    transcribeTime = 0;
    try {
      // Record 3 seconds
      await invoke("start_capture");
      captureActive = true;

      await new Promise<void>((r) => {
        activeTimeout = setTimeout(r, 3000);
      });
      activeTimeout = null;

      await invoke("stop_capture");
      captureActive = false;

      const startMs = globalThis.Date.now();
      const result = await invoke<{ text: string }>("transcribe_audio", {
        language: null,
      });
      transcribeTime = globalThis.Date.now() - startMs;
      transcribeText = result.text;
      transcribeResult = transcribeText.trim().length > 0 ? "pass" : "fail";
    } catch (e) {
      if (captureActive) {
        await invoke("stop_capture").catch(() => {});
        captureActive = false;
      }
      transcribeResult = "fail";
      transcribeText = String(e);
    } finally {
      transcribeTesting = false;
    }
  }
</script>

<div class="flex flex-col gap-6">
  <!-- Microphone Self-Check -->
  <div class="rounded-lg border border-border p-4">
    <h4 class="mb-2 text-sm font-medium">{t("selfcheck.mic_title")}</h4>
    <p class="mb-3 text-xs text-text-muted">{t("selfcheck.mic_desc")}</p>
    <div class="flex items-center gap-3">
      <button
        onclick={testMicrophone}
        disabled={micTesting || transcribeTesting}
        class="rounded bg-accent px-3 py-1.5 text-sm text-white hover:bg-accent-hover disabled:opacity-50"
      >
        {micTesting ? t("selfcheck.testing") : t("selfcheck.mic_test")}
      </button>
      {#if micResult === "pass"}
        <span class="flex items-center gap-1 text-sm text-status-success">
          <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
            <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
          </svg>
          {t("selfcheck.pass")}
        </span>
      {:else if micResult === "fail"}
        <span class="flex items-center gap-1 text-sm text-status-recording">
          <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
            <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
          </svg>
          {t("selfcheck.fail")}
        </span>
      {/if}
    </div>
    {#if micDevice}
      <p class="mt-2 text-xs text-text-muted">
        {t("selfcheck.device")}: {micDevice}
      </p>
    {/if}
  </div>

  <!-- Transcription Self-Check -->
  <div class="rounded-lg border border-border p-4">
    <h4 class="mb-2 text-sm font-medium">{t("selfcheck.transcribe_title")}</h4>
    <p class="mb-3 text-xs text-text-muted">{t("selfcheck.transcribe_desc")}</p>
    <div class="flex items-center gap-3">
      <button
        onclick={testTranscription}
        disabled={transcribeTesting || micTesting}
        class="rounded bg-accent px-3 py-1.5 text-sm text-white hover:bg-accent-hover disabled:opacity-50"
      >
        {transcribeTesting ? t("selfcheck.testing") : t("selfcheck.transcribe_test")}
      </button>
      {#if transcribeResult === "pass"}
        <span class="flex items-center gap-1 text-sm text-status-success">
          <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
            <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
          </svg>
          {t("selfcheck.pass")} ({transcribeTime}ms)
        </span>
      {:else if transcribeResult === "fail"}
        <span class="flex items-center gap-1 text-sm text-status-recording">
          <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
            <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
          </svg>
          {t("selfcheck.fail")}
        </span>
      {/if}
    </div>
    {#if transcribeText}
      <div class="mt-2 rounded bg-bg-surface p-2 text-xs">
        {transcribeText}
      </div>
    {/if}
  </div>
</div>
