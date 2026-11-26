<script lang="ts">
  // The overlay's dim (SPEC §9.2: ~20 % around the target) with a clear hole — the hovered
  // window or the area being drawn (stage 1), the frame (stage 2). One SVG path with an even-odd
  // hole, so adjacent boxes never leave seams at fractional coordinates and nothing repaints per
  // frame: the hole moves only when the target changes (no "marching ants" rAF loop — LT defect
  // П-20). In stage 2 four transparent boxes around the hole are the frame's "outside" and take
  // the pointer (handles, band, a new area); the hole lets events through to the editor below.
  // The ring marks the hole on top of the user's screen — a selection marker, not interface
  // chrome, so it is white + a dark edge to read on any picture.
  import type { Rect } from "$lib/ipc/types";
  import type { Size } from "./geometry";

  interface Props {
    /** view px; `null` — dim everything */
    hole: Rect | null;
    view: Size;
    /** `hover` — stage-1 target (ring inside the hole); `frame` — stage-2 frame (ring outside) */
    ring?: "hover" | "frame" | null;
    /** dim strength (0…1) */
    dim?: number;
    /** the outside takes the pointer (stage 2) */
    interactive?: boolean;
    cursor?: string;
    onpointerdown?: (e: PointerEvent) => void;
    onpointermove?: (e: PointerEvent) => void;
  }
  let { hole, view, ring = null, dim = 0.2, interactive = false, cursor = "default", onpointerdown, onpointermove }: Props = $props();

  const outer = $derived(`M0 0H${view.w}V${view.h}H0Z`);
  const path = $derived(hole ? `${outer}M${hole.x} ${hole.y}h${hole.w}v${hole.h}h${-hole.w}Z` : outer);

  /** Hit boxes around the hole (stage 2 only; transparent). */
  const boxes = $derived.by(() => {
    if (!interactive || !hole) return [];
    const x0 = Math.max(0, hole.x);
    const y0 = Math.max(0, hole.y);
    const x1 = Math.min(view.w, hole.x + hole.w);
    const y1 = Math.min(view.h, hole.y + hole.h);
    return [
      { x: 0, y: 0, w: view.w, h: y0 },
      { x: 0, y: y1, w: view.w, h: view.h - y1 },
      { x: 0, y: y0, w: x0, h: y1 - y0 },
      { x: x1, y: y0, w: view.w - x1, h: y1 - y0 },
    ].filter((b) => b.w > 0 && b.h > 0);
  });
</script>

<svg class="shade" width={view.w} height={view.h} viewBox="0 0 {view.w} {view.h}" aria-hidden="true">
  <path d={path} fill-rule="evenodd" fill="currentColor" fill-opacity={dim} />
</svg>
{#each boxes as b, i (i)}
  <div class="hit" style="left:{b.x}px; top:{b.y}px; width:{b.w}px; height:{b.h}px; cursor:{cursor}" {onpointerdown} {onpointermove} aria-hidden="true"></div>
{/each}
{#if hole && ring}
  <div class="ring ring--{ring}" style="left:{hole.x}px; top:{hole.y}px; width:{hole.w}px; height:{hole.h}px" aria-hidden="true"></div>
{/if}

<style>
  .shade {
    position: fixed;
    inset: 0;
    z-index: 2;
    color: var(--color-black);
    pointer-events: none;
  }
  .hit {
    position: fixed;
    z-index: 2;
    touch-action: none;
  }
  .ring {
    position: fixed;
    z-index: 3;
    pointer-events: none;
  }
  /* stage 1: inside the hole, so a window filling the monitor still shows its outline */
  .ring--hover {
    box-shadow:
      inset 0 0 0 2px var(--color-white),
      inset 0 0 0 3px color-mix(in srgb, var(--color-black) 45%, transparent);
  }
  /* stage 2: just outside the frame — the edge pixels of the shot stay visible */
  .ring--frame {
    box-shadow:
      0 0 0 1px var(--color-white),
      0 0 0 2px color-mix(in srgb, var(--color-black) 45%, transparent);
  }
</style>
