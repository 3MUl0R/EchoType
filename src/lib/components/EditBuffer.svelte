<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { t } from "$lib/i18n/index.js";

  let text = $state("");
  let loaded = $state(false);

  // Load the pending edit text from backend
  $effect(() => {
    invoke<string>("get_edit_buffer_text").then((result) => {
      text = result;
      loaded = true;
      // Focus the textarea after loading
      requestAnimationFrame(() => {
        const textarea = document.querySelector("textarea");
        if (textarea) {
          textarea.focus();
          textarea.setSelectionRange(textarea.value.length, textarea.value.length);
        }
      });
    });
  });

  async function insertText() {
    try {
      await invoke("edit_buffer_insert", { text });
      await getCurrentWindow().close();
    } catch (e) {
      console.error("Insert failed:", e);
    }
  }

  async function discardText() {
    try {
      await invoke("edit_buffer_discard");
      await getCurrentWindow().close();
    } catch (e) {
      console.error("Discard failed:", e);
    }
  }

  async function copyText() {
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      // Fallback: invoke backend clipboard
      await invoke("edit_buffer_copy", { text });
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      discardText();
    } else if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
      event.preventDefault();
      insertText();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="dark flex h-screen flex-col bg-bg-primary text-text-primary">
  <div class="flex items-center justify-between border-b border-border px-4 py-2">
    <span class="text-sm font-medium text-text-secondary">{t("edit_buffer.title")}</span>
    <span class="text-xs text-text-muted">
      {#if navigator.platform?.includes("Mac")}
        {t("edit_buffer.hint_mac")}
      {:else}
        {t("edit_buffer.hint")}
      {/if}
    </span>
  </div>

  <div class="flex-1 p-3">
    {#if loaded}
      <textarea
        bind:value={text}
        class="h-full w-full resize-none rounded border border-border bg-bg-secondary p-3
          text-sm text-text-primary placeholder-text-muted
          focus:border-accent focus:outline-none focus:ring-1 focus:ring-accent"
        placeholder={t("edit_buffer.placeholder")}
        spellcheck="true"
      ></textarea>
    {:else}
      <div class="flex h-full items-center justify-center text-text-muted">
        {t("edit_buffer.loading")}
      </div>
    {/if}
  </div>

  <div class="flex items-center justify-between border-t border-border px-4 py-3">
    <button
      onclick={discardText}
      class="rounded px-3 py-1.5 text-sm text-text-secondary
        hover:bg-bg-secondary hover:text-text-primary
        focus:outline-none focus:ring-2 focus:ring-accent"
    >
      {t("edit_buffer.discard")}
    </button>

    <div class="flex gap-2">
      <button
        onclick={copyText}
        class="rounded border border-border px-3 py-1.5 text-sm text-text-secondary
          hover:bg-bg-secondary hover:text-text-primary
          focus:outline-none focus:ring-2 focus:ring-accent"
      >
        {t("edit_buffer.copy")}
      </button>
      <button
        onclick={insertText}
        class="rounded bg-accent px-4 py-1.5 text-sm font-medium text-white
          hover:bg-accent-hover focus:outline-none focus:ring-2 focus:ring-accent"
      >
        {t("edit_buffer.insert")}
      </button>
    </div>
  </div>
</div>
