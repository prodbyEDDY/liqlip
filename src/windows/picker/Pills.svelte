<script lang="ts">
  // Pills (Liquid Tools strip: 36 px capsules, track → ink when active, quiet counters,
  // horizontal scroll) — built-ins, then folders. Folders: "+" → inline name field (Enter /
  // Esc; "exists" explained inline), double click or the context menu → rename inline, drag a
  // folder pill to reorder (`folder_move`), delete from the context menu with a confirmation
  // (SPEC §3.9). While a card is dragged, Favorites and folders lift as drop targets.
  import Ico from "$lib/components/Ico.svelte";
  import { t, type TranslationKey } from "$lib/i18n/index.svelte";
  import { shelf, type FolderError } from "./shelf.svelte";
  import { overlays } from "./overlays.svelte";
  import { drag } from "./drag.svelte";
  import { folderDropBefore, type Pill } from "./pills";

  let strip = $state<HTMLDivElement | null>(null);

  // ── keep the active pill in view (Ctrl+Tab, Alt+N) ──
  $effect(() => {
    const id = shelf.pill;
    if (!strip) return;
    const el = strip.querySelector<HTMLElement>(`[data-pill="${CSS.escape(id)}"]`);
    el?.scrollIntoView({ block: "nearest", inline: "nearest" });
  });

  function onWheel(e: WheelEvent) {
    if (!strip || strip.scrollWidth <= strip.clientWidth) return;
    if (Math.abs(e.deltaX) >= Math.abs(e.deltaY)) return;
    strip.scrollLeft += e.deltaY;
    e.preventDefault();
  }

  const label = (p: Pill) => (p.builtin ? t(`picker.pill.${p.id}` as TranslationKey) : p.name);
  const errorText = (e: FolderError) => t(`picker.folder.error.${e}` as TranslationKey);

  // ── new folder ──
  let adding = $state(false);
  let addError = $state<FolderError | null>(null);
  let busy = false;

  async function commitNew(el: HTMLInputElement, onBlur = false) {
    if (busy) return;
    const name = el.value.trim();
    if (!name) {
      adding = false;
      addError = null;
      return;
    }
    busy = true;
    const err = await shelf.createFolder(name);
    busy = false;
    if (err) {
      addError = err;
      if (!onBlur) el.select();
      return;
    }
    adding = false;
    addError = null;
    shelf.focusSearch(false);
  }

  function onNewKey(e: KeyboardEvent) {
    const el = e.currentTarget as HTMLInputElement;
    if (e.key === "Enter") {
      e.preventDefault();
      e.stopPropagation();
      void commitNew(el);
    } else if (e.key === "Escape") {
      // Esc cancels the field — it must not also reach the window (which would close the shelf).
      e.preventDefault();
      e.stopPropagation();
      adding = false;
      addError = null;
      shelf.focusSearch(false);
    }
  }

  // ── rename ──
  let renameError = $state<FolderError | null>(null);

  async function commitRename(id: number, el: HTMLInputElement, onBlur = false) {
    if (shelf.renamingFolder !== id || busy) return;
    busy = true;
    const err = await shelf.renameFolder(id, el.value);
    busy = false;
    if (err && !onBlur) {
      renameError = err;
      el.select();
      return;
    }
    shelf.renamingFolder = null;
    renameError = null;
    if (!onBlur) shelf.focusSearch(false);
  }

  function onRenameKey(id: number, e: KeyboardEvent) {
    const el = e.currentTarget as HTMLInputElement;
    if (e.key === "Enter") {
      e.preventDefault();
      e.stopPropagation();
      void commitRename(id, el);
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      shelf.renamingFolder = null;
      renameError = null;
      shelf.focusSearch(false);
    }
  }

  function autofocus(el: HTMLInputElement) {
    el.focus();
    el.select();
  }

  // ── context menu ──
  function onContext(p: Pill, e: MouseEvent) {
    if (p.folderId === null) return;
    e.preventDefault();
    overlays.open("pillMenu", { folderId: p.folderId, anchor: { x: e.clientX, y: e.clientY } });
  }

  // ── reorder folders by dragging their pills ──
  let press: { id: number; x: number; y: number } | null = null;
  let reorder = $state<{ id: number; x: number; to: number } | null>(null);
  let suppressClick = false;

  function folderOrder(): number[] {
    return shelf.pills.filter((p) => p.folderId !== null).map((p) => p.folderId!);
  }

  function insertionIndex(x: number, dragged: number): number {
    const els = [...(strip?.querySelectorAll<HTMLElement>("[data-folder]") ?? [])].filter((el) => Number(el.dataset.folder) !== dragged);
    let i = 0;
    for (const el of els) {
      const r = el.getBoundingClientRect();
      if (x > r.left + r.width / 2) i++;
    }
    return i;
  }

  function onPillDown(p: Pill, e: PointerEvent) {
    if (p.folderId === null || e.button !== 0 || shelf.renamingFolder !== null) return;
    press = { id: p.folderId, x: e.clientX, y: e.clientY };
    window.addEventListener("pointermove", onPressMove);
    window.addEventListener("pointerup", onPressUp);
    window.addEventListener("pointercancel", endPress);
  }

  function onPressMove(e: PointerEvent) {
    if (!press) return;
    if (!reorder) {
      if (Math.abs(e.clientX - press.x) < 6 && Math.abs(e.clientY - press.y) < 6) return;
      reorder = { id: press.id, x: e.clientX, to: insertionIndex(e.clientX, press.id) };
      return;
    }
    reorder = { ...reorder, x: e.clientX, to: insertionIndex(e.clientX, reorder.id) };
    // Auto-scroll the strip near its edges.
    const r = strip?.getBoundingClientRect();
    if (strip && r) {
      if (e.clientX < r.left + 24) strip.scrollLeft -= 12;
      else if (e.clientX > r.right - 24) strip.scrollLeft += 12;
    }
  }

  function onPressUp() {
    const r = reorder;
    endPress();
    if (!r) return;
    suppressClick = true;
    const order = folderOrder();
    const beforeId = folderDropBefore(order, r.id, r.to);
    const currentBefore = folderDropBefore(order, r.id, order.indexOf(r.id));
    if (beforeId !== currentBefore) void shelf.moveFolder(r.id, beforeId);
  }

  function endPress() {
    press = null;
    reorder = null;
    window.removeEventListener("pointermove", onPressMove);
    window.removeEventListener("pointerup", onPressUp);
    window.removeEventListener("pointercancel", endPress);
  }

  $effect(() => () => endPress());

  function onPillClick(p: Pill) {
    if (suppressClick) {
      suppressClick = false;
      return;
    }
    shelf.setPill(p.id);
  }

  /** Marker position: before the `to`-th remaining folder pill. */
  const markerFolder = $derived.by(() => {
    if (!reorder) return null;
    const rest = folderOrder().filter((id) => id !== reorder!.id);
    return { before: rest[reorder.to] ?? null, last: rest[rest.length - 1] ?? null };
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="tabs" bind:this={strip} onwheel={onWheel} role="tablist" aria-label={t("picker.pills.label")}>
  {#each shelf.pills as pill (pill.id)}
    {@const droppable = drag.active && pill.droppable}
    {@const isRenaming = pill.folderId !== null && shelf.renamingFolder === pill.folderId}
    <div
      class="tabwrap"
      class:tabwrap--marker-before={markerFolder?.before === pill.folderId && pill.folderId !== null}
      class:tabwrap--marker-after={markerFolder && markerFolder.before === null && markerFolder.last === pill.folderId && pill.folderId !== null}
      data-folder={pill.folderId ?? undefined}
    >
      {#if isRenaming}
        <input
          class="pill-input selectable"
          type="text"
          maxlength="64"
          value={pill.name}
          aria-label={t("picker.folder.renameLabel")}
          aria-invalid={renameError !== null}
          use:autofocus
          onkeydown={(e) => onRenameKey(pill.folderId!, e)}
          oninput={() => (renameError = null)}
          onblur={(e) => void commitRename(pill.folderId!, e.currentTarget as HTMLInputElement, true)}
        />
        {#if renameError}
          <span class="pill-err" role="alert"><Ico name="warning" size={12} />{errorText(renameError)}</span>
        {/if}
      {:else}
        <button
          type="button"
          role="tab"
          aria-selected={shelf.pill === pill.id}
          class="pill"
          class:pill--active={shelf.pill === pill.id}
          class:pill--drop={droppable}
          class:pill--dropover={droppable && drag.over === pill.id}
          class:pill--dragging={reorder?.id === pill.folderId && pill.folderId !== null}
          data-pill={pill.id}
          data-drop={droppable ? pill.id : undefined}
          title={pill.folderId !== null ? t("picker.folder.hint") : undefined}
          onmousedown={(e) => e.preventDefault()}
          onpointerdown={(e) => onPillDown(pill, e)}
          onclick={() => onPillClick(pill)}
          ondblclick={() => {
            if (pill.folderId !== null) {
              renameError = null;
              shelf.renamingFolder = pill.folderId;
            }
          }}
          oncontextmenu={(e) => onContext(pill, e)}
        >
          {#if pill.id === "favorites"}<Ico name="heart" size={13} />{/if}
          {#if pill.folderId !== null}<Ico name="folder" size={13} />{/if}
          <span class="pill__label">{label(pill)}</span>
          {#if shelf.counts}<span class="pill__count">{pill.count}</span>{/if}
        </button>
      {/if}
    </div>
  {/each}

  {#if adding}
    <div class="tabwrap">
      <input
        class="pill-input selectable"
        type="text"
        maxlength="64"
        placeholder={t("picker.folder.placeholder")}
        aria-label={t("picker.folder.new")}
        aria-invalid={addError !== null}
        use:autofocus
        onkeydown={onNewKey}
        oninput={() => (addError = null)}
        onblur={(e) => void commitNew(e.currentTarget as HTMLInputElement, true)}
      />
      {#if addError}
        <span class="pill-err" role="alert"><Ico name="warning" size={12} />{errorText(addError)}</span>
      {/if}
    </div>
  {:else}
    <button
      type="button"
      class="pill pill--add"
      title={t("picker.folder.new")}
      aria-label={t("picker.folder.new")}
      onmousedown={(e) => e.preventDefault()}
      onclick={async () => {
        await shelf.makeInteractive();
        adding = true;
        addError = null;
      }}
    >
      <Ico name="plus" size={14} />
    </button>
  {/if}
</div>

<style>
  .tabs {
    display: flex;
    flex: 0 0 auto;
    flex-wrap: nowrap;
    align-items: center;
    gap: 6px;
    padding: 12px 0 10px;
    overflow-x: auto;
    scrollbar-width: none;
  }
  .tabwrap {
    position: relative;
    display: inline-flex;
    flex: none;
    align-items: center;
    gap: 6px;
  }
  .tabwrap--marker-before::before,
  .tabwrap--marker-after::after {
    content: "";
    position: absolute;
    top: 6px;
    bottom: 6px;
    width: 3px;
    border-radius: 999px;
    background: var(--color-ink);
  }
  .tabwrap--marker-before::before {
    left: -5px;
  }
  .tabwrap--marker-after::after {
    right: -5px;
  }
  .pill {
    display: inline-flex;
    flex: none;
    align-items: center;
    gap: 5px;
    height: 36px;
    padding: 0 11px;
    border: 0;
    border-radius: var(--radius-pill);
    background: var(--color-track);
    color: var(--color-ink-2);
    font: inherit;
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
    cursor: pointer;
    transition:
      background-color var(--dur-fast) var(--ease-out-soft),
      color var(--dur-fast) var(--ease-out-soft),
      transform var(--dur-fast) var(--ease-out-soft),
      opacity var(--dur-fast) var(--ease-out-soft);
  }
  .pill:hover {
    background: var(--color-hover);
    color: var(--color-ink);
  }
  .pill:active {
    transform: scale(0.97);
  }
  .pill--active,
  .pill--active:hover {
    background: var(--color-ink);
    color: var(--color-paper);
  }
  .pill:focus-visible {
    outline: 2px solid var(--color-ink);
    outline-offset: 2px;
  }
  .pill:disabled {
    opacity: 0.45;
  }
  /* Row count — quiet by design: it informs, never competes with the label. */
  .pill__count {
    color: var(--color-ink-3);
    font-variant-numeric: tabular-nums;
    font-weight: 500;
  }
  .pill--active .pill__count,
  .pill--dropover .pill__count {
    color: var(--color-paper);
    opacity: 0.6;
  }
  /* Drop target while a card is dragged: droppable pills lift slightly; the one under the
     cursor inverts to ink (the monochrome "armed" state). */
  .pill--drop {
    transform: scale(1.04);
  }
  .pill--dropover,
  .pill--dropover:hover {
    background: var(--color-ink);
    color: var(--color-paper);
  }
  .pill--dragging {
    opacity: 0.5;
  }
  .pill--add {
    justify-content: center;
    width: 36px;
    padding: 0;
    color: var(--color-ink-2);
  }
  .pill-input {
    width: 140px;
    height: 36px;
    padding: 0 12px;
    border: 0;
    border-radius: var(--radius-pill);
    background: var(--color-track);
    color: var(--color-ink);
    font: inherit;
    font-size: 12px;
    outline: none;
  }
  .pill-input:focus-visible,
  .pill-input:focus {
    outline: 2px solid var(--color-ink);
    outline-offset: 2px;
  }
  .pill-input::placeholder {
    color: var(--color-ink-3);
  }
  .pill-err {
    display: inline-flex;
    flex: none;
    align-items: center;
    gap: 5px;
    color: var(--color-ink-2);
    font-size: var(--text-meta);
    font-weight: 500;
    white-space: nowrap;
  }
  @media (prefers-reduced-motion: reduce) {
    .pill {
      transition: none;
    }
  }
</style>
