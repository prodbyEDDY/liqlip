<script lang="ts">
  // W1 stand-in for the paste-stack popover ("Stack N" in the hint row): the ordered items,
  // remove one, Clear, Paste all. W2-C registers the full popover (reorder by drag, reverse)
  // over this kind: `overlays.register("stack", StackPopover)`.
  import Ico from "$lib/components/Ico.svelte";
  import Button from "$lib/components/Button.svelte";
  import { LayerFooter, LayerHeader, LayerItem, LayerSurface } from "./kit";
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

<LayerSurface variant="popover" dock="bottom-end" label={t("picker.stack.title")} onclose={close}>
  {#snippet header()}
    <LayerHeader title={t("picker.stack.title")} icon="stack" count={items.length} />
  {/snippet}
  {#if items.length === 0}
    <p class="empty">{t("picker.stack.empty")}</p>
  {:else}
    <ol class="list">
      {#each items as item, i (item.key)}
        <li class="row">
          <LayerItem label={item.preview} plain>
            {#snippet lead()}<span class="row__n">{i + 1}</span>{/snippet}
            {#snippet trail()}<span class="row__kind"><Ico name={KIND_ICON[item.kind]} size={14} /></span>{/snippet}
          </LayerItem>
          <button type="button" class="row__x" aria-label={t("picker.stack.remove")} title={t("picker.stack.remove")} onmousedown={(e) => e.preventDefault()} onclick={() => void remove(item.key)}>
            <Ico name="x" size={13} />
          </button>
        </li>
      {/each}
    </ol>
  {/if}
  {#snippet footer()}
    <LayerFooter>
      {#snippet actions()}
        <Button variant="ghost" size="sm" disabled={items.length === 0} onclick={() => void clear()}>{t("picker.stack.clear")}</Button>
        <Button variant="primary" size="sm" icon="paste" disabled={items.length === 0} onclick={() => void pasteAll()}>{t("picker.stack.pasteAll")}</Button>
      {/snippet}
    </LayerFooter>
  {/snippet}
</LayerSurface>

<style>
  .empty {
    margin: 0;
    padding: 10px 10px 12px;
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
    line-height: var(--text-secondary--line-height);
  }
  .list {
    display: flex;
    flex-direction: column;
    min-height: 0;
    margin: 4px 0 0;
    padding: 0;
    overflow-y: auto;
    list-style: none;
  }
  .row {
    position: relative;
    display: flex;
    align-items: center;
  }
  .row :global(.item) {
    padding-right: 40px;
  }
  .row:hover :global(.item) {
    background: var(--color-track);
  }
  .row__n {
    color: var(--color-ink-3);
    font-family: var(--font-mono);
    font-size: var(--text-meta);
    font-variant-numeric: tabular-nums;
  }
  .row__kind {
    display: inline-flex;
    flex: 0 0 auto;
    color: var(--color-ink-3);
  }
  .row__x {
    position: absolute;
    top: 4px;
    right: 4px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--h-sm);
    height: var(--h-sm);
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: var(--color-ink-3);
    opacity: 0;
    transition:
      opacity var(--dur-fast) var(--ease-out-soft),
      background-color var(--dur-fast) var(--ease-out-soft);
  }
  .row:hover .row__x,
  .row__x:focus-visible {
    opacity: 1;
  }
  .row__x:hover {
    background: var(--color-hover);
    color: var(--color-ink);
  }
</style>
