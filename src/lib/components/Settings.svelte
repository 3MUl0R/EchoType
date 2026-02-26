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
    auto_submit_enabled: boolean;
    auto_submit_key: string;
    auto_submit_delay_ms: number;
    streaming_enabled: boolean;
    edit_buffer_enabled: boolean;
    custom_vocabulary_id: number | null;
    private_mode_enabled: boolean;
    mute_system_audio: boolean;
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

  let settings: AllSettings | null = $state(null);
  let audioDevices: AudioDevice[] = $state([]);
  let profiles: Profile[] = $state([]);
  let profileSettingsCounts: Record<number, number> = $state({});
  let editingProfileId: number | null = $state(null);
  let showAddProfile = $state(false);
  let profileForm = $state({ name: "", app_identifier: "", app_identifier_type: "bundle_id" });

  interface VocabCollection {
    id: number;
    name: string;
    created_at: string;
  }

  interface VocabEntry {
    id: number;
    collection_id: number;
    correction: string;
    aliases: string[];
  }

  let vocabCollections: VocabCollection[] = $state([]);
  let vocabEntryCounts: Record<number, number> = $state({});
  let showAddCollection = $state(false);
  let newCollectionName = $state("");
  let managingCollectionId: number | null = $state(null);
  let vocabEntries: VocabEntry[] = $state([]);
  let showAddEntry = $state(false);
  let entryForm = $state({ correction: "", aliases: "" });
  let editingEntryId: number | null = $state(null);

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

  async function loadVocabCollections() {
    try {
      vocabCollections = await invoke<VocabCollection[]>("list_vocabulary_collections");
      const counts: Record<number, number> = {};
      for (const c of vocabCollections) {
        counts[c.id] = await invoke<number>("vocabulary_entry_count", { collectionId: c.id });
      }
      vocabEntryCounts = counts;
    } catch {
      vocabCollections = [];
    }
  }

  async function handleAddCollection() {
    if (!newCollectionName.trim()) return;
    try {
      await invoke("create_vocabulary_collection", { name: newCollectionName.trim() });
      newCollectionName = "";
      showAddCollection = false;
      await loadVocabCollections();
    } catch (e) {
      errorMessage = String(e);
    }
  }

  async function handleDeleteCollection(id: number) {
    try {
      await invoke("delete_vocabulary_collection", { id });
      if (settings?.custom_vocabulary_id === id) {
        await saveSetting("custom_vocabulary_id", null);
        if (settings) settings.custom_vocabulary_id = null;
      }
      await loadVocabCollections();
    } catch (e) {
      errorMessage = String(e);
    }
  }

  async function openCollectionManager(id: number) {
    managingCollectionId = id;
    try {
      vocabEntries = await invoke<VocabEntry[]>("list_vocabulary_entries", { collectionId: id });
    } catch (e) {
      errorMessage = String(e);
    }
  }

  function closeCollectionManager() {
    managingCollectionId = null;
    vocabEntries = [];
    showAddEntry = false;
    editingEntryId = null;
    entryForm = { correction: "", aliases: "" };
  }

  async function handleAddEntry() {
    if (!entryForm.correction.trim() || !entryForm.aliases.trim()) return;
    const aliases = entryForm.aliases.split(",").map((a) => a.trim()).filter((a) => a);
    try {
      await invoke("add_vocabulary_entry", {
        collectionId: managingCollectionId,
        correction: entryForm.correction.trim(),
        aliases,
      });
      entryForm = { correction: "", aliases: "" };
      showAddEntry = false;
      vocabEntries = await invoke<VocabEntry[]>("list_vocabulary_entries", {
        collectionId: managingCollectionId,
      });
      await loadVocabCollections();
    } catch (e) {
      errorMessage = String(e);
    }
  }

  async function handleUpdateEntry(id: number) {
    const aliases = entryForm.aliases.split(",").map((a) => a.trim()).filter((a) => a);
    try {
      await invoke("update_vocabulary_entry", {
        id,
        correction: entryForm.correction.trim(),
        aliases,
      });
      editingEntryId = null;
      entryForm = { correction: "", aliases: "" };
      vocabEntries = await invoke<VocabEntry[]>("list_vocabulary_entries", {
        collectionId: managingCollectionId,
      });
    } catch (e) {
      errorMessage = String(e);
    }
  }

  async function handleDeleteEntry(id: number) {
    try {
      await invoke("delete_vocabulary_entry", { id });
      vocabEntries = await invoke<VocabEntry[]>("list_vocabulary_entries", {
        collectionId: managingCollectionId,
      });
      await loadVocabCollections();
    } catch (e) {
      errorMessage = String(e);
    }
  }

  function startEditEntry(entry: VocabEntry) {
    editingEntryId = entry.id;
    entryForm = { correction: entry.correction, aliases: entry.aliases.join(", ") };
    showAddEntry = false;
  }

  async function handleExportVocab() {
    if (managingCollectionId === null) return;
    try {
      const entries = await invoke<VocabEntry[]>("list_vocabulary_entries", {
        collectionId: managingCollectionId,
      });
      const data = entries.map((e) => ({ correction: e.correction, aliases: e.aliases }));
      const blob = new Blob([JSON.stringify(data, null, 2)], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = "vocabulary.json";
      a.click();
      URL.revokeObjectURL(url);
    } catch (e) {
      errorMessage = String(e);
    }
  }

  async function handleImportVocab() {
    if (managingCollectionId === null) return;
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".json";
    input.onchange = async () => {
      const file = input.files?.[0];
      if (!file) return;
      try {
        const text = await file.text();
        const data = JSON.parse(text) as { correction: string; aliases: string[] }[];
        for (const entry of data) {
          await invoke("add_vocabulary_entry", {
            collectionId: managingCollectionId,
            correction: entry.correction,
            aliases: entry.aliases,
          });
        }
        vocabEntries = await invoke<VocabEntry[]>("list_vocabulary_entries", {
          collectionId: managingCollectionId,
        });
        await loadVocabCollections();
        successMessage = t("settings.saved");
        setTimeout(() => (successMessage = ""), 2000);
      } catch (e) {
        errorMessage = String(e);
      }
    };
    input.click();
  }

  $effect(() => {
    loadSettings();
    loadAudioDevices();
    loadProfiles();
    loadVocabCollections();
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
                class="w-full rounded border border-border bg-bg-primary px-3 py-1 text-sm"
              />
              <input
                type="text"
                bind:value={profileForm.app_identifier}
                placeholder={t("profiles.app_id")}
                class="w-full rounded border border-border bg-bg-primary px-3 py-1 text-sm"
              />
              <select
                bind:value={profileForm.app_identifier_type}
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
            class="w-full rounded border border-border bg-bg-primary px-3 py-1 text-sm"
          />
          <input
            type="text"
            bind:value={profileForm.app_identifier}
            placeholder={t("profiles.app_id")}
            class="w-full rounded border border-border bg-bg-primary px-3 py-1 text-sm"
          />
          <select
            bind:value={profileForm.app_identifier_type}
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

    <!-- Vocabulary Section -->
    <section class="mb-8">
      <h3 class="mb-4 text-sm font-medium uppercase tracking-wide text-text-secondary">
        {t("settings.section_vocabulary")}
      </h3>
      <p class="mb-3 text-xs text-text-secondary">{t("vocabulary.description")}</p>

      {#if managingCollectionId !== null}
        {@const collection = vocabCollections.find((c) => c.id === managingCollectionId)}
        <div class="space-y-3">
          <div class="flex items-center justify-between">
            <button
              onclick={closeCollectionManager}
              class="text-sm text-accent hover:underline"
            >
              {t("vocabulary.back")}
            </button>
            <span class="text-sm font-medium">{collection?.name ?? ""}</span>
            <div class="flex gap-1">
              <button
                onclick={handleImportVocab}
                class="rounded border border-border bg-bg-primary px-2 py-1 text-xs hover:bg-bg-surface"
              >
                {t("vocabulary.import")}
              </button>
              <button
                onclick={handleExportVocab}
                class="rounded border border-border bg-bg-primary px-2 py-1 text-xs hover:bg-bg-surface"
              >
                {t("vocabulary.export")}
              </button>
            </div>
          </div>

          <div class="space-y-1">
            {#each vocabEntries as entry (entry.id)}
              {#if editingEntryId === entry.id}
                <div class="rounded border border-accent bg-bg-surface p-2 space-y-1">
                  <input
                    type="text"
                    bind:value={entryForm.correction}
                    placeholder={t("vocabulary.correction")}
                    class="w-full rounded border border-border bg-bg-primary px-2 py-1 text-sm"
                  />
                  <input
                    type="text"
                    bind:value={entryForm.aliases}
                    placeholder={t("vocabulary.aliases")}
                    class="w-full rounded border border-border bg-bg-primary px-2 py-1 text-sm"
                  />
                  <div class="flex gap-2">
                    <button
                      onclick={() => handleUpdateEntry(entry.id)}
                      class="rounded bg-accent px-2 py-1 text-xs text-white hover:bg-accent/90"
                    >
                      {t("vocabulary.save")}
                    </button>
                    <button
                      onclick={() => { editingEntryId = null; entryForm = { correction: "", aliases: "" }; }}
                      class="rounded border border-border bg-bg-primary px-2 py-1 text-xs hover:bg-bg-surface"
                    >
                      {t("vocabulary.cancel")}
                    </button>
                  </div>
                </div>
              {:else}
                <div class="flex items-center justify-between rounded border border-border bg-bg-surface px-3 py-2">
                  <div class="min-w-0 flex-1">
                    <span class="text-sm font-medium">{entry.correction}</span>
                    <span class="ml-2 text-xs text-text-secondary">{entry.aliases.join(", ")}</span>
                  </div>
                  <div class="flex gap-1 ml-2">
                    <button
                      onclick={() => startEditEntry(entry)}
                      class="rounded px-2 py-0.5 text-xs text-text-secondary hover:bg-bg-primary"
                    >
                      {t("profiles.edit")}
                    </button>
                    <button
                      onclick={() => handleDeleteEntry(entry.id)}
                      class="rounded px-2 py-0.5 text-xs text-status-recording hover:bg-bg-primary"
                    >
                      {t("vocabulary.delete")}
                    </button>
                  </div>
                </div>
              {/if}
            {/each}
          </div>

          {#if showAddEntry}
            <div class="rounded border border-accent bg-bg-surface p-2 space-y-1">
              <input
                type="text"
                bind:value={entryForm.correction}
                placeholder={t("vocabulary.correction")}
                class="w-full rounded border border-border bg-bg-primary px-2 py-1 text-sm"
              />
              <input
                type="text"
                bind:value={entryForm.aliases}
                placeholder={t("vocabulary.aliases")}
                class="w-full rounded border border-border bg-bg-primary px-2 py-1 text-sm"
              />
              <div class="flex gap-2">
                <button
                  onclick={handleAddEntry}
                  class="rounded bg-accent px-2 py-1 text-xs text-white hover:bg-accent/90"
                >
                  {t("vocabulary.save")}
                </button>
                <button
                  onclick={() => { showAddEntry = false; entryForm = { correction: "", aliases: "" }; }}
                  class="rounded border border-border bg-bg-primary px-2 py-1 text-xs hover:bg-bg-surface"
                >
                  {t("vocabulary.cancel")}
                </button>
              </div>
            </div>
          {:else if editingEntryId === null}
            <button
              onclick={() => { showAddEntry = true; entryForm = { correction: "", aliases: "" }; }}
              class="rounded border border-border bg-bg-primary px-3 py-1 text-xs hover:bg-bg-surface"
            >
              {t("vocabulary.add_entry")}
            </button>
          {/if}
        </div>
      {:else}
        <div class="space-y-3">
          <div class="flex items-center justify-between mb-2">
            <label for="active-vocab" class="text-sm">{t("settings.vocabulary_id")}</label>
            <select
              id="active-vocab"
              value={settings.custom_vocabulary_id ?? ""}
              onchange={(e) => {
                const val = (e.target as HTMLSelectElement).value;
                saveSetting("custom_vocabulary_id", val === "" ? null : Number(val));
                if (settings) settings.custom_vocabulary_id = val === "" ? null : Number(val);
              }}
              class="max-w-48 truncate rounded border border-border bg-bg-primary px-3 py-1 text-sm"
            >
              <option value="">{t("settings.vocabulary_none")}</option>
              {#each vocabCollections as col (col.id)}
                <option value={col.id}>{col.name}</option>
              {/each}
            </select>
          </div>

          {#if vocabCollections.length === 0 && !showAddCollection}
            <p class="text-sm text-text-secondary">{t("vocabulary.empty")}</p>
          {/if}

          <div class="space-y-1">
            {#each vocabCollections as col (col.id)}
              <div class="flex items-center justify-between rounded border border-border bg-bg-surface px-3 py-2">
                <div>
                  <span class="text-sm font-medium">{col.name}</span>
                  <span class="ml-2 text-xs text-text-secondary">
                    {t("vocabulary.entries_count").replace("{count}", String(vocabEntryCounts[col.id] ?? 0))}
                  </span>
                </div>
                <div class="flex gap-1">
                  <button
                    onclick={() => openCollectionManager(col.id)}
                    class="rounded px-2 py-0.5 text-xs text-accent hover:bg-bg-primary"
                  >
                    {t("vocabulary.manage")}
                  </button>
                  <button
                    onclick={() => handleDeleteCollection(col.id)}
                    class="rounded px-2 py-0.5 text-xs text-status-recording hover:bg-bg-primary"
                  >
                    {t("vocabulary.delete")}
                  </button>
                </div>
              </div>
            {/each}
          </div>

          {#if showAddCollection}
            <div class="flex gap-2">
              <input
                type="text"
                bind:value={newCollectionName}
                placeholder={t("vocabulary.collection_name")}
                class="flex-1 rounded border border-border bg-bg-primary px-3 py-1 text-sm"
              />
              <button
                onclick={handleAddCollection}
                class="rounded bg-accent px-3 py-1 text-sm text-white hover:bg-accent/90"
              >
                {t("vocabulary.save")}
              </button>
              <button
                onclick={() => { showAddCollection = false; newCollectionName = ""; }}
                class="rounded border border-border bg-bg-primary px-3 py-1 text-sm hover:bg-bg-surface"
              >
                {t("vocabulary.cancel")}
              </button>
            </div>
          {:else}
            <button
              onclick={() => { showAddCollection = true; newCollectionName = ""; }}
              class="rounded border border-border bg-bg-primary px-3 py-1.5 text-sm text-text-primary hover:bg-bg-surface focus:outline-none focus:ring-2 focus:ring-accent"
            >
              {t("vocabulary.add_collection")}
            </button>
          {/if}
        </div>
      {/if}
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
