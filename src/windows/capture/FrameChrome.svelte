<script lang="ts">
  // Stage-2 frame chrome (ARCHITECTURE §18.7): eight handles OUTSIDE the frame edge (so drawing
  // inside never grabs one — LT defects A/B) and the size chip above it, which also moves the
  // frame. The move band itself is the shade around the frame (`Shade.svelte`, `hitFrame`).
  import type { Rect } from "$lib/ipc/types";
  import { t } from "$lib/i18n/index.svelte";
  import { HANDLE_CURSOR, handleCenters, visibleHandles, type Handle, type Size } from "./geometry";
  import { placeChip } from "./dock";

  interface Props {
    /** the frame, view px */
    frame: Rect;
    view: Size;
    /** "W × H" (frame px), `null` — sizes are off in Settings */
    size: string | null;
    /** the docked panel (view px) — the chip keeps off it */
    avoid: Rect | null;
    busy: boolean;
    onhandle: (h: Handle, e: PointerEvent) => void;
    onchip: (e: PointerEvent) => void;
  }
  let { frame, view, size, avoid, busy, onhandle, onchip }: Props = $props();

  let chipW = $state(0);
  let chipH = $state(0);
  const centers = $derived(handleCenters(frame, view));
  const handles = $derived(visibleHandles(frame));
  const chip = $derived(placeChip(frame, { w: chipW, h: chipH }, view, avoid));
</script>

{#each handles as h (h)}
  <div
    class="handle"
    style="left:{centers[h].x}px; top:{centers[h].y}px; cursor:{busy ? 'wait' : HANDLE_CURSOR[h]}"
    onpointerdown={(e) => onhandle(h, e)}
    aria-hidden="true"
  ></div>
{/each}
{#if size}
  <div
    class="chip"
    class:chip--measuring={!chipW}
    style="left:{chip.x}px; top:{chip.y}px; cursor:{busy ? 'wait' : 'move'}"
    title={t("capture.frame.move")}
    bind:clientWidth={chipW}
    bind:clientHeight={chipH}
    onpointerdown={onchip}
    aria-hidden="true"
  >
    {size}
  </div>
{/if}

<style>
  /* 10 px dot in a 16 px hit target (HANDLE_R 8) */
  .handle {
    position: fixed;
    z-index: 4;
    width: 16px;
    height: 16px;
    margin: -8px 0 0 -8px;
    border-radius: 999px;
    touch-action: none;
  }
  .handle::after {
    content: "";
    position: absolute;
    inset: 3px;
    border-radius: 999px;
    background: var(--color-white);
    box-shadow:
      0 0 0 1px color-mix(in srgb, var(--color-black) 35%, transparent),
      0 1px 3px color-mix(in srgb, var(--color-black) 35%, transparent);
    transition: transform var(--dur-fast) var(--ease-out-soft);
  }
  .handle:hover::after {
    transform: scale(1.25);
  }
  .chip {
    position: fixed;
    z-index: 4;
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
    touch-action: none;
    user-select: none;
  }
  .chip:hover {
    background: color-mix(in srgb, var(--color-ink) 6.5%, var(--color-raised));
  }
  .chip--measuring {
    visibility: hidden;
  }
</style>
