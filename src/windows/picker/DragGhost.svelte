<script lang="ts">
  // Small preview pinned to the cursor while a card is dragged (Liquid Tools ghost: 84×56,
  // rotated −3°): the thumbnail when the record has one, else its text; a counter when
  // several cards travel together. Lives OUTSIDE the panel so its transform can't clip it.
  import { drag } from "./drag.svelte";
  import { feed } from "./feed.svelte";

  const first = $derived(drag.ids.length ? feed.clip(drag.ids[0]) : undefined);
</script>

<div class="ghost" style="left:{drag.x}px;top:{drag.y}px" aria-hidden="true">
  {#if first?.thumbUrl}
    <img src={first.thumbUrl} alt="" draggable="false" />
  {:else}
    <span class="ghost__title">{first?.title ?? first?.preview ?? ""}</span>
  {/if}
  {#if drag.ids.length > 1}
    <span class="ghost__count">{drag.ids.length}</span>
  {/if}
</div>

<style>
  .ghost {
    position: fixed;
    z-index: 100;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 84px;
    height: 56px;
    margin: -28px 0 0 -42px;
    padding: 0 8px;
    border-radius: var(--radius-md);
    corner-shape: squircle;
    background: var(--color-card-lift);
    box-shadow: var(--shadow-pop);
    pointer-events: none;
    transform: rotate(-3deg);
  }
  .ghost img {
    position: absolute;
    inset: 0;
    display: block;
    width: 100%;
    height: 100%;
    border-radius: inherit;
    corner-shape: squircle;
    object-fit: cover;
  }
  .ghost__title {
    max-width: 100%;
    overflow: hidden;
    color: var(--color-ink-2);
    font-size: var(--text-meta);
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ghost__count {
    position: absolute;
    top: -7px;
    right: -7px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 20px;
    height: 20px;
    padding: 0 5px;
    border-radius: 999px;
    background: var(--color-ink);
    color: var(--color-paper);
    font-family: var(--font-mono);
    font-size: var(--text-meta);
    font-weight: 600;
  }
</style>
