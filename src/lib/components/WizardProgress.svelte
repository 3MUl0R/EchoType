<script lang="ts">
  import { t } from "$lib/i18n/index.js";

  interface Props {
    currentStep: number;
    totalSteps: number;
    stepLabels: string[];
  }

  let { currentStep, totalSteps, stepLabels }: Props = $props();
</script>

<nav
  class="mb-8 flex items-center justify-center gap-2"
  aria-label={t("wizard.progress")}
>
  {#each stepLabels as label, i (i)}
    <div class="flex items-center gap-2">
      <div
        class="flex h-8 w-8 items-center justify-center rounded-full text-sm font-medium
          {i < currentStep
          ? 'bg-accent text-white'
          : i === currentStep
            ? 'bg-accent text-white ring-2 ring-accent ring-offset-2 ring-offset-bg-primary'
            : 'bg-bg-surface text-text-muted'}"
        aria-current={i === currentStep ? "step" : undefined}
        aria-label="{label} — {i < currentStep ? t('wizard.step_complete') : i === currentStep ? t('wizard.step_current') : t('wizard.step_upcoming')}"
      >
        {#if i < currentStep}
          <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="3">
            <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
          </svg>
        {:else}
          {i + 1}
        {/if}
      </div>
      {#if i < totalSteps - 1}
        <div
          class="h-0.5 w-6 {i < currentStep ? 'bg-accent' : 'bg-bg-surface'}"
        ></div>
      {/if}
    </div>
  {/each}
</nav>
