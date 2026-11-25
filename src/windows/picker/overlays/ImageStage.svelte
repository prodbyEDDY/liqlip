<script lang="ts">
  // The image in Quick Look: fitted into its well (never upscaled past 2×), zoom with the
  // wheel (around the pointer), +/−/0 (the parent forwards keys: `zoomIn` / `zoomOut` /
  // `fit`), double click toggles fit ↔ actual pixels, drag pans while zoomed. Pixels show
  // crisp from 200 %. The zoom capsule sits in the well's corner, inset 4 (16 − 4 = 12 → 24 px).
  import Ico from "$lib/components/Ico.svelte";
  import { t } from "$lib/i18n/index.svelte";

  interface Props {
    src: string;
    alt: string;
    /** Natural size when known (the store has it); otherwise read from the loaded image. */
    width?: number | null;
    height?: number | null;
  }
  let { src, alt, width = null, height = null }: Props = $props();

  const PAD = 8;
  const MAX_SCALE = 8;

  let box = $state<HTMLDivElement | null>(null);
  let bw = $state(0);
  let bh = $state(0);
  let natW = $state(0);
  let natH = $state(0);
  let zoom = $state(1);
  let tx = $state(0);
  let ty = $state(0);
  let dragging = $state(false);
  let failed = $state(false);

  // The store's size is the image's real pixel size (the percentage is relative to it); the
  // loaded image's own size is the fallback.
  const nw = $derived(width || natW || 0);
  const nh = $derived(height || natH || 0);
  const fit = $derived(nw && nh && bw && bh ? Math.min((bw - 2 * PAD) / nw, (bh - 2 * PAD) / nh, 2) : 1);
  const scale = $derived(fit * zoom);
  const maxZoom = $derived(Math.max(1, MAX_SCALE / fit));
  const percent = $derived(Math.round(scale * 100));

  // A new image starts fitted.
  $effect(() => {
    void src;
    zoom = 1;
    tx = 0;
    ty = 0;
    natW = 0;
    natH = 0;
    failed = false;
  });

  const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

  function pan(x: number, y: number, s: number): [number, number] {
    const mx = Math.max(0, (nw * s - bw) / 2 + PAD);
    const my = Math.max(0, (nh * s - bh) / 2 + PAD);
    return [clamp(x, -mx, mx), clamp(y, -my, my)];
  }

  /** Zoom to `next` (× fit) keeping the point (px, py) — relative to the well's centre — still. */
  function zoomAt(next: number, px = 0, py = 0) {
    const z = clamp(next, 1, maxZoom);
    const k = (fit * z) / scale;
    const [x, y] = pan(px - (px - tx) * k, py - (py - ty) * k, fit * z);
    zoom = z;
    tx = z === 1 ? 0 : x;
    ty = z === 1 ? 0 : y;
  }

  export function zoomIn(): void {
    zoomAt(zoom * 1.5);
  }
  export function zoomOut(): void {
    zoomAt(zoom / 1.5);
  }
  export function fitToView(): void {
    zoomAt(1);
  }

  function centre(e: { clientX: number; clientY: number }): [number, number] {
    const r = box!.getBoundingClientRect();
    return [e.clientX - (r.left + r.width / 2), e.clientY - (r.top + r.height / 2)];
  }

  // Wheel zoom needs a non-passive listener (it replaces scrolling).
  $effect(() => {
    const el = box;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const [px, py] = centre(e);
      zoomAt(zoom * Math.exp(-e.deltaY * 0.0016), px, py);
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  });

  let last: { x: number; y: number } | null = null;
  function onpointerdown(e: PointerEvent) {
    if (e.button !== 0 || zoom <= 1) return;
    dragging = true;
    last = { x: e.clientX, y: e.clientY };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }
  function onpointermove(e: PointerEvent) {
    if (!dragging || !last) return;
    const [x, y] = pan(tx + e.clientX - last.x, ty + e.clientY - last.y, scale);
    tx = x;
    ty = y;
    last = { x: e.clientX, y: e.clientY };
  }
  function onpointerup() {
    dragging = false;
    last = null;
  }
  function ondblclick(e: MouseEvent) {
    const [px, py] = centre(e);
    if (zoom > 1) zoomAt(1);
    else zoomAt(Math.max(1.0001, 1 / fit), px, py); // actual pixels
  }
