<script lang="ts">
  // The generic `menu` layer: `overlays.open("menu", { anchor, items, label })` — a small menu
  // of `MenuEntry` rows built by the caller (e.g. the separator choice of the paste stack).
  import MenuLayer, { type MenuEntry } from "./MenuLayer.svelte";
  import type { LayerProps } from "../overlays.svelte";

  let { layer, close }: LayerProps = $props();
  const items = $derived((layer.ctx.items as MenuEntry[] | undefined) ?? []);
  const label = $derived((layer.ctx.label as string | undefined) ?? "");
</script>

<MenuLayer {layer} {close} {items} {label} />
