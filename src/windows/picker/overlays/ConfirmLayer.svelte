<script lang="ts">
  // Folder deletion confirmation (SPEC §3.9): "Delete folder, keep the items in history" or
  // "Delete with its items" (both undoable from the toast). Focus is trapped in the dialog;
  // Esc cancels; the safe choice is focused first.
  import { onMount } from "svelte";
  import Ico from "$lib/components/Ico.svelte";
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

<div class="scrim" role="presentation" onpointerdown={cancel}></div>
<div bind:this={dialog} class="dialog squircle" role="alertdialog" aria-modal="true" aria-labelledby="confirm-title" aria-describedby="confirm-desc">
  <div class="dialog__icon squircle"><Ico name="trash" size={20} /></div>
  <h2 id="confirm-title" class="dialog__title">{t("picker.folder.confirm.title", { name })}</h2>
  <p id="confirm-desc" class="dialog__desc">
    {count > 0 ? t("picker.folder.confirm.desc", { count: countText }) : t("picker.folder.confirm.empty")}
  </p>
  <div class="dialog__actions">
    {#if count > 0}
      <button type="button" class="btn btn--secondary squircle" data-autofocus onclick={() => choose(false)}>
        <Ico name="folderMinus" size={15} />{t("picker.folder.confirm.keep")}
      </button>
      <button type="button" class="btn btn--danger squircle" onclick={() => choose(true)}>
        <Ico name="trash" size={15} />{t("picker.folder.confirm.withClips")}
      </button>
    {:else}
      <button type="button" class="btn btn--danger squircle" data-autofocus onclick={() => choose(false)}>
        <Ico name="trash" size={15} />{t("common.delete")}
      </button>
    {/if}
    <button type="button" class="btn btn--ghost squircle" onclick={cancel}>{t("common.cancel")}</button>
  </div>
</div>

<style>
  .scrim {
    position: absolute;
    inset: 0;
    z-index: 40;
    border-radius: inherit;
    background: color-mix(in srgb, var(--color-paper) 55%, transparent);
    animation: fade-in var(--dur-base) var(--ease-out-soft);
  }
  .dialog {
    position: absolute;
    top: 50%;
    left: 50%;
    z-index: 41;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    width: min(340px, calc(100% - 32px));
    padding: 20px 18px 16px;
    border-radius: 22px;
    background: var(--color-paper);
    box-shadow: var(--shadow-card-lg);
    text-align: center;
    transform: translate(-50%, -50%);
    animation: dialog-in var(--dur-slow) var(--ease-liquid);
  }
  @keyframes dialog-in {
    from {
      opacity: 0;
      transform: translate(-50%, -50%) scale(0.94);
    }
  }
  @keyframes fade-in {
    from {
      opacity: 0;
    }
  }
  .dialog__icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 44px;
    height: 44px;
    margin-bottom: 4px;
    border-radius: 15px;
    background: var(--color-track);
    color: var(--color-ink-2);
  }
  .dialog__title {
    margin: 0;
    color: var(--color-ink);
    font-size: var(--text-panel);
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  .dialog__desc {
    margin: 0 0 8px;
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
    line-height: 1.45;
  }
  .dialog__actions {
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 100%;
  }
  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
    height: 34px;
    padding: 0 14px;
    border: 0;
    border-radius: 12px;
    font: inherit;
    font-size: var(--text-secondary);
    font-weight: 600;
    cursor: pointer;
    transition:
      background-color var(--dur-fast) var(--ease-out-soft),
      color var(--dur-fast) var(--ease-out-soft),
      transform var(--dur-fast) var(--ease-out-soft);
  }
  .btn:active {
    transform: scale(0.97);
  }
  .btn:focus-visible {
    outline: 2px solid var(--color-ink);
    outline-offset: 2px;
  }
  .btn:disabled {
    opacity: 0.45;
  }
  .btn--secondary {
    background: var(--color-track);
    color: var(--color-ink);
  }
  .btn--secondary:hover {
    background: var(--color-hover);
  }
  .btn--danger {
    background: var(--color-track);
    color: var(--color-danger);
  }
  .btn--danger:hover {
    background: var(--color-hover);
  }
  .btn--ghost {
    background: transparent;
    color: var(--color-ink-2);
  }
  .btn--ghost:hover {
    background: var(--color-hover);
    color: var(--color-ink);
  }
</style>
