<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n/index.js";

  let ipcResult = $state("");

  async function testIpc() {
    try {
      ipcResult = await invoke<string>("ping");
    } catch (e) {
      ipcResult = `Error: ${e}`;
    }
  }

  testIpc();
</script>

<div class="dark min-h-screen bg-bg-primary text-text-primary">
  <header class="border-b border-border px-6 py-4">
    <h1 class="text-xl font-semibold">{t("app.title")}</h1>
  </header>

  <nav class="border-b border-border px-6 py-2" aria-label={t("nav.label")}>
    <ul class="flex gap-4">
      <li>
        <button class="text-accent hover:text-accent-hover focus:outline-none focus:ring-2 focus:ring-accent rounded px-2 py-1">
          {t("nav.dictation")}
        </button>
      </li>
      <li>
        <button class="text-text-secondary hover:text-text-primary focus:outline-none focus:ring-2 focus:ring-accent rounded px-2 py-1">
          {t("nav.settings")}
        </button>
      </li>
    </ul>
  </nav>

  <main class="px-6 py-8">
    <p class="text-text-secondary mb-4">{t("app.description")}</p>
    {#if ipcResult}
      <p class="text-sm text-text-muted">
        {t("app.ipc_status")}: <span class="text-status-success">{ipcResult}</span>
      </p>
    {/if}
  </main>
</div>
