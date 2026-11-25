<script lang="ts">
  // Loading placeholder with the exact card geometry (one r16 tile: preview + the 32 px meta
  // row with the app icon, the title and the format badge) — SPEC §3.13: the skeleton matches
  // the grid, not generic bars.
  import Skeleton from "$lib/components/Skeleton.svelte";
</script>

<div class="sk squircle" aria-hidden="true">
  <div class="sk__preview">
    <Skeleton width="72%" height="10px" radius={5} />
    <Skeleton width="54%" height="10px" radius={5} />
    <Skeleton width="63%" height="10px" radius={5} />
  </div>
  <div class="sk__meta">
    <Skeleton width="16px" height="16px" radius={4} />
    <Skeleton width="42%" height="10px" radius={5} />
    <span class="sk__badge"><Skeleton width="30px" height="18px" circle /></span>
  </div>
</div>

<style>
  .sk {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100%;
    border-radius: var(--r-card);
    background: var(--color-card);
  }
  .sk__preview {
    display: flex;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 9px;
    min-height: 0;
    padding: 15px 12px 0;
  }
  .sk__meta {
    display: flex;
    flex: 0 0 32px;
    align-items: center;
    gap: 6px;
    padding: 0 7px 0 9px;
  }
  .sk__badge {
    margin-left: auto;
  }
</style>
