<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n/index.js";

  interface Props {
    onNavigate: (_page: string) => void;
    onDismiss: () => void;
  }

  let { onNavigate, onDismiss }: Props = $props();

  let hasModel = $state(true);
  let hasMic = $state(true);

  $effect(() => {
    checkReadiness();
  });

  async function checkReadiness() {
    try {
      const models = await invoke<{ installed: boolean }[]>(
        "list_available_models",
      );
      hasModel = models.some((m) => m.installed);

      const perms = await invoke<{ microphone: boolean }>("check_permissions");
      hasMic = perms.microphone;
    } catch {
      // If we can't check, assume OK
    }
  }

  let showBanner = $derived(!hasModel || !hasMic);
</script>

{#if showBanner}
  <div
    class="flex items-center justify-between border-b border-status-recording/30 bg-status-recording/10 px-6 py-2"
    role="alert"
  >
    <div class="flex items-center gap-3">
      <span class="text-sm font-medium text-status-recording">
        {t("wizard.banner_incomplete")}
      </span>
      <div class="flex gap-2">
        {#if !hasModel}
          <button
            onclick={() => onNavigate("models")}
            class="rounded bg-accent/20 px-2 py-0.5 text-xs text-accent hover:bg-accent/30"
          >
            {t("wizard.banner_download_model")}
          </button>
        {/if}
        {#if !hasMic}
          <button
            onclick={() => onNavigate("settings")}
            class="rounded bg-accent/20 px-2 py-0.5 text-xs text-accent hover:bg-accent/30"
          >
            {t("wizard.banner_grant_mic")}
          </button>
        {/if}
      </div>
    </div>
    <button
      onclick={onDismiss}
      class="text-text-muted hover:text-text-primary"
      aria-label={t("wizard.banner_dismiss")}
    >
      <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
        <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
      </svg>
    </button>
  </div>
{/if}
