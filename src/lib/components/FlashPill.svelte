<script lang="ts">
  // The compact ink-pill confirmation flash (✓ / ✕ + a few words) shared by the
  // shelf and the HUD. The CALLER owns positioning + transitions; this is only
  // the pill itself, so every window flashes identically.
  import Ico from "$lib/components/Ico.svelte";

  interface Props {
    message: string;
    error?: boolean;
  }
  let { message, error = false }: Props = $props();
</script>

<div class="flash" role="status">
  {#if error}
    <Ico name="x" size={12} />
  {:else}
    <Ico name="check" size={12} />
  {/if}
  <span>{message}</span>
</div>

<style>
  .flash {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 6px 11px;
    border-radius: var(--radius-pill);
    background: var(--color-ink);
    color: var(--color-paper);
    font-size: var(--text-secondary);
    font-weight: 600;
    white-space: nowrap;
    box-shadow: var(--shadow-pop);
  }
</style>
