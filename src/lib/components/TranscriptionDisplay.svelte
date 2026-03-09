<script lang="ts">
  import { t } from "$lib/i18n/index.js";

  interface Props {
    text?: string;
    durationMs?: number;
    error?: string;
  }

  let { text = "", durationMs, error }: Props = $props();

  let copied = $state(false);

  async function copyText() {
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      copied = true;
      setTimeout(() => (copied = false), 2000);
    } catch {
      // Fallback ignored — clipboard API may fail in some contexts
    }
  }
</script>

<section
  class="w-full rounded-lg border border-border bg-bg-surface p-6"
  aria-label={t("transcription.label")}
  aria-live="polite"
>
  {#if error}
    <p class="text-status-recording" role="alert">{error}</p>
  {:else if text}
    <div class="flex items-start justify-between gap-3">
      <p class="whitespace-pre-wrap text-text-primary leading-relaxed flex-1">{text}</p>
      <button
        onclick={copyText}
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
    {#if durationMs !== undefined}
      <p class="mt-4 text-xs text-text-muted">
        {t("transcription.duration").replace("{duration}", String(durationMs))}
      </p>
    {/if}
  {:else}
    <p class="text-text-muted italic">{t("transcription.empty")}</p>
  {/if}
</section>
