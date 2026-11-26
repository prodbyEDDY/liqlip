<script lang="ts">
  // The magnifier (SPEC §9.2; algorithm of LT `Capture.svelte:1560-1635` — ShareX): a circle of
  // frame pixels around the cursor, nearest-neighbour, with a pixel grid, a crosshair one cell
  // thick and the centre pixel outlined; the caption shows the position or the area size. It
  // repaints when the cursor pixel, the zoom or the frame changes — never in a loop. The lens
  // chrome is white/black: the only colour is the user's screen.
  import type { Point, Size } from "./geometry";
  import { placeLoupe } from "./dock";
  import { loupeCount, loupeSource } from "./loupe";

  interface Props {
    bitmap: ImageBitmap;
    /** the frame pixel under the cursor */
    pixel: Point;
    /** the cursor, view px */
    at: Point;
    view: Size;
    /** view px per frame pixel */
    cell: number;
    /** backing-store pixels per view px (frame.width / innerWidth) */
    density: number;
    caption: string;
  }
  let { bitmap, pixel, at, view, cell, density, caption }: Props = $props();

  const CAPTION_H = 24;
  const CAPTION_GAP = 6;
  let canvas = $state<HTMLCanvasElement | null>(null);

  const count = $derived(loupeCount(cell));
  const span = $derived(count * cell);
  const pos = $derived(placeLoupe(at, { w: span, h: span + CAPTION_GAP + CAPTION_H }, view));

  $effect(() => {
    if (!canvas) return;
    const g = canvas.getContext("2d");
    if (!g) return;
    const px = Math.round(span * density);
    if (canvas.width !== px) {
      canvas.width = px;
      canvas.height = px;
    }
    const src = loupeSource({ x: Math.min(pixel.x, bitmap.width - 1), y: Math.min(pixel.y, bitmap.height - 1) }, count);
    g.setTransform(density, 0, 0, density, 0, 0);
    g.clearRect(0, 0, span, span);
    g.save();
    g.beginPath();
    g.arc(span / 2, span / 2, span / 2 - 1, 0, Math.PI * 2);
    g.clip();
    // cells off the frame show the lens background
    g.fillStyle = "#111111";
    g.fillRect(0, 0, span, span);
    g.imageSmoothingEnabled = false;
    // a source rect past the image edge is clipped with the destination in proportion (HTML
    // spec, drawImage) — the frame's edge lands exactly on the grid
    g.drawImage(bitmap, src.x, src.y, count, count, 0, 0, span, span);
    // crosshair: four arms one cell thick up to (not over) the centre pixel
    const arm = (span - cell) / 2;
    g.fillStyle = "rgb(255 255 255 / 0.42)";
    g.fillRect(0, arm, arm, cell);
    g.fillRect(arm + cell, arm, arm, cell);
    g.fillRect(arm, 0, cell, arm);
    g.fillRect(arm, arm + cell, cell, arm);
    // pixel grid
    g.strokeStyle = "rgb(0 0 0 / 0.3)";
    g.lineWidth = 1 / density;
    g.beginPath();
    for (let i = 1; i < count; i++) {
      const p = i * cell;
      g.moveTo(p, 0);
      g.lineTo(p, span);
      g.moveTo(0, p);
      g.lineTo(span, p);
    }
    g.stroke();
    // the centre pixel: black just outside, white just inside (when the cell is big enough)
    g.lineWidth = 1;
    g.strokeStyle = "#000000";
    g.strokeRect(arm - 0.5, arm - 0.5, cell + 1, cell + 1);
    if (cell >= 6) {
      g.strokeStyle = "#ffffff";
      g.strokeRect(arm + 0.5, arm + 0.5, cell - 1, cell - 1);
    }
    g.restore();
    // rings: white outside, black inside
    g.beginPath();
    g.arc(span / 2, span / 2, span / 2 - 0.5, 0, Math.PI * 2);
    g.strokeStyle = "#ffffff";
    g.stroke();
    g.beginPath();
    g.arc(span / 2, span / 2, span / 2 - 1.5, 0, Math.PI * 2);
    g.strokeStyle = "#000000";
    g.stroke();
  });
</script>

<div class="loupe" style="left:{pos.x}px; top:{pos.y}px; width:{span}px" aria-hidden="true">
  <canvas bind:this={canvas} class="loupe__lens" style="width:{span}px; height:{span}px"></canvas>
  <span class="loupe__caption">{caption}</span>
</div>

<style>
  .loupe {
    position: fixed;
    z-index: 6;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    pointer-events: none;
  }
  .loupe__lens {
    display: block;
    border-radius: 999px;
    box-shadow: var(--shadow-pop);
  }
  .loupe__caption {
    display: inline-flex;
    align-items: center;
    height: var(--h-sm);
    padding: 0 10px;
    border-radius: 999px;
    background: var(--color-raised);
    box-shadow: var(--shadow-pop);
    color: var(--color-ink);
    font-family: var(--font-mono);
    font-size: var(--text-meta);
    font-weight: 500;
    white-space: nowrap;
  }
</style>
