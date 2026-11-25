<script lang="ts">
  // The paste-stack popover (SPEC §3.11) — "Stack N" in the hint row, or "Show paste stack" in
  // the actions menu. Docked above the hint row at the panel's right edge.
  //   · "Collect new copies" switch (`capture_set_collect`)
  //   · the ordered list: drag a row to reorder (`stack_move`), Alt+↑/↓ with the keyboard,
  //     × / Delete removes one
  //   · Reverse order · Clear (undo from the toast — the stack lives in memory, so undo adds
  //     the same records back in the same order)
  //   · Paste all with the remembered separator (Enter); the separator is chosen next to it
  // Lives on `stack:changed` (shelf.stack) and `capture:status` (shelf.capture).
  import { tick } from "svelte";
  import Ico from "$lib/components/Ico.svelte";
  import Button from "$lib/components/Button.svelte";
  import { LayerFooter, LayerHeader, LayerItem, LayerSurface } from "./kit";
  import { captureSetCollect, stackAdd, stackClear, stackMove, stackPasteAll, stackRemove, stackReverse } from "$lib/ipc/commands";
  import { errorMessage } from "$lib/ipc/errors";
  import { t, type TranslationKey } from "$lib/i18n/index.svelte";
  import type { IconName } from "$lib/icons";
  import type { Kind, MergeSeparator, Stack } from "$lib/ipc/types";
  import { overlays, type LayerProps } from "../overlays.svelte";
  import { shelf } from "../shelf.svelte";
  import { toasts } from "../toasts.svelte";
  import { reportOutcome } from "../actions.svelte";
  import { SEPARATOR_TEXT, separatorPref } from "./separator.svelte";
  import { SEPARATORS } from "./menuModel";
  import type { MenuEntry } from "./MenuLayer.svelte";
  import { focusOnce, trapTab } from "./focus";

  let { layer, close }: LayerProps = $props();
  const uid = $props.id();

  const KIND_ICON: Record<Kind, IconName> = {
    text: "text",
    link: "link",
    code: "code",
    color: "palette",
    contact: "at",
    image: "image",
    file: "file",
  };
  const SEP_GLYPH: Record<MergeSeparator, string> = { newline: "↵", space: "␣", tab: "⇥", none: "∅" };
  const SEP_ICON: Record<MergeSeparator, IconName> = { newline: "text", space: "minus", tab: "arrowRight", none: "x" };

  const items = $derived(shelf.stack.items);
  const collecting = $derived(shelf.capture.collectToStack);
  let active = $state(0);
  let surface = $state<HTMLDivElement | null>(null);
  let listEl = $state<HTMLOListElement | null>(null);

  $effect(() => {
    if (active > items.length - 1) active = Math.max(0, items.length - 1);
  });

  const focused = { v: false };
  $effect(() => focusOnce(surface, focused));
  // The "added to the stack · Show" toast would sit over the popover's bottom edge.
  toasts.dismiss();

  async function apply(f: () => Promise<Stack>): Promise<boolean> {
    try {
      shelf.stack = await f();
      return true;
    } catch (e) {
      toasts.error(errorMessage(e));
      return false;
    }
  }

  function remove(key: string) {
    shelf.stack = { items: items.filter((i) => i.key !== key) };
    void apply(() => stackRemove(key));
  }

  /** Move the item at `from` so it ends up at index `to`. */
  function moveItem(from: number, to: number) {
    if (from === to || from < 0 || to < 0 || to >= items.length) return;
    const list = items.slice();
    const [it] = list.splice(from, 1);
    list.splice(to, 0, it);
    shelf.stack = { items: list };
    active = to;
    void apply(() => stackMove(it.key, list[to + 1]?.key ?? null));
    void tick().then(reveal);
  }

  async function clear() {
    const before = items.map((i) => i.clipId);
    if (before.length === 0) return;
    if (await apply(() => stackClear())) {
      toasts.show(t("picker.stack.cleared"), {
        icon: "stack",
        action: { label: t("common.undo"), run: () => void apply(() => stackAdd(before)) },
      });
    }
  }

  async function pasteAll() {
    if (items.length === 0) return;
    close();
    try {
      const o = await stackPasteAll(SEPARATOR_TEXT[separatorPref.value]);
      reportOutcome(o, false);
    } catch (e) {
      toasts.error(errorMessage(e));
    }
  }

  async function toggleCollect() {
    try {
      shelf.capture = await captureSetCollect(!collecting);
    } catch (e) {
      toasts.error(errorMessage(e));
    }
  }

  function chooseSeparator(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const entries: MenuEntry[] = SEPARATORS.map((s) => ({
      id: s,
      label: t(`picker.menu.sep.${s}` as TranslationKey),
      icon: SEP_ICON[s],
      checked: s === separatorPref.value,
      run: () => separatorPref.set(s),
    }));
    overlays.open("menu", { anchor: { x: r.left, y: r.top }, items: entries, label: t("picker.stack.separator") });
  }

  function reveal() {
    listEl?.querySelector<HTMLElement>(`[data-index="${active}"]`)?.scrollIntoView({ block: "nearest" });
  }

  // ── drag to reorder (pointer events; no HTML5 DnD inside the shelf) ──
  let drag = $state<{ key: string; from: number; startY: number; moved: boolean; target: number } | null>(null);

  function onRowDown(e: PointerEvent, index: number) {
    if (e.button !== 0 || (e.target as HTMLElement).closest("button")) return;
    active = index;
    drag = { key: items[index].key, from: index, startY: e.clientY, moved: false, target: index };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function onRowMove(e: PointerEvent) {
    if (!drag) return;
    if (!drag.moved && Math.abs(e.clientY - drag.startY) < 4) return;
    const rows = [...(listEl?.querySelectorAll<HTMLElement>("[data-index]") ?? [])];
    let target = rows.length;
    for (let j = 0; j < rows.length; j++) {
      const r = rows[j].getBoundingClientRect();
      if (e.clientY < r.top + r.height / 2) {
        target = j;
        break;
      }
    }
    drag = { ...drag, moved: true, target };
  }

  function onRowUp() {
    const d = drag;
    drag = null;
    if (!d?.moved) return;
    const to = d.target > d.from ? d.target - 1 : d.target;
    moveItem(d.from, to);
  }

  /** Where the insertion marker shows: before row i, or after the last one. */
  const marker = $derived.by(() => {
    if (!drag?.moved) return null;
    const { from, target } = drag;
    if (target === from || target === from + 1) return null;
    return target;
  });

  // ── keyboard ──
  $effect(() => {
    layer.onKey = (e: KeyboardEvent) => {
      if (trapTab(e, surface)) return true;
      const plain = !e.ctrlKey && !e.metaKey;
      if (plain && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
        const d = e.key === "ArrowDown" ? 1 : -1;
        if (e.altKey) moveItem(active, active + d);
        else if (items.length) {
          active = (active + d + items.length) % items.length;
          void tick().then(reveal);
        }
        return true;
      }
      if (plain && !e.altKey && (e.key === "Home" || e.key === "End")) {
        active = e.key === "Home" ? 0 : Math.max(0, items.length - 1);
        void tick().then(reveal);
        return true;
      }
      if (plain && !e.altKey && (e.key === "Delete" || e.key === "Backspace")) {
        if (!e.repeat && items[active]) remove(items[active].key);
        return true;
      }
      if ((e.key === "Enter" || e.code === "NumpadEnter") && plain && !e.altKey && !e.shiftKey) {
        if (document.activeElement instanceof HTMLButtonElement) return false;
        if (!e.repeat) void pasteAll();
        return true;
      }
      return false;
    };
  });
</script>

<LayerSurface variant="popover" dock="bottom-end" width="min(372px, calc(100% - 24px))" labelledby="{uid}-title" bind:el={surface} onclose={close}>
  {#snippet header()}
    <LayerHeader title={t("picker.stack.title")} id="{uid}-title" icon="stack" count={items.length} onclose={close}>
      {#snippet tools()}
        <Button
          variant="ghost"
          size="sm"
          icon="reverse"
          disabled={items.length < 2}
          title={t("picker.stack.reverse")}
          aria-label={t("picker.stack.reverse")}
          onmousedown={(e: MouseEvent) => e.preventDefault()}
          onclick={() => void apply(() => stackReverse())}
        />
        <Button
          variant="ghost"
          size="sm"
          icon="trash"
          disabled={items.length === 0}
          title={t("picker.stack.clear")}
          aria-label={t("picker.stack.clear")}
          onmousedown={(e: MouseEvent) => e.preventDefault()}
          onclick={() => void clear()}
        />
      {/snippet}
    </LayerHeader>
  {/snippet}

  <div class="collect">
    <LayerItem label={t("picker.stack.collect")} hint={t("picker.stack.collectHint")} icon="clipboardList" toggle={collecting} onclick={() => void toggleCollect()} />
  </div>

  {#if items.length === 0}
    <div class="empty">
      <span class="empty__icon" aria-hidden="true"><Ico name="stack" size={18} /></span>
      <p class="empty__title">{t("picker.outcome.stack_empty")}</p>
      <p class="empty__hint">{t("picker.stack.emptyHint")}</p>
    </div>
  {:else}
    <ol class="list" bind:this={listEl} aria-label={t("picker.stack.list")} title={t("picker.stack.dragHint")}>
      {#each items as item, i (item.key)}
        <li
          class="row"
          class:row--active={i === active}
          class:row--dragging={drag?.moved && drag.key === item.key}
          class:row--drop-before={marker === i}
          class:row--drop-after={marker === items.length && i === items.length - 1}
          data-index={i}
          onpointerdown={(e) => onRowDown(e, i)}
          onpointermove={onRowMove}
          onpointerup={onRowUp}
          onpointercancel={() => (drag = null)}
        >
          <LayerItem label={item.preview || t(`common.kind.${item.kind}` as TranslationKey)} plain active={i === active}>
            {#snippet lead()}<span class="row__n">{i + 1}</span>{/snippet}
            {#snippet trail()}<span class="row__kind" title={t(`common.kind.${item.kind}` as TranslationKey)}><Ico name={KIND_ICON[item.kind]} size={14} /></span>{/snippet}
          </LayerItem>
          <button
            type="button"
            class="row__x"
            aria-label={t("picker.stack.remove")}
            title={t("picker.stack.remove")}
            onmousedown={(e) => e.preventDefault()}
            onclick={() => remove(item.key)}
          >
            <Ico name="x" size={13} />
          </button>
        </li>
      {/each}
    </ol>
    {#if items.length > 1}<p class="caption">{t("picker.stack.dragHint")}</p>{/if}
  {/if}

  {#snippet footer()}
    <LayerFooter>
      {#snippet actions()}
        <button type="button" class="sep" title={t("picker.stack.separator")} aria-haspopup="menu" onmousedown={(e) => e.preventDefault()} onclick={chooseSeparator}>
          <span class="sep__glyph" aria-hidden="true">{SEP_GLYPH[separatorPref.value]}</span>
          <span class="sep__label">{t(`picker.menu.sep.${separatorPref.value}` as TranslationKey)}</span>
          <Ico name="chevronDown" size={13} />
        </button>
        <Button variant="primary" size="sm" icon="paste" disabled={items.length === 0} onclick={() => void pasteAll()}>{t("picker.stack.pasteAll")}</Button>
      {/snippet}
    </LayerFooter>
  {/snippet}
</LayerSurface>

<style>
  .collect {
    margin-top: 4px;
  }
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 14px 16px 10px;
    text-align: center;
  }
  .empty__icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 40px;
    height: 40px;
    margin-bottom: 4px;
    border-radius: 999px;
    background: var(--color-track);
    color: var(--color-ink-2);
  }
  .empty__title {
    margin: 0;
    color: var(--color-ink);
    font-size: var(--text-row);
    font-weight: 600;
  }
  .empty__hint {
    margin: 0;
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
    line-height: var(--text-secondary--line-height);
    text-wrap: pretty;
  }
  .list {
    display: flex;
    flex: 1 1 auto;
    flex-direction: column;
    min-height: 0;
    margin: 6px 0 0;
    padding: 0;
    overflow-y: auto;
    list-style: none;
  }
  .row {
    position: relative;
    display: flex;
    flex: none;
    align-items: center;
    border-radius: var(--layer-r-in);
    cursor: grab;
    touch-action: none;
  }
  .row :global(.item) {
    padding-right: 40px;
  }
  .row:hover :global(.item) {
    background: var(--color-track);
  }
  .row--dragging {
    opacity: 0.45;
    cursor: grabbing;
  }
  /* Insertion marker: a 3 px ink capsule at the gap (the only line-like mark — it's a cursor,
     like the favorites order marker on cards). */
  .row--drop-before::before,
  .row--drop-after::after {
    position: absolute;
    left: 10px;
    right: 10px;
    z-index: 1;
    height: 3px;
    border-radius: 999px;
    background: var(--color-ink);
    content: "";
  }
  .row--drop-before::before {
    top: -2px;
  }
  .row--drop-after::after {
    bottom: -2px;
  }
  .row__n {
    min-width: 18px;
    color: var(--color-ink-3);
    font-family: var(--font-mono);
    font-size: var(--text-meta);
    font-variant-numeric: tabular-nums;
    text-align: center;
  }
  .row__kind {
    display: inline-flex;
    flex: 0 0 auto;
    color: var(--color-ink-3);
  }
  /* Remove: a 24 px circle inset 4 px in the 32 px row (16 − 4 = 12, concentric). */
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
  .row--active .row__x,
  .row__x:focus-visible {
    opacity: 1;
  }
  .row__x:hover {
    background: var(--color-hover);
    color: var(--color-ink);
  }
  .row__x:focus-visible {
    outline-offset: -2px;
  }
  .caption {
    margin: 6px 0 0;
    padding: 0 10px;
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
    line-height: var(--text-secondary--line-height);
  }

  /* Separator choice: a ghost capsule next to "Paste all" (a split control). */
  .sep {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    max-width: 180px;
    height: var(--h-md);
    padding: 0 10px 0 12px;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: var(--color-ink-2);
    font: inherit;
    font-size: var(--text-secondary);
    font-weight: 600;
    transition:
      background-color var(--dur-fast) var(--ease-out-soft),
      color var(--dur-fast) var(--ease-out-soft);
  }
  .sep:hover {
    background: var(--color-track);
    color: var(--color-ink);
  }
  .sep:active {
    background: var(--color-hover);
  }
  .sep__glyph {
    color: var(--color-ink-3);
    font-family: var(--font-mono);
  }
  .sep__label {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
