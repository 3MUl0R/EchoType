<script lang="ts">
  import { t } from "$lib/i18n/index.js";

  interface ModelInfo {
    id: string;
    name: string;
    description: string;
    size_bytes: number;
    speed_tier: string;
    accuracy_tier: string;
    languages: string[];
    installed: boolean;
    downloading: boolean;
    active: boolean;
  }

  interface Props {
    model: ModelInfo;
    downloadProgress?: number;
    onDownload?: (_id: string) => void;
    onCancel?: (_id: string) => void;
    onDelete?: (_id: string) => void;
    onActivate?: (_id: string) => void;
  }

  let {
    model,
    downloadProgress,
    onDownload,
    onCancel,
    onDelete,
    onActivate,
  }: Props = $props();

  function formatSize(bytes: number): string {
    if (bytes >= 1024 * 1024 * 1024) {
      return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
    }
    return `${(bytes / (1024 * 1024)).toFixed(0)} MB`;
  }

  function tierLabel(tier: string): string {
    return tier.charAt(0).toUpperCase() + tier.slice(1);
  }
</script>

<article
  class="rounded-lg border border-border bg-bg-surface p-4"
  aria-label={model.name}
>
  <div class="mb-2 flex items-start justify-between">
    <div>
      <h3 class="font-medium text-text-primary">{model.name}</h3>
      <p class="mt-1 text-sm text-text-secondary">{model.description}</p>
    </div>
    {#if model.active}
      <span
        class="rounded-full bg-status-success/20 px-2 py-0.5 text-xs font-medium text-status-success"
      >
        {t("models.active")}
      </span>
    {/if}
  </div>

  <div class="mb-3 flex flex-wrap gap-3 text-xs text-text-secondary">
    <span>{t("models.size")}: {formatSize(model.size_bytes)}</span>
    <span>{t("models.speed")}: {tierLabel(model.speed_tier)}</span>
    <span>{t("models.accuracy")}: {tierLabel(model.accuracy_tier)}</span>
    <span>
      {t("models.languages")}:
      {model.languages.length > 3
        ? `${model.languages.slice(0, 3).join(", ")} +${model.languages.length - 3}`
        : model.languages.join(", ")}
    </span>
  </div>

  {#if model.downloading && downloadProgress !== undefined}
    <div class="mb-3">
      <div
        class="h-2 w-full overflow-hidden rounded-full bg-bg-primary"
        role="progressbar"
        aria-valuenow={Math.round(downloadProgress * 100)}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-label="{model.name} download progress"
      >
        <div
          class="h-full rounded-full bg-accent transition-all duration-200"
          style="width: {downloadProgress * 100}%"
        ></div>
      </div>
      <p class="mt-1 text-xs text-text-secondary">
        {Math.round(downloadProgress * 100)}%
      </p>
    </div>
  {/if}

  <div class="flex gap-2">
    {#if model.downloading}
      <button
        onclick={() => onCancel?.(model.id)}
        class="rounded bg-status-recording/20 px-3 py-1.5 text-sm text-status-recording hover:bg-status-recording/30 focus:outline-none focus:ring-2 focus:ring-status-recording"
      >
        {t("models.cancel")}
      </button>
    {:else if !model.installed}
      <button
        onclick={() => onDownload?.(model.id)}
        class="rounded bg-accent px-3 py-1.5 text-sm text-white hover:bg-accent-hover focus:outline-none focus:ring-2 focus:ring-accent"
      >
        {t("models.download")}
      </button>
    {:else if !model.active}
      <button
        onclick={() => onActivate?.(model.id)}
        class="rounded bg-accent px-3 py-1.5 text-sm text-white hover:bg-accent-hover focus:outline-none focus:ring-2 focus:ring-accent"
      >
        {t("models.activate")}
      </button>
      <button
        onclick={() => onDelete?.(model.id)}
        class="rounded bg-status-recording/20 px-3 py-1.5 text-sm text-status-recording hover:bg-status-recording/30 focus:outline-none focus:ring-2 focus:ring-status-recording"
      >
        {t("models.delete")}
      </button>
    {/if}
  </div>
</article>
