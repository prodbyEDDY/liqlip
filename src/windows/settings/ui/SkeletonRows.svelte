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
  {#if title}<div class="sk__title"><Skeleton width="120px" height="12px" /></div>{/if}
  <div class="sk__body squircle">
    {#each Array.from({ length: rows }, (_, i) => i) as i (i)}
      <div class="sk__row">
        <div class="sk__text">
          <Skeleton width={widths[i % widths.length]} height="12px" />
          <Skeleton width="70%" height="10px" />
        </div>
        <Skeleton width="40px" height="24px" circle />
      </div>
    {/each}
  </div>
</div>

<style>
  .sk {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .sk + :global(.sk) {
    margin-top: 24px;
  }
  .sk__title {
    padding: 2px 4px;
  }
  .sk__body {
    display: flex;
    flex-direction: column;
    padding: var(--pad-tile);
    border-radius: var(--r-tile);
    background: var(--color-tile);
  }
  .sk__row {
    display: flex;
    align-items: center;
    gap: 16px;
    min-height: 60px;
    padding: 12px;
  }
  .sk__text {
    display: flex;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 8px;
  }
</style>
