<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { t } from "$lib/i18n/index.js";
  import ModelCard from "./ModelCard.svelte";

  interface ModelInfo {
    id: string;
    name: string;
    description: string;
    file: string;
    size_bytes: number;
    sha256: string;
    url: string;
    speed_tier: string;
    accuracy_tier: string;
    languages: string[];
    engine: string;
    installed: boolean;
    downloading: boolean;
    active: boolean;
  }

  interface DownloadProgress {
    model_id: string;
    downloaded_bytes: number;
    total_bytes: number;
    status: string;
  }

  let models: ModelInfo[] = $state([]);
  let downloadProgress: Record<string, number> = $state({});
  let loadingModelId: string | null = $state(null);
  let errorMessage = $state("");

  async function loadModels() {
    try {
      models = await invoke<ModelInfo[]>("list_available_models");
      errorMessage = "";
    } catch (e) {
      errorMessage = String(e);
    }
  }

  async function handleDownload(modelId: string) {
    try {
      await invoke("download_model", { modelId });
      // Mark as downloading in local state immediately
      models = models.map((m) =>
        m.id === modelId ? { ...m, downloading: true } : m,
      );
    } catch (e) {
      errorMessage = String(e);
    }
  }

  async function handleCancel(modelId: string) {
    try {
      await invoke("cancel_download", { modelId });
    } catch (e) {
      errorMessage = String(e);
    }
  }

  async function handleDelete(modelId: string) {
    try {
      await invoke("delete_model", { modelId });
      await loadModels();
    } catch (e) {
      errorMessage = String(e);
    }
  }

  async function handleActivate(modelId: string) {
    loadingModelId = modelId;
    try {
      await invoke("set_active_model", { modelId });
      await loadModels();
    } catch (e) {
      errorMessage = String(e);
    } finally {
      loadingModelId = null;
    }
  }

  // Load models on mount
  $effect(() => {
    loadModels();
  });

  // Listen for download progress events
  $effect(() => {
    const unlisten = listen<DownloadProgress>(
      "model:download-progress",
      (event) => {
        const data = event.payload;

        if (data.total_bytes > 0) {
          downloadProgress = {
            ...downloadProgress,
            [data.model_id]: data.downloaded_bytes / data.total_bytes,
          };
        }

        if (data.status === "complete" || data.status === "failed" || data.status === "cancelled") {
          // Refresh models list
          loadModels();
          // Clear progress
          const { [data.model_id]: _, ...rest } = downloadProgress;
          downloadProgress = rest;
        }
      },
    );

    return () => {
      unlisten.then((fn) => fn());
    };
  });
</script>

<div class="mx-auto max-w-2xl px-6 py-8">
  <h2 class="mb-6 text-lg font-semibold">{t("models.title")}</h2>

  {#if errorMessage}
    <p class="mb-4 rounded bg-status-recording/20 p-3 text-sm text-status-recording" role="alert">
      {errorMessage}
    </p>
  {/if}

  {#if loadingModelId}
    <p class="mb-4 text-sm text-text-secondary" aria-live="polite">
      {t("models.loading")}
    </p>
  {/if}

  <div class="space-y-4" role="list" aria-label={t("models.available")}>
    {#each models as model (model.id)}
      <div role="listitem">
        <ModelCard
          {model}
          downloadProgress={downloadProgress[model.id]}
          onDownload={handleDownload}
          onCancel={handleCancel}
          onDelete={handleDelete}
          onActivate={handleActivate}
        />
      </div>
    {/each}
  </div>

  {#if models.length === 0}
    <p class="text-center text-text-secondary">{t("models.no_models")}</p>
  {/if}
</div>
