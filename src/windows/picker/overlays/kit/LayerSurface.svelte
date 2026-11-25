<script lang="ts">
  // The surface every shelf layer is built on (W2-E kit; W2-C builds Quick Look, the actions
  // menu and the stack popover from it). It draws the click-catching scrim and one raised
  // squircle surface inside the panel, positions it, and publishes the concentric geometry
  // to its content through CSS variables:
  //   --layer-r   the surface radius      --layer-pad  its padding
  //   --layer-r-in = radius − padding     (rows, wells, buttons in the corners use it)
  //
  // Variants (radius = 16 + padding, so corner content is a 32 px capsule or an r16 well):
  //   menu     r22 · pad 6   anchored at a viewport point, clamped inside the panel
  //   popover  r24 · pad 8   anchored (or docked bottom-end above the hint row)
  //   sheet    r24 · pad 8   covers the panel, inset 4 px (concentric with the panel)
  //   dialog   r28 · pad 12  centred, dim scrim
  import { getContext, onMount, type Snippet } from "svelte";
  import { shelf } from "../../shelf.svelte";
  import { LAYER_DEPTH } from "./depth";

  interface Props {
    variant?: "menu" | "popover" | "sheet" | "dialog";
    /** Viewport point for `menu` / `popover` (context menu, the ⋯ button). */
    anchor?: { x: number; y: number } | null;
    /** `popover` without an anchor: dock to the bottom-right corner above the hint row. */
    dock?: "bottom-end" | "bottom-start" | null;
    /** Dim the panel behind the layer (default: sheet / dialog dim, menu / popover don't). */
    dim?: boolean;
    role?: string;
    label?: string;
    labelledby?: string;
    describedby?: string;
    modal?: boolean;
    /** Fixed width (px or CSS length); menus size to content within min/max. */
    width?: number | string;
    /** Scrim click / right click → close. */
    onclose: () => void;
    /** The surface element (focus management, measuring). */
    el?: HTMLDivElement | null;
    header?: Snippet;
    footer?: Snippet;
    children: Snippet;
  }
  let {
    variant = "menu",
    anchor = null,
    dock = null,
    dim = variant === "sheet" || variant === "dialog",
    role = variant === "menu" ? "menu" : "dialog",
    label,
    labelledby,
    describedby,
    modal = variant === "sheet" || variant === "dialog",
    width,
    onclose,
    el = $bindable(null),
    header,
    footer,
    children,
  }: Props = $props();

  const anchored = $derived((variant === "menu" || variant === "popover") && !dock);
  /** Depth in the layer stack (OverlayHost): each layer's scrim covers the ones below. */
  const depth = getContext<(() => number) | undefined>(LAYER_DEPTH);
  const z = $derived(40 + 2 * (depth?.() ?? 0));
  let pos = $state<{ x: number; y: number } | null>(null);

  onMount(() => {
    if (!anchored) return;
    const panel = shelf.panelEl?.getBoundingClientRect();
    const box = el?.getBoundingClientRect();
    const pad = 8;
    if (!panel || !box) {
      pos = { x: pad, y: pad };
      return;
    }
    const a = anchor ?? { x: panel.left + 40, y: panel.top + 120 };
    let x = a.x - panel.left;
    let y = a.y - panel.top;
    if (x + box.width > panel.width - pad) x = panel.width - pad - box.width;
    // Doesn't fit below the point: flip above it if there's room, otherwise slide up just
    // enough to fit (rather than jumping to the panel's top edge).
    if (y + box.height > panel.height - pad) {
      const above = y - box.height - 8;
      y = above >= pad ? above : panel.height - pad - box.height;
    }
    pos = { x: Math.max(pad, x), y: Math.max(pad, y) };
  });

  const style = $derived.by(() => {
    const parts: string[] = [`z-index:${z + 1}`];
    if (width !== undefined) parts.push(`width:${typeof width === "number" ? `${width}px` : width}`);
    // Before it is measured and placed the surface is transparent, not hidden: its content
    // can take focus right away (a menu's filter field focuses on mount).
    if (anchored) parts.push(pos ? `left:${pos.x}px;top:${pos.y}px` : "left:0;top:0;opacity:0");
    return parts.join(";");
  });
