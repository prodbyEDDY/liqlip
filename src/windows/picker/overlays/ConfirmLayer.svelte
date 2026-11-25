<script lang="ts">
  // Folder deletion confirmation (SPEC §3.9): "Delete folder, keep the items in history" or
  // "Delete with its items" (both undoable from the toast). Focus is trapped in the dialog;
  // Esc cancels; the safe choice is focused first.
  import { onMount } from "svelte";
  import Ico from "$lib/components/Ico.svelte";
  import Button from "$lib/components/Button.svelte";
  import { LayerSurface } from "./kit";
  import { getLocale, t, type TranslationKey } from "$lib/i18n/index.svelte";
  import type { LayerProps } from "../overlays.svelte";
  import { shelf } from "../shelf.svelte";
  import { pluralCategory } from "../format";

  let { layer, close }: LayerProps = $props();

  const folderId = $derived(layer.ctx.folderId as number);
  const name = $derived(shelf.folderName(folderId));
  const count = $derived(shelf.counts?.folders[folderId] ?? shelf.folders.find((f) => f.id === folderId)?.count ?? 0);
  const countText = $derived(t(`picker.count.items.${pluralCategory(count, getLocale())}` as TranslationKey, { n: count }));

  let dialog = $state<HTMLDivElement | null>(null);

  onMount(() => {
    dialog?.querySelector<HTMLButtonElement>("button[data-autofocus]")?.focus();
  });

  function choose(withClips: boolean) {
    close();
    void shelf.deleteFolder(folderId, withClips);
    shelf.focusSearch(false);
  }

  function cancel() {
    close();
    shelf.focusSearch(false);
  }

  $effect(() => {
    layer.onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        cancel();
        return true;
      }
      if (e.key === "Tab" && dialog) {
        // Keep focus inside the dialog.
        const buttons = [...dialog.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")];
        const i = buttons.indexOf(document.activeElement as HTMLButtonElement);
        const next = buttons[(i + (e.shiftKey ? -1 : 1) + buttons.length) % buttons.length];
        next?.focus();
        return true;
      }
      return false; // Enter / Space activate the focused button natively
    };
  });
</script>

<LayerSurface variant="dialog" role="alertdialog" labelledby="confirm-title" describedby="confirm-desc" onclose={cancel} bind:el={dialog}>
  <div class="body">
    <div class="icon squircle" aria-hidden="true"><Ico name="folderMinus" size={20} /></div>
    <h2 id="confirm-title" class="title">{t("picker.folder.confirm.title", { name })}</h2>
    <p id="confirm-desc" class="desc">
      {count > 0 ? t("picker.folder.confirm.desc", { count: countText }) : t("picker.folder.confirm.empty")}
    </p>
  </div>
  <div class="actions">
    {#if count > 0}
      <Button variant="secondary" size="sm" icon="folderMinus" data-autofocus onclick={() => choose(false)}>{t("picker.folder.confirm.keep")}</Button>
      <Button variant="danger" size="sm" icon="trash" onclick={() => choose(true)}>{t("picker.folder.confirm.withClips")}</Button>
    {:else}
      <Button variant="danger" size="sm" icon="trash" data-autofocus onclick={() => choose(false)}>{t("common.delete")}</Button>
    {/if}
    <Button variant="ghost" size="sm" onclick={cancel}>{t("common.cancel")}</Button>
  </div>
</LayerSurface>

<style>
  .body {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 12px 8px 14px;
    text-align: center;
  }
  .icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 44px;
    height: 44px;
    margin-bottom: 6px;
    border-radius: var(--radius-md);
    background: var(--color-track);
    color: var(--color-ink-2);
  }
  .title {
    margin: 0;
    color: var(--color-ink);
    font-size: var(--text-panel);
    font-weight: 600;
    line-height: var(--text-panel--line-height);
    overflow-wrap: anywhere;
    text-wrap: balance;
  }
  .desc {
    margin: 0;
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
    line-height: var(--text-secondary--line-height);
    text-wrap: pretty;
  }
  .actions {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .actions :global(.btn) {
    width: 100%;
  }
</style>
