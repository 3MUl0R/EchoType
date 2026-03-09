<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n/index.js";
  import HistoryEntry from "./HistoryEntry.svelte";

  interface HistoryItem {
    id: number;
    text: string;
    audio_path: string | null;
    duration_ms: number | null;
    engine_id: string | null;
    language: string | null;
    words_per_minute: number | null;
    created_at: string;
    is_private: boolean;
  }

  let entries: HistoryItem[] = $state([]);
  let errorMessage = $state("");
  let loading = $state(false);
  let hasMore = $state(true);
  let offset = $state(0);
  let totalCount = $state(0);
  const pageSize = 20;

  async function loadEntries(reset = false) {
    if (reset) {
      offset = 0;
      entries = [];
      hasMore = true;
    }

    loading = true;
    try {
      // Fetch total count for diagnostics
      const count = await invoke<number>("get_history_count");
      totalCount = count;
      console.log("[History] total entries in DB:", count);

      const result = await invoke<HistoryItem[]>("get_history", {
        limit: pageSize,
        offset,
      });
      console.log("[History] fetched", result.length, "entries, offset:", offset);
      if (reset) {
        entries = result;
      } else {
        entries = [...entries, ...result];
      }
      hasMore = result.length === pageSize;
      offset += result.length;
      errorMessage = "";
    } catch (e) {
      console.error("[History] load error:", e);
      errorMessage = String(e);
    } finally {
      loading = false;
    }
  }

  async function handleDelete(id: number) {
    try {
      await invoke("delete_history_entry", { id });
      entries = entries.filter((e) => e.id !== id);
    } catch (e) {
      errorMessage = String(e);
    }
  }

  async function handleCopy(id: number) {
    try {
      await invoke("copy_history_text", { id });
    } catch (e) {
      errorMessage = String(e);
    }
  }

  async function handleClearAll() {
    try {
      await invoke("clear_history");
      entries = [];
      hasMore = false;
    } catch (e) {
      errorMessage = String(e);
    }
  }

  $effect(() => {
    loadEntries(true);
  });
</script>

<div class="mx-auto max-w-2xl px-6 py-8">
  <div class="mb-6 flex items-center justify-between">
    <h2 class="text-lg font-semibold">{t("history.title")}</h2>
    {#if entries.length > 0}
      <button
        onclick={handleClearAll}
        class="rounded bg-status-recording/20 px-3 py-1.5 text-sm text-status-recording hover:bg-status-recording/30 focus:outline-none focus:ring-2 focus:ring-status-recording"
      >
        {t("history.clear_all")}
      </button>
    {/if}
  </div>

  {#if errorMessage}
    <p
      class="mb-4 rounded bg-status-recording/20 p-3 text-sm text-status-recording"
      role="alert"
    >
      {errorMessage}
    </p>
  {/if}

  <div class="space-y-3" role="list" aria-label={t("history.title")}>
    {#each entries as entry (entry.id)}
      <div role="listitem">
        <HistoryEntry
          {entry}
          onDelete={handleDelete}
          onCopy={handleCopy}
        />
      </div>
    {/each}
  </div>

  {#if entries.length === 0 && !loading}
    <p class="text-center text-text-secondary">{t("history.empty")}</p>
    {#if totalCount === 0}
      <p class="mt-2 text-center text-xs text-text-muted">
        Dictation history will appear here after your first transcription.
      </p>
    {/if}
  {/if}

  {#if hasMore && entries.length > 0}
    <div class="mt-4 flex justify-center">
      <button
        onclick={() => loadEntries()}
        disabled={loading}
        class="rounded bg-bg-primary px-4 py-2 text-sm text-text-primary border border-border hover:bg-bg-surface focus:outline-none focus:ring-2 focus:ring-accent disabled:opacity-50"
      >
        {loading ? t("history.loading") : t("history.load_more")}
      </button>
    </div>
  {/if}
</div>
