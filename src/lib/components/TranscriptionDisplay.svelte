<script lang="ts">
  import { t } from "$lib/i18n/index.js";

  interface Props {
    text?: string;
    durationMs?: number;
    error?: string;
  }

  let { text = "", durationMs, error }: Props = $props();
</script>

<section
  class="w-full rounded-lg border border-border bg-bg-surface p-6"
  aria-label={t("transcription.label")}
  aria-live="polite"
>
  {#if error}
    <p class="text-status-recording" role="alert">{error}</p>
  {:else if text}
    <p class="whitespace-pre-wrap text-text-primary leading-relaxed">{text}</p>
    {#if durationMs !== undefined}
      <p class="mt-4 text-xs text-text-muted">
        {t("transcription.duration").replace("{duration}", String(durationMs))}
      </p>
    {/if}
  {:else}
    <p class="text-text-muted italic">{t("transcription.empty")}</p>
  {/if}
</section>
