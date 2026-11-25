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

<div class="flash squircle" role="status">
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
    font-size: 11px;
    font-weight: 600;
    white-space: nowrap;
    box-shadow: 0 4px 14px rgb(0 0 0 / 0.25);
  }
  /* Dark: deepen the drop shadow against the dark ground. */
  :global([data-theme="dark"]) .flash {
    box-shadow: 0 4px 14px rgb(0 0 0 / 0.5);
  }
</style>
