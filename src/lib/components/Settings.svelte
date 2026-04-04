<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t, type StringKeys } from "$lib/i18n/index.js";
  import { setTheme, type Theme } from "$lib/theme/index.js";
  import PermissionGuide from "./PermissionGuide.svelte";
  import DiagnosticInfo from "./DiagnosticInfo.svelte";
  import SelfCheck from "./SelfCheck.svelte";

  interface Props {
    onRunWizard?: () => void;
  }

  let { onRunWizard }: Props = $props();

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
    auto_submit_enabled: boolean;
    auto_submit_key: string;
    auto_submit_delay_ms: number;
    streaming_enabled: boolean;
    dictation_route: string;
    streaming_endpoint_ms: number;
    simulated_streaming_enabled: boolean;
    edit_buffer_enabled: boolean;
    custom_words: string[];
    private_mode_enabled: boolean;
    mute_system_audio: boolean;
    engine_type: string;
    cloud_provider: string | null;
    cloud_opt_in_confirmed: boolean;
    cloud_fallback_local: boolean;
    openai_model: string;
    groq_model: string;
    deepgram_model: string;
    typing_baseline_wpm: number | null;
    theme: string;
    wizard_completed: boolean;
    update_behavior: string;
  }

  interface AudioDevice {
    name: string;
    is_default: boolean;
  }

  interface Profile {
    id: number;
    name: string;
    app_identifier: string;
    app_identifier_type: string;
    created_at: string;
  }

  type SettingsSection = "dictation" | "engine" | "microphone" | "feedback" | "theme" | "history" | "cloud" | "profiles" | "vocabulary" | "advanced" | "diagnostics" | "permissions";
  let activeSection: SettingsSection = $state("dictation");

  const sectionNav: { id: SettingsSection; labelKey: StringKeys }[] = [
    { id: "dictation", labelKey: "settings.section_dictation" },
    { id: "cloud", labelKey: "settings.section_cloud" },
    { id: "engine", labelKey: "settings.section_engine" },
    { id: "microphone", labelKey: "settings.section_microphone" },
    { id: "feedback", labelKey: "settings.section_feedback" },
    { id: "theme", labelKey: "settings.theme" },
    { id: "history", labelKey: "settings.section_history" },
    { id: "profiles", labelKey: "settings.section_profiles" },
    { id: "vocabulary", labelKey: "settings.section_vocabulary" },
    { id: "advanced", labelKey: "settings.section_advanced" },
    { id: "permissions", labelKey: "permissions.title" },
    { id: "diagnostics", labelKey: "settings.diagnostics" },
  ];

  let settings: AllSettings | null = $state(null);
  let audioDevices: AudioDevice[] = $state([]);
  let profiles: Profile[] = $state([]);
  let profileSettingsCounts: Record<number, number> = $state({});
  let editingProfileId: number | null = $state(null);
  let showAddProfile = $state(false);
  let profileForm = $state({ name: "", app_identifier: "", app_identifier_type: "bundle_id" });

  let customWords: string[] = $state([]);
  let newCustomWord = $state("");

  interface CloudProviderInfo {
    id: string;
    name: string;
    has_key: boolean;
    masked_last4: string | null;
  }

  let cloudProviders: CloudProviderInfo[] = $state([]);
  let cloudKeyInput: Record<string, string> = $state({});
  let cloudKeyEditing: string | null = $state(null);
  let cloudKeyTesting: string | null = $state(null);
  let cloudKeyValid: Record<string, boolean | null> = $state({});

  /** Available models per cloud provider. */
  const cloudModels: Record<string, { id: string; label: string }[]> = {
    groq: [
      { id: "whisper-large-v3", label: "Whisper Large V3" },
      { id: "whisper-large-v3-turbo", label: "Whisper Large V3 Turbo" },
    ],
    openai: [
      { id: "whisper-1", label: "Whisper 1" },
      { id: "gpt-4o-transcribe", label: "GPT-4o Transcribe" },
      { id: "gpt-4o-mini-transcribe", label: "GPT-4o Mini Transcribe" },
    ],
    deepgram: [
      { id: "nova-2", label: "Nova 2" },
      { id: "nova-3", label: "Nova 3" },
      { id: "enhanced", label: "Enhanced" },
      { id: "base", label: "Base" },
    ],
  };

  /** Get the setting key name for a provider's model. */
  function modelSettingKey(providerId: string): string {
    return `${providerId}_model`;
  }

  /** Get the current model for a provider from settings. */
  function currentModel(providerId: string): string {
    if (!settings) return "";
    const key = modelSettingKey(providerId) as keyof AllSettings;
    return (settings[key] as string) ?? "";
  }

  /** Change cloud model and re-activate if this provider is active. */
  async function changeCloudModel(providerId: string, model: string) {
    if (!settings) return;
    const key = modelSettingKey(providerId);
    await saveSetting(key, model);
    // Update local settings state
    const sKey = key as keyof AllSettings;
    (settings as unknown as Record<string, unknown>)[sKey] = model;
    // If this provider is currently active, re-activate to apply the new model
    if (settings.engine_type === "cloud" && settings.cloud_provider === providerId) {
      engineSwitching = true;
      try {
        await invoke("activate_cloud_engine", { provider: providerId });
      } catch (e) {
        console.error("Failed to re-activate cloud engine:", e);
      } finally {
        engineSwitching = false;
      }
    }
  }

  let showCloudOptIn = $state(false);
  let cloudOptInProvider: string | null = $state(null);
  let cloudOptInProviderName: string = $state("");
  let engineSwitching = $state(false);

  // Hotkey recording state
  let hotkeyRecording = $state(false);
  let hotkeyPending = $state("");
  let hotkeyError = $state("");

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

  /** Convert a keyboard event into a Tauri global-shortcut string. */
  function keyEventToShortcut(e: KeyboardEvent): string | null {
    // Ignore bare modifier presses
    if (["Control", "Shift", "Alt", "Meta"].includes(e.key)) return null;

    const parts: string[] = [];
    if (e.ctrlKey) parts.push("ctrl");
    if (e.altKey) parts.push("alt");
    if (e.shiftKey) parts.push("shift");
    if (e.metaKey) parts.push("super");

    // Must have at least one modifier
    if (parts.length === 0) return null;

    // Map special key names to Tauri format
    const keyMap: Record<string, string> = {
      " ": "space",
      ArrowUp: "up",
      ArrowDown: "down",
      ArrowLeft: "left",
      ArrowRight: "right",
      Enter: "enter",
      Backspace: "backspace",
      Delete: "delete",
      Escape: "escape",
      Tab: "tab",
      Home: "home",
      End: "end",
      PageUp: "pageup",
      PageDown: "pagedown",
      Insert: "insert",
    };

    let key = keyMap[e.key] ?? e.key.toLowerCase();
    // Function keys: F1..F24
    if (/^f\d{1,2}$/i.test(e.key)) {
      key = e.key.toUpperCase();
    }

    parts.push(key);
    return parts.join("+");
  }

  function startHotkeyRecording() {
    hotkeyRecording = true;
    hotkeyPending = "";
    hotkeyError = "";
  }

  function cancelHotkeyRecording() {
    hotkeyRecording = false;
    hotkeyPending = "";
    hotkeyError = "";
  }

  function handleHotkeyKeydown(e: KeyboardEvent) {
    if (!hotkeyRecording) return;
    e.preventDefault();
    e.stopPropagation();

    // Escape cancels recording
    if (e.key === "Escape") {
      cancelHotkeyRecording();
      return;
    }

    const shortcut = keyEventToShortcut(e);
    if (shortcut) {
      hotkeyPending = shortcut;
      saveHotkey(shortcut);
    }
  }

  async function saveHotkey(shortcut: string) {
    try {
      await invoke("change_hotkey", { shortcut });
      if (settings) {
        settings.hotkey = shortcut;
      }
      hotkeyRecording = false;
      hotkeyPending = "";
      hotkeyError = "";
      successMessage = t("hotkey.changed");
      setTimeout(() => (successMessage = ""), 2000);
    } catch (e) {
      hotkeyError = String(e);
      hotkeyPending = "";
    }
  }

  async function resetHotkey() {
    const defaultHotkey =
      navigator.platform.toLowerCase().includes("mac")
        ? "cmd+shift+space"
        : "ctrl+shift+space";
    await saveHotkey(defaultHotkey);
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

  async function loadProfiles() {
    try {
      profiles = await invoke<Profile[]>("list_profiles");
      const counts: Record<number, number> = {};
      for (const p of profiles) {
        const s = await invoke<[string, string][]>("get_profile_settings", { profileId: p.id });
        counts[p.id] = s.length;
      }
      profileSettingsCounts = counts;
    } catch {
      profiles = [];
    }
  }

  async function handleAddProfile() {
    if (!profileForm.name.trim() || !profileForm.app_identifier.trim()) return;
    try {
      await invoke("create_profile", {
        name: profileForm.name,
        appIdentifier: profileForm.app_identifier,
        appIdentifierType: profileForm.app_identifier_type,
      });
      profileForm = { name: "", app_identifier: "", app_identifier_type: "bundle_id" };
      showAddProfile = false;
      await loadProfiles();
      successMessage = t("settings.saved");
      setTimeout(() => (successMessage = ""), 2000);
    } catch (e) {
      errorMessage = String(e);
    }
  }

  async function handleUpdateProfile(id: number) {
    try {
      await invoke("update_profile", {
        id,
        name: profileForm.name,
        appIdentifier: profileForm.app_identifier,
        appIdentifierType: profileForm.app_identifier_type,
      });
      editingProfileId = null;
      profileForm = { name: "", app_identifier: "", app_identifier_type: "bundle_id" };
      await loadProfiles();
      successMessage = t("settings.saved");
      setTimeout(() => (successMessage = ""), 2000);
    } catch (e) {
      errorMessage = String(e);
    }
  }

  async function handleDeleteProfile(id: number) {
    try {
      await invoke("delete_profile", { id });
      await loadProfiles();
    } catch (e) {
      errorMessage = String(e);
    }
  }

  function startEditProfile(profile: Profile) {
    editingProfileId = profile.id;
    profileForm = {
      name: profile.name,
      app_identifier: profile.app_identifier,
      app_identifier_type: profile.app_identifier_type,
    };
    showAddProfile = false;
  }

  function cancelProfileEdit() {
    editingProfileId = null;
    showAddProfile = false;
    profileForm = { name: "", app_identifier: "", app_identifier_type: "bundle_id" };
  }

  async function loadCustomWords() {
    try {
      customWords = await invoke<string[]>("get_custom_words");
    } catch {
      customWords = [];
    }
  }

  async function handleAddCustomWord() {
    if (!newCustomWord.trim()) return;
    try {
      customWords = await invoke<string[]>("add_custom_word", { word: newCustomWord.trim() });
      newCustomWord = "";
    } catch (e) {
      errorMessage = String(e);
    }
  }

  async function handleRemoveCustomWord(word: string) {
    try {
      customWords = await invoke<string[]>("remove_custom_word", { word });
    } catch (e) {
      errorMessage = String(e);
    }
  }

  async function loadCloudProviders() {
    try {
      cloudProviders = await invoke<CloudProviderInfo[]>("list_cloud_providers");
    } catch (e) {
      console.error("Failed to load cloud providers:", e);
    }
  }

  function startEditCloudKey(providerId: string) {
    cloudKeyEditing = providerId;
    cloudKeyInput[providerId] = "";
    cloudKeyValid[providerId] = null;
  }

  async function saveCloudKey(providerId: string) {
    const key = cloudKeyInput[providerId]?.trim();
    if (!key) return;
    try {
      await invoke("set_api_key", { provider: providerId, key });
      cloudKeyEditing = null;
      cloudKeyInput[providerId] = "";
      await loadCloudProviders();
      successMessage = t("settings.saved");
      setTimeout(() => (successMessage = ""), 2000);
    } catch (e) {
      errorMessage = String(e);
    }
  }

  async function removeCloudKey(providerId: string) {
    try {
      await invoke("delete_api_key", { provider: providerId });
      cloudKeyValid[providerId] = null;
      await loadCloudProviders();
      successMessage = t("settings.saved");
      setTimeout(() => (successMessage = ""), 2000);
    } catch (e) {
      errorMessage = String(e);
    }
  }

  async function testCloudKey(providerId: string) {
    cloudKeyTesting = providerId;
    cloudKeyValid[providerId] = null;
    try {
      const valid = await invoke<boolean>("validate_api_key", { provider: providerId });
      cloudKeyValid[providerId] = valid;
    } catch (e) {
      cloudKeyValid[providerId] = false;
      errorMessage = String(e);
    } finally {
      cloudKeyTesting = null;
    }
  }

  function requestActivateCloud(providerId: string) {
    const provider = cloudProviders.find((p) => p.id === providerId);
    if (!provider) return;
    if (settings?.cloud_opt_in_confirmed) {
      doActivateCloud(providerId);
    } else {
      cloudOptInProvider = providerId;
      cloudOptInProviderName = provider.name;
      showCloudOptIn = true;
    }
  }

  async function confirmCloudOptIn() {
    if (!cloudOptInProvider) return;
    try {
      await saveSetting("cloud_opt_in_confirmed", true);
      if (settings) settings.cloud_opt_in_confirmed = true;
      showCloudOptIn = false;
      doActivateCloud(cloudOptInProvider);
      cloudOptInProvider = null;
      cloudOptInProviderName = "";
    } catch (e) {
      errorMessage = String(e);
    }
  }

  function cancelCloudOptIn() {
    showCloudOptIn = false;
    cloudOptInProvider = null;
    cloudOptInProviderName = "";
  }

  async function doActivateCloud(providerId: string) {
    engineSwitching = true;
    try {
      await invoke("activate_cloud_engine", { provider: providerId });
      if (settings) {
        settings.engine_type = "cloud";
        settings.cloud_provider = providerId;
      }
      successMessage = t("settings.saved");
      setTimeout(() => (successMessage = ""), 2000);
    } catch (e) {
      errorMessage = String(e);
    } finally {
      engineSwitching = false;
    }
  }

  async function switchToLocal() {
    engineSwitching = true;
    try {
      await invoke("activate_local_engine");
      if (settings) {
        settings.engine_type = "local";
        settings.cloud_provider = null;
      }
      successMessage = t("settings.saved");
      setTimeout(() => (successMessage = ""), 2000);
    } catch (e) {
      errorMessage = String(e);
    } finally {
      engineSwitching = false;
    }
  }

  $effect(() => {
    loadSettings();
    loadAudioDevices();
    loadProfiles();
    loadCustomWords();
    loadCloudProviders();
  });
</script>

<div class="flex h-full">
  <!-- Left sidebar nav -->
  <nav class="w-44 shrink-0 border-r border-border overflow-y-auto py-4 px-2">
    {#each sectionNav as sec (sec.id)}
      <button
        class="w-full rounded px-3 py-1.5 text-left text-xs transition-colors {activeSection === sec.id
          ? 'bg-accent/15 text-accent font-medium'
          : 'text-text-secondary hover:bg-bg-secondary hover:text-text-primary'}"
        onclick={() => (activeSection = sec.id)}
      >
        {t(sec.labelKey)}
      </button>
    {/each}
  </nav>

  <!-- Main content -->
  <div class="flex-1 overflow-y-auto px-6 py-6">
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
        aria-live="polite"
      >
        {successMessage}
      </p>
    {/if}

    {#if activeSection === "permissions"}
      <PermissionGuide />
    {/if}

    {#if settings}
      {#if activeSection === "dictation"}
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

        <div class="flex flex-col gap-1">
          <div class="flex items-center justify-between">
            <label for="hotkey" class="text-sm">{t("settings.hotkey")}</label>
            {#if hotkeyRecording}
              <div class="flex items-center gap-2">
                <span
                  id="hotkey"
                  role="button"
                  tabindex="0"
                  class="rounded border-2 border-accent bg-bg-primary px-3 py-1 text-sm font-mono animate-pulse cursor-pointer"
                  onkeydown={handleHotkeyKeydown}
                >
                  {hotkeyPending || t("hotkey.recording")}
                </span>
                <button
                  type="button"
                  class="text-xs text-text-muted hover:text-text-primary"
                  onclick={cancelHotkeyRecording}
                >
                  {t("hotkey.cancel")}
                </button>
              </div>
            {:else}
              <div class="flex items-center gap-2">
                <button
                  type="button"
                  id="hotkey"
                  class="rounded border border-border bg-bg-primary px-3 py-1 text-sm font-mono hover:border-accent cursor-pointer transition-colors"
                  onclick={startHotkeyRecording}
                  title={t("hotkey.click_to_change")}
                >
                  {settings.hotkey}
                </button>
                {#if settings.hotkey.toLowerCase() !== (navigator.platform.toLowerCase().includes("mac") ? "cmd+shift+space" : "ctrl+shift+space")}
                  <button
                    type="button"
                    class="text-xs text-text-muted hover:text-text-primary"
                    onclick={resetHotkey}
                  >
                    {t("hotkey.reset")}
                  </button>
                {/if}
              </div>
            {/if}
          </div>
          {#if hotkeyError}
            <p class="text-xs text-red-500">{hotkeyError}</p>
          {/if}
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
            <option value="en">{t("settings.lang_english")}</option>
            <option value="auto">{t("settings.lang_auto")}</option>
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

        <div class="flex items-center justify-between">
          <label for="auto-submit" class="text-sm"
            >{t("settings.auto_submit")}</label
          >
          <input
            id="auto-submit"
            type="checkbox"
            checked={settings.auto_submit_enabled}
            onchange={(e) =>
              saveSetting(
                "auto_submit_enabled",
                (e.target as HTMLInputElement).checked,
              )}
            class="h-4 w-4 rounded accent-accent"
          />
        </div>

        {#if settings.auto_submit_enabled}
          <div class="flex items-center justify-between">
            <label for="submit-key" class="text-sm"
              >{t("settings.auto_submit_key")}</label
            >
            <select
              id="submit-key"
              value={settings.auto_submit_key}
              onchange={(e) =>
                saveSetting(
                  "auto_submit_key",
                  (e.target as HTMLSelectElement).value,
                )}
              class="rounded border border-border bg-bg-primary px-3 py-1 text-sm"
            >
              <option value="enter">{t("settings.submit_enter")}</option>
              <option value="ctrl_enter"
                >{t("settings.submit_ctrl_enter")}</option
              >
              <option value="cmd_enter"
                >{t("settings.submit_cmd_enter")}</option
              >
            </select>
          </div>

          <div class="flex items-center justify-between">
            <label for="submit-delay" class="text-sm"
              >{t("settings.auto_submit_delay")}</label
            >
            <input
              id="submit-delay"
              type="number"
              min="50"
              max="500"
              step="50"
              value={settings.auto_submit_delay_ms}
              onchange={(e) =>
                saveSetting(
                  "auto_submit_delay_ms",
                  parseInt((e.target as HTMLInputElement).value) || 100,
                )}
              class="w-24 rounded border border-border bg-bg-primary px-3 py-1 text-sm"
            />
          </div>
        {/if}

        <div class="flex items-center justify-between">
          <label for="streaming" class="text-sm"
            >{t("settings.streaming_preview")}</label
          >
          <input
            id="streaming"
            type="checkbox"
            checked={settings.streaming_enabled}
            onchange={(e) =>
              saveSetting(
                "streaming_enabled",
                (e.target as HTMLInputElement).checked,
              )}
            class="h-4 w-4 rounded accent-accent"
          />
        </div>

        {#if settings.engine_type === "cloud" && (settings.cloud_provider === "deepgram" || settings.cloud_provider === "openai") && settings.dictation_route !== "classic"}
          <div class="flex items-center justify-between">
            <div>
              <label for="pause-sensitivity" class="text-sm">{t("settings.pause_sensitivity")}</label>
              <p class="text-xs text-text-muted">{t("settings.pause_sensitivity_hint")}</p>
            </div>
            <select
              id="pause-sensitivity"
              value={settings.streaming_endpoint_ms}
              onchange={(e) =>
                saveSetting(
                  "streaming_endpoint_ms",
                  parseInt((e.target as HTMLSelectElement).value, 10),
                )}
              class="rounded border border-border bg-bg-primary px-3 py-1 text-sm"
            >
              <option value={500}>{t("settings.pause_short")}</option>
              <option value={1500}>{t("settings.pause_medium")}</option>
              <option value={3000}>{t("settings.pause_long")}</option>
              <option value={5000}>{t("settings.pause_very_long")}</option>
            </select>
          </div>
        {/if}

        <div class="flex items-center justify-between">
          <label for="edit-buffer" class="text-sm"
            >{t("settings.edit_buffer")}</label
          >
          <input
            id="edit-buffer"
            type="checkbox"
            checked={settings.edit_buffer_enabled}
            onchange={(e) =>
              saveSetting(
                "edit_buffer_enabled",
                (e.target as HTMLInputElement).checked,
              )}
            class="h-4 w-4 rounded accent-accent"
          />
        </div>
      </div>
    </section>
    {/if}

    {#if activeSection === "engine"}
    <!-- Engine Section -->
    <section class="mb-8">
      <h3 class="mb-4 text-sm font-medium uppercase tracking-wide text-text-secondary">
        {t("settings.section_engine")}
      </h3>
      <div class="space-y-4">
        <div class="flex items-center justify-between">
          <span class="text-sm">{t("cloud.engine_type")}</span>
          <div class="flex items-center gap-2">
            <button
              class="rounded px-3 py-1 text-sm transition-colors {settings.engine_type === 'local' ? 'bg-accent text-white' : 'border border-border bg-bg-primary text-text-secondary hover:bg-bg-secondary'}"
              disabled={engineSwitching || settings.engine_type === "local"}
              onclick={() => switchToLocal()}
            >
              {t("cloud.engine_local")}
            </button>
            <button
              class="rounded px-3 py-1 text-sm transition-colors {settings.engine_type === 'cloud' ? 'bg-accent text-white' : 'border border-border bg-bg-primary text-text-secondary hover:bg-bg-secondary'}"
              disabled={engineSwitching || settings.engine_type === "cloud"}
              onclick={() => {
                const available = cloudProviders.filter((p) => p.has_key);
                if (available.length > 0) {
                  requestActivateCloud(available[0].id);
                }
              }}
              title={cloudProviders.filter((p) => p.has_key).length === 0 ? t("cloud.no_keys_hint") : ""}
            >
              {t("cloud.engine_cloud")}
            </button>
          </div>
        </div>

        {#if settings.engine_type === "cloud" && settings.cloud_provider}
          <div class="flex items-center justify-between">
            <span class="text-sm">{t("settings.cloud_provider")}</span>
            <span class="text-sm text-text-secondary">
              {cloudProviders.find((p) => p.id === settings?.cloud_provider)?.name ?? settings.cloud_provider}
            </span>
          </div>
        {/if}

        {#if cloudProviders.filter((p) => p.has_key).length === 0}
          <p class="text-xs text-text-muted">
            {t("settings.configure_cloud_hint")}
            <button
              class="ml-1 text-accent hover:text-accent-hover"
              onclick={() => (activeSection = "cloud")}
            >Cloud Providers</button>
          </p>
        {/if}

        {#if engineSwitching}
          <p class="text-xs text-text-secondary">{t("cloud.activating")}</p>
        {/if}

        {#if settings.engine_type === "cloud" && settings.cloud_provider}
          <div class="flex items-center justify-between">
            <span class="text-sm">{t("settings.active_cloud_model")}</span>
            <span class="text-sm text-text-secondary">
              {cloudProviders.find((p) => p.id === settings?.cloud_provider)?.name ?? settings.cloud_provider}
              / {currentModel(settings.cloud_provider)}
            </span>
          </div>
        {/if}

        {#if settings.engine_type === "cloud" && (settings.cloud_provider === "deepgram" || settings.cloud_provider === "openai")}
          <div class="flex items-center justify-between">
            <div>
              <label for="dictation-route" class="text-sm">{t("settings.streaming_mode")}</label>
              <p class="text-xs text-text-muted">{t("settings.streaming_mode_hint")}</p>
            </div>
            <select
              id="dictation-route"
              value={settings.dictation_route}
              onchange={(e) =>
                saveSetting(
                  "dictation_route",
                  (e.target as HTMLSelectElement).value,
                )}
              class="rounded border border-border bg-bg-primary px-3 py-1 text-sm"
            >
              <option value="auto">{t("settings.route_auto")}</option>
              <option value="streaming">{t("settings.route_streaming")}</option>
              <option value="classic">{t("settings.route_classic")}</option>
            </select>
          </div>

        {/if}

        {#if settings.engine_type === "local" || (settings.engine_type === "cloud" && settings.cloud_provider === "groq")}
          <div class="flex items-center justify-between">
            <div>
              <label for="simulated-streaming" class="text-sm"
                >{t("settings.simulated_streaming")}</label
              >
              <p class="text-xs text-text-muted"
                >{t("settings.simulated_streaming_hint")}</p
              >
            </div>
            <input
              id="simulated-streaming"
              type="checkbox"
              checked={settings.simulated_streaming_enabled}
              onchange={(e) =>
                saveSetting(
                  "simulated_streaming_enabled",
                  (e.target as HTMLInputElement).checked,
                )}
              class="h-4 w-4 rounded accent-accent"
            />
          </div>
        {/if}

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
    {/if}

    {#if activeSection === "microphone"}
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

        <div class="flex items-center justify-between">
          <label for="mute-system-audio" class="text-sm"
            >{t("settings.mute_system_audio")}</label
          >
          <input
            id="mute-system-audio"
            type="checkbox"
            checked={settings.mute_system_audio}
            onchange={(e) =>
              saveSetting(
                "mute_system_audio",
                (e.target as HTMLInputElement).checked,
              )}
            class="h-4 w-4 rounded accent-accent"
          />
        </div>
      </div>
    </section>

    {/if}

    {#if activeSection === "feedback"}
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

    {/if}

    {#if activeSection === "theme"}
    <!-- Appearance Section -->
    <section class="mb-8">
      <h3 class="mb-4 text-sm font-medium uppercase tracking-wide text-text-secondary">
        {t("settings.theme")}
      </h3>
      <div class="space-y-4">
        <div class="flex items-center justify-between">
          <label for="theme-select" class="text-sm">{t("settings.theme")}</label>
          <select
            id="theme-select"
            value={settings.theme}
            onchange={(e) => {
              const val = (e.target as HTMLSelectElement).value;
              saveSetting("theme", val);
              setTheme(val as Theme);
            }}
            class="rounded border border-border bg-bg-surface px-3 py-1 text-sm"
          >
            <option value="dark">{t("settings.theme_dark")}</option>
            <option value="light">{t("settings.theme_light")}</option>
            <option value="high-contrast">{t("settings.theme_high_contrast")}</option>
            <option value="auto">{t("settings.theme_auto")}</option>
          </select>
        </div>
      </div>
    </section>

    {/if}

    {#if activeSection === "history"}
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

        <div class="flex items-center justify-between">
          <div>
            <label for="private-mode" class="text-sm"
              >{t("settings.private_mode")}</label
            >
            <p class="text-xs text-text-secondary">{t("settings.private_mode_desc")}</p>
          </div>
          <input
            id="private-mode"
            type="checkbox"
            checked={settings.private_mode_enabled}
            onchange={(e) =>
              saveSetting(
                "private_mode_enabled",
                (e.target as HTMLInputElement).checked,
              )}
            class="h-4 w-4 rounded accent-accent"
          />
        </div>
      </div>
    </section>

    {/if}

    {#if activeSection === "cloud"}
    <!-- Cloud Providers Section -->
    <section class="mb-8">
      <h3 class="mb-4 text-sm font-medium uppercase tracking-wide text-text-secondary">
        {t("settings.section_cloud")}
      </h3>
      <p class="mb-3 text-xs text-text-secondary">{t("cloud.description")}</p>

      <div class="space-y-3">
        {#each cloudProviders as provider (provider.id)}
          <div class="rounded border {settings.engine_type === 'cloud' && settings.cloud_provider === provider.id ? 'border-accent' : 'border-border'} p-3">
            <div class="flex items-center justify-between">
              <div>
                <span class="text-sm font-medium">{provider.name}</span>
                {#if settings.engine_type === "cloud" && settings.cloud_provider === provider.id}
                  <span class="ml-2 rounded-full bg-accent/20 px-2 py-0.5 text-xs text-accent">{t("cloud.active_badge")}</span>
                {/if}
                {#if provider.has_key}
                  <span class="ml-2 text-xs text-accent">{t("cloud.key_configured")}</span>
                  {#if provider.masked_last4}
                    <span class="ml-1 text-xs text-text-secondary">({provider.masked_last4})</span>
                  {/if}
                {:else}
                  <span class="ml-2 text-xs text-text-secondary">{t("cloud.key_not_set")}</span>
                {/if}
              </div>
              <div class="flex gap-2">
                {#if provider.has_key}
                  {#if !(settings.engine_type === "cloud" && settings.cloud_provider === provider.id)}
                    <button
                      class="rounded bg-accent/10 px-2 py-1 text-xs text-accent hover:bg-accent/20"
                      disabled={engineSwitching}
                      onclick={() => requestActivateCloud(provider.id)}
                    >
                      {engineSwitching ? t("cloud.activating") : t("cloud.activate")}
                    </button>
                  {/if}
                  <button
                    class="rounded px-2 py-1 text-xs text-accent hover:bg-accent/10"
                    disabled={cloudKeyTesting === provider.id}
                    onclick={() => testCloudKey(provider.id)}
                  >
                    {cloudKeyTesting === provider.id ? t("cloud.testing") : t("cloud.test_key")}
                  </button>
                  <button
                    class="rounded px-2 py-1 text-xs text-status-recording hover:bg-status-recording/10"
                    onclick={() => removeCloudKey(provider.id)}
                  >
                    {t("cloud.remove_key")}
                  </button>
                {:else if cloudKeyEditing !== provider.id}
                  <button
                    class="rounded px-2 py-1 text-xs text-accent hover:bg-accent/10"
                    onclick={() => startEditCloudKey(provider.id)}
                  >
                    {t("cloud.add_key")}
                  </button>
                {/if}
              </div>
            </div>

            {#if cloudKeyValid[provider.id] === true}
              <p class="mt-1 text-xs text-accent">{t("cloud.key_valid")}</p>
            {:else if cloudKeyValid[provider.id] === false}
              <p class="mt-1 text-xs text-status-recording">{t("cloud.key_invalid")}</p>
            {/if}

            {#if provider.has_key && cloudModels[provider.id]}
              <div class="mt-2 flex items-center gap-2">
                <label for="model-{provider.id}" class="text-xs text-text-secondary">Model:</label>
                <select
                  id="model-{provider.id}"
                  class="rounded border border-border bg-bg-primary px-2 py-1 text-xs"
                  value={currentModel(provider.id)}
                  onchange={(e) => changeCloudModel(provider.id, (e.target as HTMLSelectElement).value)}
                  disabled={engineSwitching}
                >
                  {#each cloudModels[provider.id] as model (model.id)}
                    <option value={model.id}>{model.label}</option>
                  {/each}
                </select>
                {#if settings?.engine_type === "cloud" && settings?.cloud_provider === provider.id}
                  <span class="text-[10px] text-text-muted">(active)</span>
                {/if}
              </div>
            {/if}

            {#if cloudKeyEditing === provider.id}
              <div class="mt-2 flex gap-2">
                <input
                  type="password"
                  placeholder={t("cloud.key_placeholder")}
                  aria-label={`${provider.name} API key`}
                  bind:value={cloudKeyInput[provider.id]}
                  class="flex-1 rounded border border-border bg-bg-primary px-2 py-1 text-sm"
                />
                <button
                  class="rounded bg-accent px-3 py-1 text-xs text-white hover:bg-accent/80"
                  onclick={() => saveCloudKey(provider.id)}
                >
                  {t("cloud.save")}
                </button>
                <button
                  class="rounded px-2 py-1 text-xs text-text-secondary hover:bg-bg-secondary"
                  onclick={() => (cloudKeyEditing = null)}
                >
                  {t("cloud.cancel")}
                </button>
              </div>
            {/if}
          </div>
        {/each}
      </div>

      {#if settings}
        <div class="mt-4 space-y-4">
          <div class="flex items-center justify-between">
            <label for="cloud-fallback" class="text-sm"
              >{t("settings.cloud_fallback_local")}</label
            >
            <input
              id="cloud-fallback"
              type="checkbox"
              checked={settings.cloud_fallback_local}
              onchange={(e) =>
                saveSetting(
                  "cloud_fallback_local",
                  (e.target as HTMLInputElement).checked,
                )}
              class="h-4 w-4 rounded accent-accent"
            />
          </div>
        </div>
      {/if}
    </section>

    {/if}

    {#if activeSection === "profiles"}
    <!-- Profiles Section -->
    <section class="mb-8">
      <h3 class="mb-4 text-sm font-medium uppercase tracking-wide text-text-secondary">
        {t("settings.section_profiles")}
      </h3>
      <p class="mb-3 text-xs text-text-secondary">{t("profiles.description")}</p>

      {#if profiles.length === 0 && !showAddProfile}
        <p class="mb-3 text-sm text-text-secondary">{t("profiles.empty")}</p>
      {/if}

      <div class="space-y-2 mb-3">
        {#each profiles as profile (profile.id)}
          {#if editingProfileId === profile.id}
            <div class="rounded border border-accent bg-bg-surface p-3 space-y-2">
              <input
                type="text"
                bind:value={profileForm.name}
                placeholder={t("profiles.name")}
                aria-label={t("profiles.name")}
                class="w-full rounded border border-border bg-bg-primary px-3 py-1 text-sm"
              />
              <input
                type="text"
                bind:value={profileForm.app_identifier}
                placeholder={t("profiles.app_id")}
                aria-label={t("profiles.app_id")}
                class="w-full rounded border border-border bg-bg-primary px-3 py-1 text-sm"
              />
              <select
                bind:value={profileForm.app_identifier_type}
                aria-label={t("profiles.app_id_type")}
                class="w-full rounded border border-border bg-bg-primary px-3 py-1 text-sm"
              >
                <option value="bundle_id">{t("profiles.type_bundle_id")}</option>
                <option value="exe_path">{t("profiles.type_exe_path")}</option>
                <option value="wm_class">{t("profiles.type_wm_class")}</option>
              </select>
              <div class="flex gap-2">
                <button
                  onclick={() => handleUpdateProfile(profile.id)}
                  class="rounded bg-accent px-3 py-1 text-sm text-white hover:bg-accent/90"
                >
                  {t("profiles.save")}
                </button>
                <button
                  onclick={cancelProfileEdit}
                  class="rounded border border-border bg-bg-primary px-3 py-1 text-sm hover:bg-bg-surface"
                >
                  {t("profiles.cancel")}
                </button>
              </div>
            </div>
          {:else}
            <div class="flex items-center justify-between rounded border border-border bg-bg-surface p-3">
              <div>
                <span class="text-sm font-medium">{profile.name}</span>
                <span class="ml-2 text-xs text-text-secondary">{profile.app_identifier}</span>
                {#if profileSettingsCounts[profile.id]}
                  <span class="ml-2 text-xs text-accent">
                    {t("profiles.settings_count").replace("{count}", String(profileSettingsCounts[profile.id]))}
                  </span>
                {/if}
              </div>
              <div class="flex gap-1">
                <button
                  onclick={() => startEditProfile(profile)}
                  class="rounded px-2 py-1 text-xs text-text-secondary hover:bg-bg-primary"
                >
                  {t("profiles.edit")}
                </button>
                <button
                  onclick={() => handleDeleteProfile(profile.id)}
                  class="rounded px-2 py-1 text-xs text-status-recording hover:bg-bg-primary"
                >
                  {t("profiles.delete")}
                </button>
              </div>
            </div>
          {/if}
        {/each}
      </div>

      {#if showAddProfile}
        <div class="rounded border border-accent bg-bg-surface p-3 space-y-2">
          <input
            type="text"
            bind:value={profileForm.name}
            placeholder={t("profiles.name")}
            aria-label={t("profiles.name")}
            class="w-full rounded border border-border bg-bg-primary px-3 py-1 text-sm"
          />
          <input
            type="text"
            bind:value={profileForm.app_identifier}
            placeholder={t("profiles.app_id")}
            aria-label={t("profiles.app_id")}
            class="w-full rounded border border-border bg-bg-primary px-3 py-1 text-sm"
          />
          <select
            bind:value={profileForm.app_identifier_type}
            aria-label={t("profiles.app_id_type")}
            class="w-full rounded border border-border bg-bg-primary px-3 py-1 text-sm"
          >
            <option value="bundle_id">{t("profiles.type_bundle_id")}</option>
            <option value="exe_path">{t("profiles.type_exe_path")}</option>
            <option value="wm_class">{t("profiles.type_wm_class")}</option>
          </select>
          <div class="flex gap-2">
            <button
              onclick={handleAddProfile}
              class="rounded bg-accent px-3 py-1 text-sm text-white hover:bg-accent/90"
            >
              {t("profiles.save")}
            </button>
            <button
              onclick={cancelProfileEdit}
              class="rounded border border-border bg-bg-primary px-3 py-1 text-sm hover:bg-bg-surface"
            >
              {t("profiles.cancel")}
            </button>
          </div>
        </div>
      {:else if editingProfileId === null}
        <button
          onclick={() => { showAddProfile = true; profileForm = { name: "", app_identifier: "", app_identifier_type: "bundle_id" }; }}
          class="rounded border border-border bg-bg-primary px-3 py-1.5 text-sm text-text-primary hover:bg-bg-surface focus:outline-none focus:ring-2 focus:ring-accent"
        >
          {t("profiles.add")}
        </button>
      {/if}
    </section>

    {/if}

    {#if activeSection === "vocabulary"}
    <!-- Custom Words Section -->
    <section class="mb-8">
      <h3 class="mb-4 text-sm font-medium uppercase tracking-wide text-text-secondary">
        {t("settings.section_vocabulary")}
      </h3>
      <p class="mb-3 text-xs text-text-secondary">{t("vocabulary.description")}</p>

      <div class="mb-4 flex gap-2">
        <input
          type="text"
          bind:value={newCustomWord}
          placeholder={t("vocabulary.add_placeholder")}
          aria-label={t("vocabulary.add_placeholder")}
          class="flex-1 rounded border border-border bg-bg-primary px-3 py-1.5 text-sm"
          onkeydown={(e: KeyboardEvent) => { if (e.key === "Enter") handleAddCustomWord(); }}
        />
        <button
          onclick={handleAddCustomWord}
          disabled={!newCustomWord.trim()}
          class="rounded bg-accent px-4 py-1.5 text-sm text-white hover:bg-accent-hover focus:outline-none focus:ring-2 focus:ring-accent disabled:opacity-50"
        >
          {t("vocabulary.add")}
        </button>
      </div>

      {#if customWords.length === 0}
        <p class="text-sm text-text-secondary">{t("vocabulary.empty")}</p>
      {:else}
        <div class="flex flex-wrap gap-2">
          {#each customWords as word (word)}
            <span class="inline-flex items-center gap-1 rounded-full border border-border bg-bg-surface px-3 py-1 text-sm">
              {word}
              <button
                onclick={() => handleRemoveCustomWord(word)}
                class="ml-0.5 rounded-full p-0.5 text-text-secondary hover:text-status-recording"
                aria-label="{t('vocabulary.remove')} {word}"
              >
                <svg xmlns="http://www.w3.org/2000/svg" class="h-3.5 w-3.5" viewBox="0 0 20 20" fill="currentColor">
                  <path fill-rule="evenodd" d="M4.293 4.293a1 1 0 011.414 0L10 8.586l4.293-4.293a1 1 0 111.414 1.414L11.414 10l4.293 4.293a1 1 0 01-1.414 1.414L10 11.414l-4.293 4.293a1 1 0 01-1.414-1.414L8.586 10 4.293 5.707a1 1 0 010-1.414z" clip-rule="evenodd" />
                </svg>
              </button>
            </span>
          {/each}
        </div>
        <p class="mt-3 text-xs text-text-secondary">
          {t("vocabulary.word_count").replace("{count}", String(customWords.length))}
        </p>
      {/if}
    </section>
    {/if}

    {#if activeSection === "advanced"}
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

        <div class="flex items-center justify-between">
          <label for="update-behavior" class="text-sm"
            >{t("settings.update_behavior")}</label
          >
          <select
            id="update-behavior"
            value={settings.update_behavior}
            onchange={(e) =>
              saveSetting("update_behavior", (e.target as HTMLSelectElement).value)}
            class="rounded border border-border bg-bg-primary px-3 py-1 text-sm"
          >
            <option value="auto_update">{t("settings.update_auto")}</option>
            <option value="download_and_prompt">{t("settings.update_prompt")}</option>
            <option value="notify_only">{t("settings.update_notify")}</option>
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

        {#if onRunWizard}
          <button
            onclick={onRunWizard}
            class="rounded bg-bg-primary px-3 py-1.5 text-sm text-text-primary border border-border hover:bg-bg-surface focus:outline-none focus:ring-2 focus:ring-accent"
          >
            {t("settings.run_wizard_again")}
          </button>
        {/if}
      </div>
    </section>

    {/if}

    {#if activeSection === "diagnostics"}
    <!-- Diagnostics Section -->
    <section class="mb-8">
      <h3 class="mb-4 text-sm font-medium uppercase tracking-wide text-text-secondary">
        {t("settings.diagnostics")}
      </h3>
      <p class="mb-3 text-xs text-text-muted">{t("diagnostic.description")}</p>
      <DiagnosticInfo />
    </section>

    <!-- Self-Checks Section -->
    <section class="mb-8">
      <h3 class="mb-4 text-sm font-medium uppercase tracking-wide text-text-secondary">
        {t("settings.self_checks")}
      </h3>
      <SelfCheck />
    </section>
    {/if}
  {/if}
  </div>
</div>

<!-- Cloud Opt-In Confirmation Modal -->
{#if showCloudOptIn}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
    role="dialog"
    aria-modal="true"
    aria-labelledby="cloud-optin-title"
  >
    <div class="mx-4 max-w-md rounded-lg border border-border bg-bg-primary p-6 shadow-xl">
      <h3 id="cloud-optin-title" class="mb-3 text-base font-semibold">
        {t("cloud.opt_in_title")}
      </h3>
      <p class="mb-5 text-sm text-text-secondary">
        {t("cloud.opt_in_body").replace("{provider}", cloudOptInProviderName)}
      </p>
      <div class="flex justify-end gap-3">
        <button
          class="rounded border border-border px-4 py-2 text-sm text-text-secondary hover:bg-bg-secondary"
          onclick={cancelCloudOptIn}
        >
          {t("cloud.opt_in_cancel")}
        </button>
        <button
          class="rounded bg-accent px-4 py-2 text-sm text-white hover:bg-accent/80"
          onclick={confirmCloudOptIn}
        >
          {t("cloud.opt_in_confirm")}
        </button>
      </div>
    </div>
  </div>
{/if}
