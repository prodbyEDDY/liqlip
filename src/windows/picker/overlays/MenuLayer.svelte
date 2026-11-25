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
    /** Draw a gap before this entry (groups, without a divider line). */
    gapBefore?: boolean;
    run: () => void;
  }
</script>

<script lang="ts">
  // A small context menu anchored at a viewport point, clamped inside the panel — built on
  // the layer kit (LayerSurface `menu` + LayerItem). ↑/↓ move, Enter runs, Esc / a click
  // outside closes. Used by the W1 actions stub and the folder pill menu; W2-C's searchable
  // actions menu replaces the `actions` layer.
  import type { Layer } from "../overlays.svelte";
  import { LayerItem, LayerSurface } from "./kit";

  interface Props {
    layer: Layer;
    close: () => void;
    items: MenuEntry[];
    label: string;
  }
  let { layer, close, items, label }: Props = $props();

  let active = $state(0);
  const enabled = $derived(items.filter((i) => !i.disabled));

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
      if (e.key === "Home" || e.key === "End") {
        active = e.key === "Home" ? 0 : Math.max(0, enabled.length - 1);
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

<LayerSurface variant="menu" anchor={layer.ctx.anchor ?? null} {label} onclose={close}>
  {#each items as entry (entry.id)}
    {#if entry.gapBefore}<span class="gap" aria-hidden="true"></span>{/if}
    <LayerItem
      label={entry.label}
      icon={entry.icon}
      keys={entry.keys}
      danger={entry.danger}
      disabled={entry.disabled}
      active={enabled[active]?.id === entry.id}
      onpointermove={() => {
        const i = enabled.findIndex((x) => x.id === entry.id);
        if (i >= 0) active = i;
      }}
      onclick={() => run(entry)}
    />
  {/each}
</LayerSurface>

<style>
  .gap {
    display: block;
    flex: none;
    height: 6px;
  }
</style>
