<script lang="ts">
  // Loading placeholder with the exact card geometry (square preview + 36 px meta row) —
  // SPEC §3.13: the skeleton matches the grid, not generic bars.
  import Skeleton from "$lib/components/Skeleton.svelte";
</script>

<div class="sk" aria-hidden="true">
  <Skeleton width="100%" height="auto" radius={12} />
  <div class="sk__meta">
    <Skeleton width="15px" height="15px" radius={3} />
    <Skeleton width="48%" height="9px" radius={5} />
    <span class="sk__badge"><Skeleton width="28px" height="15px" circle /></span>
  </div>
</div>

<style>
  .sk {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100%;
  }
  .sk > :global(.skeleton:first-child) {
    flex: 1 1 auto;
    min-height: 0;
  }
  .sk__meta {
    display: flex;
    flex: 0 0 36px;
    align-items: center;
    gap: 6px;
    padding: 0 3px;
  }
  .sk__badge {
    margin-left: auto;
  }
</style>