</script>

<div
  class="stage checker"
  class:stage--zoomed={zoom > 1}
  class:stage--dragging={dragging}
  bind:this={box}
  bind:clientWidth={bw}
  bind:clientHeight={bh}
  role="presentation"
  {onpointerdown}
  {onpointermove}
  {onpointerup}
  onpointercancel={onpointerup}
  {ondblclick}
>
  {#if failed}
    <span class="stage__fail"><Ico name="image" size={22} />{t("picker.card.noPreview")}</span>
  {:else}
    <img
      {src}
      {alt}
      draggable="false"
      width={nw || undefined}
      height={nh || undefined}
      class:pixelated={scale >= 2}
      style:transform="translate(-50%, -50%) translate({tx}px, {ty}px) scale({scale})"
      onload={(e) => {
        const img = e.currentTarget as HTMLImageElement;
        natW = img.naturalWidth;
        natH = img.naturalHeight;
      }}
      onerror={() => (failed = true)}
    />
  {/if}
  <div class="zoom" role="toolbar" aria-label={t("picker.ql.zoomFit")}>
    <button type="button" title={t("picker.ql.zoomOut")} aria-label={t("picker.ql.zoomOut")} disabled={zoom <= 1} onmousedown={(e) => e.preventDefault()} onclick={zoomOut}>
      <Ico name="minus" size={13} />
    </button>
    <button type="button" class="zoom__pct" title={t("picker.ql.zoomFit")} onmousedown={(e) => e.preventDefault()} onclick={fitToView}>{percent}%</button>
    <button type="button" title={t("picker.ql.zoomIn")} aria-label={t("picker.ql.zoomIn")} disabled={zoom >= maxZoom} onmousedown={(e) => e.preventDefault()} onclick={zoomIn}>
      <Ico name="plus" size={13} />
    </button>
  </div>
</div>

<style>
  /* The well clips the zoomed image to its own squircle (content, not a shadow or a ring). */
  .stage {
    position: relative;
    flex: 1 1 auto;
    min-height: 0;
    overflow: hidden;
    border-radius: var(--layer-r-in);
    touch-action: none;
  }
  .checker {
    background-color: var(--color-paper);
    background-image: conic-gradient(var(--color-checker) 25%, transparent 0 50%, var(--color-checker) 0 75%, transparent 0);
    background-size: 16px 16px;
  }
  .stage--zoomed {
    cursor: grab;
  }
  .stage--dragging {
    cursor: grabbing;
  }
  img {
    position: absolute;
    top: 50%;
    left: 50%;
    max-width: none;
    transform-origin: center;
    transition: transform 90ms var(--ease-out-soft);
    user-select: none;
  }
  .stage--dragging img {
    transition: none;
  }
  img.pixelated {
    image-rendering: pixelated;
  }
  .stage__fail {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
  }
  .zoom {
    position: absolute;
    right: 4px;
    bottom: 4px;
    display: flex;
    align-items: center;
    height: var(--h-sm);
    border-radius: 999px;
    background: var(--color-raised);
    box-shadow: var(--shadow-pop);
    opacity: 0.94;
  }
  .zoom button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: var(--h-sm);
    height: var(--h-sm);
    padding: 0;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: var(--color-ink-2);
    font: inherit;
    transition:
      background-color var(--dur-fast) var(--ease-out-soft),
      color var(--dur-fast) var(--ease-out-soft);
  }
  .zoom button:hover:not(:disabled) {
    background: var(--color-track);
    color: var(--color-ink);
  }
  .zoom button:disabled {
    opacity: 0.4;
  }
  .zoom button:focus-visible {
    outline-offset: -2px;
  }
  .zoom .zoom__pct {
    min-width: 44px;
    color: var(--color-ink);
    font-family: var(--font-mono);
    font-size: var(--text-meta);
    font-variant-numeric: tabular-nums;
  }
</style>
