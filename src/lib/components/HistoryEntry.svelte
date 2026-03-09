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
  let copied = $state(false);

  async function copyToClipboard() {
    try {
      await navigator.clipboard.writeText(entry.text);
      copied = true;
      setTimeout(() => (copied = false), 2000);
      onCopy?.(entry.id);
    } catch {
      onCopy?.(entry.id);
    }
  }

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
  <div class="flex items-start gap-2">
    <button
      onclick={() => (expanded = !expanded)}
      class="flex-1 text-left focus:outline-none focus:ring-2 focus:ring-accent rounded"
      aria-expanded={expanded}
      aria-label={entry.text.slice(0, 60)}
    >
      <div class="flex items-start justify-between gap-2">
        <p class="text-sm text-text-primary line-clamp-2">{entry.text}</p>
        <span class="shrink-0 text-xs text-text-secondary">
          {formatDate(entry.created_at)}
        </span>
      </div>
    </button>
    <button
      onclick={copyToClipboard}
      class="shrink-0 rounded p-1.5 text-text-muted hover:bg-bg-secondary hover:text-text-primary transition-colors"
      aria-label={copied ? t("transcription.copied") : t("transcription.copy")}
      title={copied ? t("transcription.copied") : t("transcription.copy")}
    >
      {#if copied}
        <svg class="h-4 w-4 text-status-success" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
          <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
        </svg>
      {:else}
        <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
          <path stroke-linecap="round" stroke-linejoin="round" d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" />
        </svg>
      {/if}
    </button>
  </div>

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
