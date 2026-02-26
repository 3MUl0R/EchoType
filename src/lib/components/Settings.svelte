<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n/index.js";
  import PermissionGuide from "./PermissionGuide.svelte";

  interface AllSettings {
    hotkey: string;
    output_method: string;
    active_model_id: string | null;
    language: string;
    denoise_enabled: boolean;
    log_level: string;
    history_retention_count: number;
    history_retention_days: number | null;
    history_enabled: boolean;
    overlay_enabled: boolean;
    audio_feedback_enabled: boolean;
    audio_feedback_volume: number;
    selected_mic_device: string | null;
    mic_auto_fallback: boolean;
    activation_mode: string;
    noise_suppression_level: string;
    auto_punctuate: boolean;
    dictation_mode: string;
    silence_cutoff_seconds: number;
    toggle_auto_stop_enabled: boolean;
  }

  interface AudioDevice {
    name: string;
    is_default: boolean;
  }

  let settings: AllSettings | null = $state(null);
  let audioDevices: AudioDevice[] = $state([]);
  let errorMessage = $state("");
  let successMessage = $state("");

  async function loadSettings() {
    try {
      settings = await invoke<AllSettings>("get_all_settings");
      errorMessage = "";
    } catch (e) {
      errorMessage = String(e);
    }
  }

  async function saveSetting(key: string, value: unknown) {
    try {
      await invoke("set_setting", { key, value: JSON.stringify(value) });
      successMessage = t("settings.saved");
      setTimeout(() => (successMessage = ""), 2000);
    } catch (e) {
      errorMessage = String(e);
    }
  }

  async function handleExport() {
    try {
      const data = await invoke("export_settings");
      const blob = new Blob([JSON.stringify(data, null, 2)], {
        type: "application/json",
      });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = "echotype-settings.json";
      a.click();
      URL.revokeObjectURL(url);
    } catch (e) {
      errorMessage = String(e);
    }
  }

  async function handleImport() {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".json";
    input.onchange = async () => {
      const file = input.files?.[0];
      if (!file) return;
      try {
        const text = await file.text();
        const data = JSON.parse(text);
        const count = await invoke<number>("import_settings", { data });
        successMessage = `${t("settings.imported")} (${count})`;
        setTimeout(() => (successMessage = ""), 3000);
        await loadSettings();
      } catch (e) {
        errorMessage = String(e);
      }
    };
    input.click();
  }

  async function loadAudioDevices() {
    try {
      audioDevices = await invoke<AudioDevice[]>("list_audio_devices");
    } catch {
      audioDevices = [];
    }
  }

  $effect(() => {
    loadSettings();
    loadAudioDevices();
  });
</script>

