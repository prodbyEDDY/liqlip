<script lang="ts">
  // One open layer in `OverlayHost`: renders its registered component and tells the kit how
  // deep in the stack it is (z-order of its scrim and surface).
  import { setContext } from "svelte";
  import { overlays, type Layer } from "../overlays.svelte";
  import { LAYER_DEPTH } from "./kit/depth";

  interface Props {
    layer: Layer;
    depth: number;
  }
  let { layer, depth }: Props = $props();

  setContext(LAYER_DEPTH, () => depth);
  const Component = $derived(overlays.component(layer.kind));
</script>

{#if Component}
  <Component {layer} close={() => overlays.close(layer)} />
{/if}
