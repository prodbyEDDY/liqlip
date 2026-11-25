<script lang="ts">
  // The card grid: a virtualized listbox (only the viewport ± 300 px is in the DOM, plus the
  // current card), skeletons with the grid's own geometry (first load and the next page),
  // the next page pulled by an IntersectionObserver sentinel (one request in flight), the
  // "New items · Show" plaque, and the empty / error states. Vertical grid in the centre and
  // side panels; one horizontal row in the strip, where the wheel scrolls sideways.
  import Ico from "$lib/components/Ico.svelte";
  import Button from "$lib/components/Button.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import Card from "./Card.svelte";
  import SkeletonCard from "./SkeletonCard.svelte";
  import GridEmpty from "./GridEmpty.svelte";
  import { feed } from "./feed.svelte";
  import { selection } from "./selection.svelte";
  import { shelf } from "./shelf.svelte";
  import { keyboard } from "./keyboard.svelte";
  import { view } from "./view.svelte";
  import { geometry, tileRect, visibleRange } from "./layout";

  let scroller = $state<HTMLDivElement | null>(null);
  let sentinel = $state<HTMLDivElement | null>(null);

  $effect(() => {
    view.el = scroller;
    if (!scroller) return;
    const el = scroller;
    const ro = new ResizeObserver(() => view.measure());
    ro.observe(el);
    view.measure();
    return () => {
      ro.disconnect();
      if (view.el === el) view.el = null;
    };
  });
  // Axis change (layout switch) → re-measure on the next frame.
  $effect(() => {
    void view.horizontal;
    requestAnimationFrame(() => view.measure());
  });

  const g = $derived(view.geometry);
  /** Skeleton tiles after the last row while another page may follow. */
  const extra = $derived(
    feed.hasMore && !feed.moreFailed && feed.mode === "clips"
      ? g.horizontal
        ? 3
        : ((g.columns - (feed.count % g.columns)) % g.columns) + g.columns
      : 0,
  );
  /** Content size including the trailing skeleton row. */
  const box = $derived(extra ? geometry(feed.count + extra, view.width, view.height, view.horizontal) : g);
  const range = $derived(visibleRange(box, Math.max(0, view.offset - view.lead), view.extent));
  const indices = $derived.by(() => {
    const [start, end] = range;
    const out: number[] = [];
    for (let i = start; i < Math.min(end, feed.count); i++) out.push(i);
    // The current card stays mounted (aria-activedescendant, rename field) even off-screen.
    const cur = feed.indexOf(selection.current);
    if (cur >= 0 && (cur < start || cur >= end)) out.push(cur);
    return out;
  });
  const skeletonIndices = $derived.by(() => {
    const [start, end] = range;
    const out: number[] = [];
    for (let i = Math.max(start, feed.count); i < Math.min(end, feed.count + extra); i++) out.push(i);
    return out;
  });
  /** First-load skeleton: enough tiles to fill the viewport. */
  const loadingTiles = $derived.by(() => {
    const sk = geometry(1, view.width, view.height, view.horizontal);
    const n = sk.horizontal
      ? Math.max(1, Math.ceil(view.width / sk.strideX))
      : sk.columns * Math.max(1, Math.ceil((view.height + 10) / sk.strideY));
    const lg = geometry(n, view.width, view.height, view.horizontal);
    return { g: lg, list: Array.from({ length: n }, (_, i) => i) };
  });
  const digits = $derived.by(() => {
    if (!keyboard.ctrlHeld) return null;
    void view.offset;
    void feed.count;
    return new Map(view.onScreen().map((index, n) => [index, n + 1]));
  });

  // Overflow cues: the scroller fades the edge(s) that hide more cards.
  const fadeStart = $derived(view.offset > 2);
  const fadeEnd = $derived.by(() => {
    const end = (view.horizontal ? box.width + view.lead * 2 : box.height) - view.extent;
    return feed.status === "ready" && (view.offset < end - 2 || feed.hasMore);
  });

  function onScroll() {
    view.measure();
    const end = view.horizontal ? box.width : box.height;
    if (feed.hasMore && view.offset + view.extent + 320 >= end) void feed.loadMore();
  }

  // The plain wheel only emits deltaY, which a horizontal scroller ignores: translate it in
  // the strip (Shift+wheel and horizontal swipes already arrive as deltaX).
  function onWheel(e: WheelEvent) {
    if (!view.horizontal || !scroller) return;
    if (scroller.scrollWidth <= scroller.clientWidth) return;
    if (Math.abs(e.deltaX) >= Math.abs(e.deltaY)) return;
    scroller.scrollLeft += e.deltaY;
    e.preventDefault();
  }

  $effect(() => {
    const root = scroller;
    const target = sentinel;
    if (!root || !target) return;
    const io = new IntersectionObserver((hits) => {
      if (hits.some((h) => h.isIntersecting)) void feed.loadMore();
    }, { root, rootMargin: "320px" });
    io.observe(target);
    return () => io.disconnect();
  });

  const tileStyle = (x: number, y: number, w: number, h: number) =>
    `left:${x + view.lead}px;top:${y}px;width:${w}px;height:${h}px`;
