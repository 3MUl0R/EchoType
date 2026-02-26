<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n/index.js";

  interface PermissionStatus {
    accessibility: boolean;
    microphone: boolean;
  }

  let status: PermissionStatus | null = $state(null);

  async function checkPermissions() {
    try {
      status = await invoke<PermissionStatus>("check_permissions");
    } catch {
      status = null;
    }
  }

  async function openSettings(permission: string) {
    try {
      await invoke("open_permission_settings", { permission });
    } catch {
      // Best-effort — platform may not support opening settings
    }
  }

  $effect(() => {
    checkPermissions();
  });
</script>

{#if status}
  <section class="mb-8">
    <h3
      class="mb-4 text-sm font-medium uppercase tracking-wide text-text-secondary"
    >
      {t("permissions.title")}
    </h3>
    <div class="space-y-3">
      <!-- Accessibility -->
      <div
        class="flex items-center justify-between rounded border border-border p-3"
      >
        <div class="flex-1">
          <div class="flex items-center gap-2">
            <span class="text-sm font-medium"
              >{t("permissions.accessibility")}</span
            >
            <span
              class="rounded-full px-2 py-0.5 text-xs font-medium {status.accessibility
                ? 'bg-status-success/20 text-status-success'
                : 'bg-status-recording/20 text-status-recording'}"
            >
              {status.accessibility
                ? t("permissions.granted")
                : t("permissions.not_granted")}
            </span>
          </div>
          <p class="mt-1 text-xs text-text-secondary">
            {t("permissions.accessibility_desc")}
          </p>
        </div>
        {#if !status.accessibility}
          <button
            onclick={() => openSettings("accessibility")}
            class="ml-3 shrink-0 rounded border border-border bg-bg-primary px-3 py-1 text-xs hover:bg-bg-surface focus:outline-none focus:ring-2 focus:ring-accent"
          >
            {t("permissions.open_settings")}
          </button>
        {/if}
      </div>

      <!-- Microphone -->
      <div
        class="flex items-center justify-between rounded border border-border p-3"
      >
        <div class="flex-1">
          <div class="flex items-center gap-2">
            <span class="text-sm font-medium"
              >{t("permissions.microphone")}</span
            >
            <span
              class="rounded-full px-2 py-0.5 text-xs font-medium {status.microphone
                ? 'bg-status-success/20 text-status-success'
                : 'bg-status-recording/20 text-status-recording'}"
            >
              {status.microphone
                ? t("permissions.granted")
                : t("permissions.not_granted")}
            </span>
          </div>
          <p class="mt-1 text-xs text-text-secondary">
            {t("permissions.microphone_desc")}
          </p>
        </div>
        {#if !status.microphone}
          <button
            onclick={() => openSettings("microphone")}
            class="ml-3 shrink-0 rounded border border-border bg-bg-primary px-3 py-1 text-xs hover:bg-bg-surface focus:outline-none focus:ring-2 focus:ring-accent"
          >
            {t("permissions.open_settings")}
          </button>
        {/if}
      </div>

      <!-- Refresh button -->
      <button
        onclick={checkPermissions}
        class="text-xs text-text-secondary underline hover:text-text-primary"
      >
        {t("permissions.refresh")}
      </button>
    </div>
  </section>
{/if}
