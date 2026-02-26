<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n/index.js";

  let diagnosticText = $state("");
  let loading = $state(false);
  let copied = $state(false);
  let copyFailed = $state(false);

  async function collectAndCopy() {
    loading = true;
    copied = false;
    copyFailed = false;
    try {
      diagnosticText = await invoke<string>("get_diagnostic_info");
    } catch (e) {
      diagnosticText = `Error collecting diagnostics: ${e}`;
      loading = false;
      return;
    }
    try {
      await navigator.clipboard.writeText(diagnosticText);
      copied = true;
    } catch {
      copyFailed = true;
    } finally {
      loading = false;
    }
  }
</script>

<div class="flex flex-col gap-3">
  <div class="flex items-center gap-3">
    <button
      onclick={collectAndCopy}
      disabled={loading}
      class="rounded bg-accent px-3 py-1.5 text-sm text-white hover:bg-accent-hover disabled:opacity-50"
    >
      {loading ? t("diagnostic.collecting") : t("diagnostic.copy_button")}
    </button>
    {#if copied}
      <span class="text-sm text-status-success">{t("diagnostic.copied")}</span>
    {/if}
    {#if copyFailed}
      <span class="text-sm text-text-muted">{t("diagnostic.copy_failed")}</span>
    {/if}
  </div>
  {#if diagnosticText}
    <pre class="max-h-48 overflow-auto rounded bg-bg-surface p-3 text-xs text-text-secondary">{diagnosticText}</pre>
  {/if}
</div>
