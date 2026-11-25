<script lang="ts" module>
  import type { IconName } from "$lib/icons";
  export interface MenuEntry {
    id: string;
    label: string;
    icon: IconName;
    /** Shortcut shown on the right ("Ctrl D"). */
    keys?: string;
    danger?: boolean;
    disabled?: boolean;
    run: () => void;
  }
</script>

<script lang="ts">
  // A small context menu anchored at a viewport point, clamped inside the panel. ↑/↓ move,
  // Enter runs, Esc / a click outside closes. Used by the W1 actions stub and the folder pill
  // menu; W2-C's searchable actions menu replaces the `actions` layer.
  import { onMount } from "svelte";
  import Ico from "$lib/components/Ico.svelte";
  import type { Layer } from "../overlays.svelte";
  import { shelf } from "../shelf.svelte";

  interface Props {
    layer: Layer;
    close: () => void;
    items: MenuEntry[];
    label: string;
  }
  let { layer, close, items, label }: Props = $props();

  let menu = $state<HTMLDivElement | null>(null);
  let pos = $state({ x: 0, y: 0, ready: false });
  let active = $state(0);
  const enabled = $derived(items.filter((i) => !i.disabled));

  onMount(() => {
    const panel = shelf.panelEl?.getBoundingClientRect();
    const box = menu?.getBoundingClientRect();
    const anchor = layer.ctx.anchor ?? { x: (panel?.left ?? 0) + 40, y: (panel?.top ?? 0) + 120 };
    if (!panel || !box) {
      pos = { x: 40, y: 120, ready: true };
      return;
    }
    const pad = 8;
    let x = anchor.x - panel.left;
    let y = anchor.y - panel.top;
    if (x + box.width > panel.width - pad) x = Math.max(pad, panel.width - pad - box.width);
    if (y + box.height > panel.height - pad) y = Math.max(pad, y - box.height - 8);
    pos = { x: Math.max(pad, x), y: Math.max(pad, y), ready: true };
  });

  function run(entry: MenuEntry) {
    if (entry.disabled) return;
    close();
    entry.run();
  }

  // Keys for the top layer (the root routes them here first).
  $effect(() => {
    layer.onKey = (e: KeyboardEvent) => {
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        const n = enabled.length;
        if (n) active = (active + (e.key === "ArrowDown" ? 1 : -1) + n) % n;
        return true;
      }
      if (e.key === "Enter" || e.key === " ") {
        const entry = enabled[active];
        if (entry) run(entry);
        return true;
      }
      if (e.key === "Tab") return true;
      return false;
    };
  });
</script>

<div class="scrim" role="presentation" onpointerdown={close} oncontextmenu={(e) => { e.preventDefault(); close(); }}></div>
<div
  bind:this={menu}
  class="menu squircle"
  class:menu--ready={pos.ready}
  role="menu"
  aria-label={label}
  tabindex="-1"
  style={`left:${pos.x}px;top:${pos.y}px`}
>
  {#each items as entry (entry.id)}
    {@const isActive = enabled[active]?.id === entry.id}
    <button
      type="button"
      role="menuitem"
      class="item"
      class:item--active={isActive}
      class:item--danger={entry.danger}
      disabled={entry.disabled}
      onmousedown={(e) => e.preventDefault()}
      onpointermove={() => {
        const i = enabled.findIndex((x) => x.id === entry.id);
        if (i >= 0) active = i;
      }}
      onclick={() => run(entry)}
    >
      <Ico name={entry.icon} size={15} />
      <span class="item__label">{entry.label}</span>
      {#if entry.keys}<span class="item__keys">{entry.keys}</span>{/if}
    </button>
  {/each}
</div>

<style>
  .scrim {
    position: absolute;
    inset: 0;
    z-index: 40;
  }
  .menu {
    position: absolute;
    z-index: 41;
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 220px;
    max-width: 300px;
    padding: 6px;
    border-radius: 14px;
    background: var(--color-paper);
    box-shadow: var(--shadow-pop);
    opacity: 0;
    outline: none;
  }
  .menu--ready {
    opacity: 1;
    animation: menu-in var(--dur-base) var(--ease-out-soft);
  }
  @keyframes menu-in {
    from {
      opacity: 0;
      transform: translateY(-4px) scale(0.97);
      filter: blur(2px);
    }
  }
  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 34px;
    padding: 0 10px;
    border: 0;
    border-radius: 10px;
    background: transparent;
    color: var(--color-ink);
    font: inherit;
    font-size: var(--text-row);
    font-weight: 500;
    text-align: left;
    cursor: pointer;
  }
  .item :global(.ico) {
    color: var(--color-ink-3);
  }
  .item--active {
    background: var(--color-hover);
  }
  .item:active:not(:disabled) {
    background: var(--color-selected);
  }
  .item:focus-visible {
    outline: 2px solid var(--color-ink);
    outline-offset: -2px;
  }
  .item:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .item--danger,
  .item--danger :global(.ico) {
    color: var(--color-danger);
  }
  .item__label {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .item__keys {
    flex: 0 0 auto;
    color: var(--color-ink-3);
    font-family: var(--font-mono);
    font-size: var(--text-meta);
  }
</style>