</script>

<div
  class="scrim"
  class:scrim--dim={dim}
  style:z-index={z}
  role="presentation"
  onpointerdown={onclose}
  oncontextmenu={(e) => {
    e.preventDefault();
    onclose();
  }}
></div>
<div
  bind:this={el}
  class="layer layer--{variant} squircle"
  class:layer--dock-end={dock === "bottom-end"}
  class:layer--dock-start={dock === "bottom-start"}
  class:layer--placed={!anchored || pos !== null}
  {role}
  aria-label={label}
  aria-labelledby={labelledby}
  aria-describedby={describedby}
  aria-modal={modal || undefined}
  tabindex="-1"
  {style}
>
  {#if header}{@render header()}{/if}
  {@render children()}
  {#if footer}{@render footer()}{/if}
</div>

<style>
  .scrim {
    position: absolute;
    inset: 0;
    z-index: 40;
    border-radius: inherit;
  }
  .scrim--dim {
    background: var(--color-scrim);
    animation: fade-in var(--dur-base) var(--ease-out-soft);
  }
  @keyframes fade-in {
    from {
      opacity: 0;
    }
  }

  .layer {
    --layer-r: var(--r-layer);
    --layer-pad: var(--pad-layer);
    --layer-r-in: max(4px, calc(var(--layer-r) - var(--layer-pad)));
    position: absolute;
    z-index: 41;
    display: flex;
    flex-direction: column;
    min-height: 0;
    padding: var(--layer-pad);
    border-radius: var(--layer-r);
    background: var(--color-raised);
    box-shadow: var(--shadow-pop);
    color: var(--color-ink);
    outline: none;
  }
  .layer:focus-visible {
    outline: none;
  }
  .layer--placed {
    animation: layer-in var(--dur-base) var(--ease-out-soft);
  }
  @keyframes layer-in {
    from {
      opacity: 0;
      transform: translateY(-4px) scale(0.98);
    }
  }

  .layer--menu {
    min-width: 220px;
    max-width: min(320px, calc(100% - 16px));
    max-height: calc(100% - 16px);
    overflow-y: auto;
  }

  .layer--popover {
    --layer-r: var(--r-popover);
    --layer-pad: var(--pad-popover);
    width: min(340px, calc(100% - 24px));
    max-height: calc(100% - 96px);
  }
  /* Docked above the hint row, in the panel's padding grid (right / left edge = 12 px). */
  .layer--dock-end,
  .layer--dock-start {
    bottom: calc(var(--pad-panel) + var(--h-md) + 4px);
  }
  .layer--dock-end {
    right: var(--pad-panel);
    transform-origin: bottom right;
  }
  .layer--dock-start {
    left: var(--pad-panel);
    transform-origin: bottom left;
  }
  .layer--dock-end.layer--placed,
  .layer--dock-start.layer--placed {
    animation-name: layer-up;
  }
  @keyframes layer-up {
    from {
      opacity: 0;
      transform: translateY(6px) scale(0.98);
    }
  }

  .layer--sheet {
    --layer-r: var(--r-sheet);
    --layer-pad: var(--pad-sheet);
    inset: var(--inset-sheet);
    box-shadow: var(--shadow-card-lg);
    animation-name: sheet-in;
    animation-duration: var(--dur-slow);
    animation-timing-function: var(--ease-liquid);
  }
  @keyframes sheet-in {
    from {
      opacity: 0;
      transform: scale(0.97);
    }
  }

  .layer--dialog {
    --layer-r: var(--r-dialog);
    --layer-pad: var(--pad-dialog);
    top: 50%;
    left: 50%;
    width: min(360px, calc(100% - 32px));
    max-height: calc(100% - 32px);
    box-shadow: var(--shadow-card-lg);
    transform: translate(-50%, -50%);
    animation-name: dialog-in;
    animation-duration: var(--dur-slow);
    animation-timing-function: var(--ease-liquid);
  }
  @keyframes dialog-in {
    from {
      opacity: 0;
      transform: translate(-50%, -50%) scale(0.95);
    }
  }
</style>
