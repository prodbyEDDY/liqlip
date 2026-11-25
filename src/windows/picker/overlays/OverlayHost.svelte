<script lang="ts">
  // The root's overlay slot: renders the open layers bottom → top with the component
  // registered for their kind (`overlays.register`). Lives inside the panel, over the grid.
  // When the last layer closes and focus went with it (the layer's element left the DOM),
  // focus returns to the search field — the shelf's home (typing, arrows, Enter work again).
  // A layer action that focuses something itself (F2 rename, a folder name) keeps it.
  import { tick } from "svelte";
  import { overlays } from "../overlays.svelte";
  import { shelf } from "../shelf.svelte";

  $effect(() => {
    if (overlays.layers.length > 0) return;
    void tick().then(() => {
      if (overlays.layers.length > 0 || !shelf.open) return;
      const active = document.activeElement;
      if (!active || active === document.body || !shelf.panelEl?.contains(active)) shelf.focusSearch(false);
    });
  });
</script>

{#each overlays.layers as layer (layer.id)}
  {@const Layer = overlays.component(layer.kind)}
  {#if Layer}
    <Layer {layer} close={() => overlays.close(layer)} />
  {/if}
{/each}