</script>

<div class="grid-area">
  {#if shelf.pendingNew}
    <button type="button" class="plaque" onclick={() => shelf.showNew()}>
      <Ico name="arrowUp" size={14} />
      <span>{t("picker.newItems")}</span>
      <span class="plaque__dot" aria-hidden="true">·</span>
      <span class="plaque__show">{t("picker.newItems.show")}</span>
    </button>
  {/if}

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="list-wrap"
    class:list-wrap--row={view.horizontal}
    class:list-wrap--fade-start={fadeStart}
    class:list-wrap--fade-end={fadeEnd}
    bind:this={scroller}
    onscroll={onScroll}
    onwheel={onWheel}
  >
    {#if feed.status === "loading" || (feed.status === "idle" && shelf.open)}
      <div class="virtual" style:width={view.horizontal ? `${loadingTiles.g.width + view.lead * 2}px` : "100%"} style:height={`${loadingTiles.g.height}px`} aria-busy="true" aria-label={t("common.loading")}>
        {#each loadingTiles.list as i (i)}
          {@const r = tileRect(loadingTiles.g, i)}
          <div class="tile" style={tileStyle(r.x, r.y, r.width, r.height)}><SkeletonCard /></div>
        {/each}
      </div>
    {:else if feed.status === "error" || (feed.status === "ready" && feed.count === 0)}
      <GridEmpty />
    {:else if feed.status === "ready"}
      <div
        class="virtual"
        id="picker-grid"
        role="listbox"
        tabindex="-1"
        aria-label={t("picker.list.label")}
        aria-multiselectable={feed.mode === "clips"}
        aria-orientation={view.horizontal ? "horizontal" : "vertical"}
        style:width={view.horizontal ? `${box.width + view.lead * 2}px` : "100%"}
        style:height={`${box.height}px`}
      >
        {#each indices as i (feed.idAt(i))}
          {@const r = tileRect(g, i)}
          <div class="tile" role="presentation" style={tileStyle(r.x, r.y, r.width, r.height)}>
            {#if feed.mode === "snippets"}
              <Card snippet={feed.snippets[i]} digit={digits?.get(i) ?? null} />
            {:else}
              <Card clip={feed.items[i]} digit={digits?.get(i) ?? null} />
            {/if}
          </div>
        {/each}
        {#each skeletonIndices as i (`sk${i}`)}
          {@const r = tileRect(box, i)}
          <div class="tile" aria-hidden="true" style={tileStyle(r.x, r.y, r.width, r.height)}><SkeletonCard /></div>
        {/each}
        {#if feed.hasMore}
          {@const r = tileRect(box, feed.count)}
          <div class="sentinel" bind:this={sentinel} style={`left:${r.x + view.lead}px;top:${r.y}px`} aria-hidden="true"></div>
        {/if}
        {#if feed.moreFailed}
          {@const r = tileRect(box, feed.count)}
          <div class="more-failed" style={view.horizontal ? `left:${r.x + view.lead}px;top:0;height:${g.previewH}px` : `left:0;right:0;top:${r.y}px`}>
            <span>{t("picker.error.more")}</span>
            <Button variant="secondary" size="sm" icon="refresh" onclick={() => void feed.loadMore()}>{t("common.retry")}</Button>
          </div>
        {/if}
      </div>
    {/if}
  </div>
</div>

<style>
  .grid-area {
    position: relative;
    display: flex;
    flex: 1 1 auto;
    flex-direction: column;
    min-height: 0;
  }
  /* Vertical grid (centre / side panels). The scroller reaches the panel edges (negative
     margin = panel padding) so hover lifts and shadows are never cut at an invisible line:
     12 px of padding on the left, the 12 px scrollbar gutter on the right, 4 px on top for
     the lift and 8 px at the bottom for the shadow. Edges that hide cards fade (mask). */
  .list-wrap {
    --fade: 16px;
    flex: 1 1 auto;
    min-height: 0;
    margin: 0 calc(-1 * var(--pad-panel));
    padding: 4px 0 8px var(--pad-panel);
    overflow-x: hidden;
    overflow-y: auto;
    scrollbar-gutter: stable;
  }
  .list-wrap--fade-start {
    mask-image: linear-gradient(to bottom, transparent 0, #000 var(--fade), #000 100%);
  }
  .list-wrap--fade-end {
    mask-image: linear-gradient(to bottom, #000 0, #000 calc(100% - var(--fade)), transparent 100%);
  }
  .list-wrap--fade-start.list-wrap--fade-end {
    mask-image: linear-gradient(to bottom, transparent 0, #000 var(--fade), #000 calc(100% - var(--fade)), transparent 100%);
  }
  /* Strip (top / bottom, centre "one row"): one horizontal row, flush to the panel's side
     edges; the leading inset lives inside the content so it scrolls away. No scrollbar —
     the wheel scrolls sideways and the edges fade where cards continue. */
  .list-wrap--row {
    --fade: 28px;
    overflow-x: auto;
    overflow-y: hidden;
    min-width: 0;
    padding: 4px 0 8px;
    scrollbar-gutter: auto;
    scrollbar-width: none;
  }
  .list-wrap--row.list-wrap--fade-start {
    mask-image: linear-gradient(to right, transparent 0, #000 var(--fade), #000 100%);
  }
  .list-wrap--row.list-wrap--fade-end {
    mask-image: linear-gradient(to right, #000 0, #000 calc(100% - var(--fade)), transparent 100%);
  }
  .list-wrap--row.list-wrap--fade-start.list-wrap--fade-end {
    mask-image: linear-gradient(to right, transparent 0, #000 var(--fade), #000 calc(100% - var(--fade)), transparent 100%);
  }
  .virtual {
    position: relative;
    min-height: 1px;
    outline: none;
  }
  .tile {
    position: absolute;
    border-radius: var(--r-card);
    transition: transform var(--dur-fast) var(--ease-out-soft);
  }
  /* The current card lifts 2 px (transform: neighbours never move). */
  .tile:has(> :global(.card.is-current)) {
    z-index: 1;
    transform: translateY(-2px);
  }
  .tile:has(> :global(.card.is-dragging)) {
    transform: none;
  }
  .sentinel {
    position: absolute;
    width: 1px;
    height: 1px;
  }
  .more-failed {
    position: absolute;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 12px;
    height: 48px;
    color: var(--color-ink-2);
    font-size: var(--text-secondary);
  }
  /* "New items · Show" — an ink capsule floating over the top of the grid. */
  .plaque {
    position: absolute;
    top: 8px;
    left: 50%;
    z-index: 4;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: var(--h-md);
    padding: 0 14px 0 12px;
    border: 0;
    border-radius: 999px;
    background: var(--color-ink);
    color: var(--color-paper);
    font: inherit;
    font-size: var(--text-secondary);
    font-weight: 600;
    box-shadow: var(--shadow-pop);
    transform: translateX(-50%);
    animation: plaque-in var(--dur-base) var(--ease-out-soft);
    transition: background-color var(--dur-fast) var(--ease-out-soft);
  }
  .plaque:hover {
    background: var(--color-ink-hover);
  }
  .plaque:active {
    background: var(--color-ink-active);
  }
  .plaque__dot {
    opacity: 0.6;
  }
  .plaque__show {
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  @keyframes plaque-in {
    from {
      opacity: 0;
      transform: translate(-50%, -6px);
    }
  }
</style>
