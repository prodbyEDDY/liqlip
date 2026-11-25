<script lang="ts">
  // Loading skeleton with the geometry of setting rows (never a centred spinner).
  import Skeleton from "$lib/components/Skeleton.svelte";

  interface Props {
    rows?: number;
    /** Show a group title placeholder above. */
    title?: boolean;
  }
  let { rows = 3, title = true }: Props = $props();
  const widths = ["46%", "38%", "52%", "34%", "44%"];
</script>

<div class="sk" aria-hidden="true">
  {#if title}<div class="sk__title"><Skeleton width="140px" height="13px" /></div>{/if}
  <div class="sk__body squircle">
    {#each Array.from({ length: rows }, (_, i) => i) as i (i)}
      <div class="sk__row">
        <div class="sk__text">
          <Skeleton width={widths[i % widths.length]} height="12px" />
          <Skeleton width="70%" height="10px" />
        </div>
        <Skeleton width="40px" height="22px" radius={999} />
      </div>
    {/each}
  </div>
</div>

<style>
  .sk {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .sk + :global(.sk) {
    margin-top: 26px;
  }
  .sk__title {
    padding: 0 4px;
  }
  .sk__body {
    display: flex;
    flex-direction: column;
    gap: 2px;
    border-radius: 16px;
    overflow: hidden;
  }
  .sk__row {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 16px;
    background: var(--s-tile);
  }
  .sk__text {
    display: flex;
    flex-direction: column;
    gap: 8px;
    flex: 1 1 auto;
  }
</style>
