<script lang="ts">
  import { t } from "$lib/i18n/index.js";

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

  interface Props {
    entry: HistoryItem;
    onDelete?: (_id: number) => void;
    onCopy?: (_id: number) => void;
  }

  let { entry, onDelete, onCopy }: Props = $props();
  let expanded = $state(false);

  function formatDate(iso: string): string {
    try {
      const date = new Date(iso);
      return date.toLocaleString();
    } catch {
      return iso;
    }
  }

  function formatDuration(ms: number | null): string {
    if (ms === null) return "";
    if (ms < 1000) return `${ms}ms`;
    return `${(ms / 1000).toFixed(1)}s`;
  }
</script>

<article
  class="rounded-lg border border-border bg-bg-surface p-4"
  aria-label={t("history.entry")}
>
  <button
    onclick={() => (expanded = !expanded)}
    class="w-full text-left focus:outline-none focus:ring-2 focus:ring-accent rounded"
    aria-expanded={expanded}
    aria-label={expanded ? entry.text.slice(0, 60) : entry.text.slice(0, 60)}
  >
    <div class="flex items-start justify-between gap-2">
      <p class="text-sm text-text-primary line-clamp-2">{entry.text}</p>
      <span class="shrink-0 text-xs text-text-secondary">
        {formatDate(entry.created_at)}
      </span>
    </div>
  </button>

  {#if expanded}
    <div class="mt-3 border-t border-border pt-3">
      <p class="mb-3 text-sm text-text-primary whitespace-pre-wrap">
        {entry.text}
      </p>

      <div class="mb-3 flex flex-wrap gap-3 text-xs text-text-secondary">
        {#if entry.duration_ms !== null}
          <span>{t("history.duration")}: {formatDuration(entry.duration_ms)}</span>
        {/if}
        {#if entry.engine_id !== null}
          <span>{t("history.engine")}: {entry.engine_id}</span>
        {/if}
        {#if entry.words_per_minute !== null}
          <span>{t("history.wpm")}: {Math.round(entry.words_per_minute)}</span>
        {/if}
      </div>

      <div class="flex gap-2">
        <button
          onclick={() => onCopy?.(entry.id)}
          class="rounded bg-accent px-3 py-1.5 text-sm text-white hover:bg-accent-hover focus:outline-none focus:ring-2 focus:ring-accent"
        >
          {t("history.copy")}
        </button>
        <button
          onclick={() => onDelete?.(entry.id)}
          class="rounded bg-status-recording/20 px-3 py-1.5 text-sm text-status-recording hover:bg-status-recording/30 focus:outline-none focus:ring-2 focus:ring-status-recording"
        >
          {t("history.delete")}
        </button>
      </div>
    </div>
  {/if}
</article>
