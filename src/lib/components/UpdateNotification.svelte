<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n/index.js";

  interface UpdateInfo {
    version: string;
    notes: string | null;
    date: string | null;
  }

  let update: UpdateInfo | null = $state(null);
  let dismissed = $state(false);

  $effect(() => {
    checkUpdate();
  });

  async function checkUpdate() {
    try {
      update = await invoke<UpdateInfo | null>("check_for_update");
    } catch {
      // Silently fail — offline is normal
    }
  }
</script>

{#if update && !dismissed}
  <div
    class="flex items-center justify-between border-b border-accent/30 bg-accent/10 px-6 py-2"
    role="status"
  >
    <div class="flex items-center gap-3">
      <span class="text-sm font-medium text-accent">
        {t("update.available", { version: update.version })}
      </span>
    </div>
    <div class="flex items-center gap-2">
      <button
        onclick={() => (dismissed = true)}
        class="text-text-muted hover:text-text-primary"
        aria-label={t("wizard.banner_dismiss")}
      >
        <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
          <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
        </svg>
      </button>
    </div>
  </div>
{/if}
