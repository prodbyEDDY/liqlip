<script lang="ts">
  // The root's overlay slot: renders the open layers bottom → top with the component
  // registered for their kind (`overlays.register`). Lives inside the panel, over the grid.
  import { overlays } from "../overlays.svelte";
</script>

{#each overlays.layers as layer (layer.id)}
  {@const Layer = overlays.component(layer.kind)}
  {#if Layer}
    <Layer {layer} close={() => overlays.close(layer)} />
  {/if}
{/each}
