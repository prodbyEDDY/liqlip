<script lang="ts">
  // The root's overlay slot: renders the open layers bottom → top with the component
  // registered for their kind (`overlays.register`). Lives inside the panel, over the grid.
  // When a layer closes and focus went with it (its element left the DOM), focus returns to
  // the layer below (its surface), or — after the last one — to the search field, the shelf's
  // home (typing, arrows, Enter work again). A layer action that focuses something itself
  // (F2 rename, a folder name, the editor after "Keep editing") keeps it.
  import { tick } from "svelte";
  import { overlays } from "../overlays.svelte";
  import { shelf } from "../shelf.svelte";
  import LayerSlot from "./LayerSlot.svelte";

  let count = 0;
  $effect(() => {
    const n = overlays.layers.length;
    const closed = n < count;
    const opened = n > count;
    count = n;
    // A hover-opened (passive) shelf doesn't own the keyboard: a layer opened by the mouse
    // (⋯, right click, "Stack N") would focus a filter or a field that typing never reaches,
    // and the shelf would close when the cursor leaves. Opening a layer is engaging with the
    // shelf — make it keyboard-interactive, like a click into the search (SPEC §3.3).
    if (opened && !shelf.interactive) void shelf.makeInteractive();
    if (!closed) return;
    void tick().then(() => {
      if (!shelf.open || overlays.layers.length !== n) return;
      const active = document.activeElement;
      const lost = !active || active === document.body || !shelf.panelEl?.contains(active);
      if (!lost) return;
      if (n === 0) shelf.focusSearch(false);
      else [...(shelf.panelEl?.querySelectorAll<HTMLElement>(".layer") ?? [])].pop()?.focus({ preventScroll: true });
    });
  });
</script>

{#each overlays.layers as layer, depth (layer.id)}
  <LayerSlot {layer} {depth} />
{/each}
