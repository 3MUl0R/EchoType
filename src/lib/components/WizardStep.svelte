<script lang="ts">
  import { t } from "$lib/i18n/index.js";
  import type { Snippet } from "svelte";

  interface Props {
    title: string;
    description?: string;
    showBack?: boolean;
    showSkip?: boolean;
    showNext?: boolean;
    nextLabel?: string;
    nextDisabled?: boolean;
    onBack?: () => void;
    onSkip?: () => void;
    onNext?: () => void;
    children: Snippet;
  }

  let {
    title,
    description,
    showBack = true,
    showSkip = true,
    showNext = true,
    nextLabel,
    nextDisabled = false,
    onBack,
    onSkip,
    onNext,
    children,
  }: Props = $props();
</script>

<div class="flex flex-col items-center">
  <h2 class="mb-2 text-2xl font-semibold">{title}</h2>
  {#if description}
    <p class="mb-6 max-w-md text-center text-sm text-text-secondary">
      {description}
    </p>
  {/if}

  <div class="mb-8 w-full max-w-md">
    {@render children()}
  </div>

  <div class="flex items-center gap-3">
    {#if showBack}
      <button
        onclick={onBack}
        class="rounded border border-border px-4 py-2 text-sm text-text-secondary hover:text-text-primary focus:outline-none focus:ring-2 focus:ring-accent"
      >
        {t("wizard.back")}
      </button>
    {/if}
    {#if showSkip}
      <button
        onclick={onSkip}
        class="px-4 py-2 text-sm text-text-muted hover:text-text-secondary focus:outline-none focus:ring-2 focus:ring-accent rounded"
      >
        {t("wizard.skip")}
      </button>
    {/if}
    {#if showNext}
      <button
        onclick={onNext}
        disabled={nextDisabled}
        class="rounded bg-accent px-6 py-2 text-sm font-medium text-white hover:bg-accent-hover focus:outline-none focus:ring-2 focus:ring-accent disabled:opacity-50 disabled:cursor-not-allowed"
      >
        {nextLabel ?? t("wizard.next")}
      </button>
    {/if}
  </div>
</div>