<div class="mx-auto max-w-2xl px-6 py-8">
  <h2 class="mb-6 text-lg font-semibold">{t("settings.title")}</h2>

  {#if errorMessage}
    <p
      class="mb-4 rounded bg-status-recording/20 p-3 text-sm text-status-recording"
      role="alert"
    >
      {errorMessage}
    </p>
  {/if}

  {#if successMessage}
    <p
      class="mb-4 rounded bg-status-success/20 p-3 text-sm text-status-success"
      role="status"
    >
      {successMessage}
    </p>
  {/if}

  <PermissionGuide />

  {#if settings}
    <!-- Dictation Section -->
    <section class="mb-8">
      <h3 class="mb-4 text-sm font-medium uppercase tracking-wide text-text-secondary">
        {t("settings.section_dictation")}
      </h3>
      <div class="space-y-4">
        <div class="flex items-center justify-between">
          <label for="activation-mode" class="text-sm"
            >{t("settings.activation_mode")}</label
          >
          <select
            id="activation-mode"
            value={settings.activation_mode}
            onchange={(e) =>
              saveSetting(
                "activation_mode",
                (e.target as HTMLSelectElement).value,
              )}
            class="rounded border border-border bg-bg-primary px-3 py-1 text-sm"
          >
            <option value="hold">{t("settings.mode_hold")}</option>
            <option value="toggle">{t("settings.mode_toggle")}</option>
          </select>
        </div>

        <div class="flex items-center justify-between">
          <label for="hotkey" class="text-sm">{t("settings.hotkey")}</label>
          <span
            id="hotkey"
            class="rounded border border-border bg-bg-primary px-3 py-1 text-sm font-mono"
          >
            {settings.hotkey}
          </span>
        </div>

        <div class="flex items-center justify-between">
          <label for="output-method" class="text-sm"
            >{t("settings.output_method")}</label
          >
          <select
            id="output-method"
            value={settings.output_method}
            onchange={(e) =>
              saveSetting(
                "output_method",
                (e.target as HTMLSelectElement).value,
              )}
            class="rounded border border-border bg-bg-primary px-3 py-1 text-sm"
          >
            <option value="direct_input">{t("settings.output_direct")}</option>
            <option value="clipboard_paste"
              >{t("settings.output_clipboard_paste")}</option
            >
            <option value="clipboard_only"
              >{t("settings.output_clipboard_only")}</option
            >
          </select>
        </div>

        <div class="flex items-center justify-between">
          <label for="language" class="text-sm">{t("settings.language")}</label>
          <select
            id="language"
            value={settings.language}
            onchange={(e) =>
              saveSetting("language", (e.target as HTMLSelectElement).value)}
            class="rounded border border-border bg-bg-primary px-3 py-1 text-sm"
          >
            <option value="en">English</option>
            <option value="auto">Auto-detect</option>
          </select>
        </div>

        <div class="flex items-center justify-between">
          <label for="dictation-mode" class="text-sm"
            >{t("settings.dictation_mode")}</label
          >
          <select
            id="dictation-mode"
            value={settings.dictation_mode}
            onchange={(e) =>
              saveSetting(
                "dictation_mode",
                (e.target as HTMLSelectElement).value,
              )}
            class="rounded border border-border bg-bg-primary px-3 py-1 text-sm"
          >
            <option value="formatted">{t("settings.mode_formatted")}</option>
            <option value="raw">{t("settings.mode_raw")}</option>
          </select>
        </div>

        <div class="flex items-center justify-between">
          <label for="auto-punctuate" class="text-sm"
            >{t("settings.auto_punctuate")}</label
          >
          <input
            id="auto-punctuate"
            type="checkbox"
            checked={settings.auto_punctuate}
            onchange={(e) =>
              saveSetting(
                "auto_punctuate",
                (e.target as HTMLInputElement).checked,
              )}
            class="h-4 w-4 rounded accent-accent"
          />
        </div>

        {#if settings.activation_mode === "toggle"}
          <div class="flex items-center justify-between">
            <label for="toggle-auto-stop" class="text-sm"
              >{t("settings.toggle_auto_stop")}</label
            >
            <input
              id="toggle-auto-stop"
              type="checkbox"
              checked={settings.toggle_auto_stop_enabled}
              onchange={(e) =>
                saveSetting(
                  "toggle_auto_stop_enabled",
                  (e.target as HTMLInputElement).checked,
                )}
              class="h-4 w-4 rounded accent-accent"
            />
          </div>

          {#if settings.toggle_auto_stop_enabled}
            <div class="flex items-center justify-between">
              <label for="silence-cutoff" class="text-sm"
                >{t("settings.silence_cutoff")}</label
              >
              <input
                id="silence-cutoff"
                type="number"
                min="0.5"
                max="10"
                step="0.5"
                value={settings.silence_cutoff_seconds}
                onchange={(e) =>
                  saveSetting(
                    "silence_cutoff_seconds",
                    parseFloat((e.target as HTMLInputElement).value) || 1.5,
                  )}
                class="w-24 rounded border border-border bg-bg-primary px-3 py-1 text-sm"
              />
            </div>
          {/if}
        {/if}
      </div>
    </section>

    <!-- Engine Section -->
    <section class="mb-8">
      <h3 class="mb-4 text-sm font-medium uppercase tracking-wide text-text-secondary">
        {t("settings.section_engine")}
      </h3>
      <div class="space-y-4">
        <div class="flex items-center justify-between">
          <span class="text-sm">{t("settings.active_model")}</span>
          <span class="text-sm text-text-secondary">
            {settings.active_model_id ?? t("settings.no_model")}
          </span>
        </div>

        <div class="flex items-center justify-between">
          <label for="noise-suppression" class="text-sm"
            >{t("settings.noise_suppression")}</label
          >
          <select
            id="noise-suppression"
            value={settings.noise_suppression_level}
            onchange={(e) =>
              saveSetting(
                "noise_suppression_level",
                (e.target as HTMLSelectElement).value,
              )}
            class="rounded border border-border bg-bg-primary px-3 py-1 text-sm"
          >
            <option value="off">{t("settings.suppression_off")}</option>
            <option value="light">{t("settings.suppression_light")}</option>
            <option value="moderate">{t("settings.suppression_moderate")}</option>
            <option value="aggressive"
              >{t("settings.suppression_aggressive")}</option
            >
          </select>
        </div>
      </div>
    </section>

    <!-- Microphone Section -->
    <section class="mb-8">
      <h3 class="mb-4 text-sm font-medium uppercase tracking-wide text-text-secondary">
        {t("settings.section_microphone")}
      </h3>
      <div class="space-y-4">
        <div class="flex items-center justify-between">
          <label for="mic-device" class="text-sm"
            >{t("settings.mic_device")}</label
          >
          <select
            id="mic-device"
            value={settings.selected_mic_device ?? ""}
            onchange={(e) => {
              const val = (e.target as HTMLSelectElement).value;
              if (val === "") {
                saveSetting("selected_mic_device", null);
              } else {
                saveSetting("selected_mic_device", val);
              }
            }}
            class="max-w-48 truncate rounded border border-border bg-bg-primary px-3 py-1 text-sm"
          >
            <option value="">{t("settings.mic_default")}</option>
            {#each audioDevices as device (device.name)}
              <option value={device.name}>{device.name}</option>
            {/each}
          </select>
        </div>

        <div class="flex items-center justify-between">
          <label for="mic-fallback" class="text-sm"
            >{t("settings.mic_auto_fallback")}</label
          >
          <input
            id="mic-fallback"
            type="checkbox"
            checked={settings.mic_auto_fallback}
            onchange={(e) =>
              saveSetting(
                "mic_auto_fallback",
                (e.target as HTMLInputElement).checked,
              )}
            class="h-4 w-4 rounded accent-accent"
          />
        </div>
      </div>
    </section>

    <!-- Feedback Section -->
    <section class="mb-8">
      <h3 class="mb-4 text-sm font-medium uppercase tracking-wide text-text-secondary">
        {t("settings.section_feedback")}
      </h3>
      <div class="space-y-4">
        <div class="flex items-center justify-between">
          <label for="audio-feedback" class="text-sm"
            >{t("settings.audio_feedback")}</label
          >
          <input
            id="audio-feedback"
            type="checkbox"
            checked={settings.audio_feedback_enabled}
            onchange={(e) =>
              saveSetting(
                "audio_feedback_enabled",
                (e.target as HTMLInputElement).checked,
              )}
            class="h-4 w-4 rounded accent-accent"
          />
        </div>

        {#if settings.audio_feedback_enabled}
          <div class="flex items-center justify-between">
            <label for="audio-volume" class="text-sm"
              >{t("settings.audio_volume")}</label
            >
            <input
              id="audio-volume"
              type="range"
              min="0"
              max="1"
              step="0.1"
              value={settings.audio_feedback_volume}
              onchange={(e) =>
                saveSetting(
                  "audio_feedback_volume",
                  parseFloat((e.target as HTMLInputElement).value),
                )}
              class="w-32 accent-accent"
            />
          </div>
        {/if}

        <div class="flex items-center justify-between">
          <label for="overlay" class="text-sm"
            >{t("settings.overlay_enabled")}</label
          >
          <input
            id="overlay"
            type="checkbox"
            checked={settings.overlay_enabled}
            onchange={(e) =>
              saveSetting(
                "overlay_enabled",
                (e.target as HTMLInputElement).checked,
              )}
            class="h-4 w-4 rounded accent-accent"
          />
        </div>
      </div>
    </section>

    <!-- History Section -->
    <section class="mb-8">
      <h3 class="mb-4 text-sm font-medium uppercase tracking-wide text-text-secondary">
        {t("settings.section_history")}
      </h3>
      <div class="space-y-4">
        <div class="flex items-center justify-between">
          <label for="history-enabled" class="text-sm"
            >{t("settings.history_enabled")}</label
          >
          <input
            id="history-enabled"
            type="checkbox"
            checked={settings.history_enabled}
            onchange={(e) =>
              saveSetting(
                "history_enabled",
                (e.target as HTMLInputElement).checked,
              )}
            class="h-4 w-4 rounded accent-accent"
          />
        </div>

        <div class="flex items-center justify-between">
          <label for="retention-count" class="text-sm"
            >{t("settings.retention_count")}</label
          >
          <input
            id="retention-count"
            type="number"
            min="1"
            max="10000"
            value={settings.history_retention_count}
            onchange={(e) =>
              saveSetting(
                "history_retention_count",
                parseInt((e.target as HTMLInputElement).value) || 50,
              )}
            class="w-24 rounded border border-border bg-bg-primary px-3 py-1 text-sm"
          />
        </div>
      </div>
    </section>

    <!-- Advanced Section -->
    <section class="mb-8">
      <h3 class="mb-4 text-sm font-medium uppercase tracking-wide text-text-secondary">
        {t("settings.section_advanced")}
      </h3>
      <div class="space-y-4">
        <div class="flex items-center justify-between">
          <label for="log-level" class="text-sm"
            >{t("settings.log_level")}</label
          >
          <select
            id="log-level"
            value={settings.log_level}
            onchange={(e) =>
              saveSetting("log_level", (e.target as HTMLSelectElement).value)}
            class="rounded border border-border bg-bg-primary px-3 py-1 text-sm"
          >
            <option value="error">Error</option>
            <option value="warn">Warn</option>
            <option value="info">Info</option>
            <option value="debug">Debug</option>
            <option value="trace">Trace</option>
          </select>
        </div>

        <div class="flex gap-2">
          <button
            onclick={handleExport}
            class="rounded bg-bg-primary px-3 py-1.5 text-sm text-text-primary border border-border hover:bg-bg-surface focus:outline-none focus:ring-2 focus:ring-accent"
          >
            {t("settings.export")}
          </button>
          <button
            onclick={handleImport}
            class="rounded bg-bg-primary px-3 py-1.5 text-sm text-text-primary border border-border hover:bg-bg-surface focus:outline-none focus:ring-2 focus:ring-accent"
          >
            {t("settings.import")}
          </button>
        </div>
      </div>
    </section>
  {/if}
</div>
