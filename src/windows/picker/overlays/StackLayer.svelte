<script lang="ts">
  // W1 stand-in for the paste-stack popover ("Stack N" in the hint row): the ordered items,
  // remove one, Clear, Paste all. W2-C registers the full popover (reorder by drag, reverse)
  // over this kind: `overlays.register("stack", StackPopover)`.
  import Ico from "$lib/components/Ico.svelte";
  import { stackClear, stackPasteAll, stackRemove } from "$lib/ipc/commands";
  import { errorMessage } from "$lib/ipc/errors";
  import { t, type TranslationKey } from "$lib/i18n/index.svelte";
  import type { IconName } from "$lib/icons";
  import type { Kind, MergeSeparator } from "$lib/ipc/types";
  import type { LayerProps } from "../overlays.svelte";
  import { shelf } from "../shelf.svelte";
  import { toasts } from "../toasts.svelte";

  let { layer, close }: LayerProps = $props();

  const KIND_ICON: Record<Kind, IconName> = {
    text: "text",
    link: "link",
    code: "code",
    color: "palette",
    contact: "at",
    image: "image",
    file: "file",
  };
  const SEPARATORS: Record<MergeSeparator, string> = { newline: "\n", space: " ", tab: "\t", none: "" };

  const items = $derived(shelf.stack.items);

  async function run<T>(f: () => Promise<T>): Promise<T | undefined> {
    try {
      return await f();
    } catch (e) {
      toasts.error(errorMessage(e));
      return undefined;
    }
  }

  async function remove(key: string) {
    const s = await run(() => stackRemove(key));
    if (s) shelf.stack = s;
  }

  async function clear() {
    const s = await run(() => stackClear());
    if (s) shelf.stack = s;
  }

  async function pasteAll() {
    close();
    const o = await run(() => stackPasteAll(SEPARATORS[shelf.settings?.picker.mergeSeparator ?? "newline"]));
    if (o?.result === "failed") toasts.error(t(`picker.outcome.${o.reason ?? "failed"}` as TranslationKey));
  }

  $effect(() => {
    layer.onKey = (e: KeyboardEvent) => {
      if (e.key === "Enter" && items.length) {
        void pasteAll();
        return true;
      }
      return false;
    };
  });
</script>

<div class="scrim" role="presentation" onpointerdown={close}></div>
<div class="pop squircle" role="dialog" aria-label={t("picker.stack.title")}>
  <header class="pop__head">
    <Ico name="stack" size={15} />
    <span class="pop__title">{t("picker.stack.title")}</span>
    <span class="pop__count">{items.length}</span>
  </header>
  {#if items.length === 0}
    <p class="pop__empty">{t("picker.stack.empty")}</p>
  {:else}
    <ol class="list">
      {#each items as item, i (item.key)}
        <li class="row">
          <span class="row__n">{i + 1}</span>
          <Ico name={KIND_ICON[item.kind]} size={14} />
          <span class="row__text">{item.preview}</span>
          <button type="button" class="row__x" aria-label={t("picker.stack.remove")} title={t("picker.stack.remove")} onclick={() => void remove(item.key)}>
            <Ico name="x" size={12} />
          </button>
        </li>
      {/each}
    </ol>
  {/if}
  <footer class="pop__foot">
    <button type="button" class="btn btn--ghost" disabled={items.length === 0} onclick={() => void clear()}>{t("picker.stack.clear")}</button>
    <button type="button" class="btn btn--primary" disabled={items.length === 0} onclick={() => void pasteAll()}>
      <Ico name="paste" size={14} />{t("picker.stack.pasteAll")}
    </button>
  </footer>
</div>

<style>
  .scrim {
    position: absolute;
    inset: 0;
    z-index: 40;
  }
  .pop {
    position: absolute;
    right: 14px;
    bottom: 40px;
    z-index: 41;
    display: flex;
    flex-direction: column;
    width: min(320px, calc(100% - 28px));
    max-height: min(360px, calc(100% - 100px));
    padding: 8px;
    border-radius: 16px;
    background: var(--color-paper);
    box-shadow: var(--shadow-pop);
    animation: pop-in var(--dur-base) var(--ease-out-soft);
  }
  @keyframes pop-in {
    from {
      opacity: 0;
      transform: translateY(6px) scale(0.98);
    }
  }
  .pop__head {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    padding: 0 6px;
    color: var(--color-ink-2);
  }
  .pop__title {
    flex: 1 1 auto;
    color: var(--color-ink);
    font-size: var(--text-row);
    font-weight: 600;
  }
  .pop__count {
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
    font-variant-numeric: tabular-nums;
  }
  .pop__empty {
    margin: 0;
    padding: 12px 8px;
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    list-style: none;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 32px;
    padding: 0 4px 0 8px;
    border-radius: 10px;
    color: var(--color-ink-3);
  }
  .row:hover {
    background: var(--color-hover);
  }
  .row__n {
    width: 16px;
    color: var(--color-ink-3);
    font-family: var(--font-mono);
    font-size: var(--text-meta);
    text-align: right;
  }
  .row__text {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    color: var(--color-ink);
    font-size: var(--text-secondary);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .row__x {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: var(--color-ink-3);
    cursor: pointer;
  }
  .row__x:hover {
    background: var(--color-selected);
    color: var(--color-ink);
  }
  .row__x:focus-visible,
  .btn:focus-visible {
    outline: 2px solid var(--color-ink);
    outline-offset: 1px;
  }
  .pop__foot {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
    padding-top: 8px;
  }
  .btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    padding: 0 12px;
    border: 0;
    border-radius: 999px;
    font: inherit;
    font-size: var(--text-secondary);
    font-weight: 600;
    cursor: pointer;
    transition:
      background-color var(--dur-fast) var(--ease-out-soft),
      transform var(--dur-fast) var(--ease-out-soft);
  }
  .btn:active:not(:disabled) {
    transform: scale(0.97);
  }
  .btn:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .btn--ghost {
    background: transparent;
    color: var(--color-ink-2);
  }
  .btn--ghost:hover:not(:disabled) {
    background: var(--color-hover);
    color: var(--color-ink);
  }
  .btn--primary {
    background: var(--color-ink);
    color: var(--color-paper);
  }
  .btn--primary:hover:not(:disabled) {
    background: var(--color-ink-hover);
  }
  .btn--primary:active:not(:disabled) {
    background: var(--color-ink-active);
  }
</style>
