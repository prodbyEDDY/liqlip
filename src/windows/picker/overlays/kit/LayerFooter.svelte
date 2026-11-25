<script lang="ts">
  // Footer row of a layer: keyboard hints on the left (same Kbd size everywhere), actions on
  // the right. Buttons are 32 px capsules — concentric in the layer's bottom corners.
  import type { Snippet } from "svelte";
  import Kbd from "$lib/components/Kbd.svelte";

  interface Props {
    /** [keys, label] pairs. */
    hints?: [string, string][];
    actions?: Snippet;
  }
  let { hints = [], actions }: Props = $props();
</script>

<footer class="foot">
  {#if hints.length}
    <div class="foot__hints">
      {#each hints as [keys, label] (keys)}
        <span class="foot__hint"><Kbd size="sm">{keys}</Kbd><span>{label}</span></span>
      {/each}
    </div>
  {/if}
  {#if actions}<div class="foot__actions">{@render actions()}</div>{/if}
</footer>

<style>
  .foot {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    gap: 12px;
    min-height: var(--h-md);
    margin-top: 6px;
  }
  .foot__hints {
    display: flex;
    flex: 1 1 auto;
    align-items: center;
    gap: 14px;
    min-width: 0;
    padding-left: 6px;
    overflow: hidden;
    color: var(--color-ink-2);
    font-size: var(--text-secondary);
    white-space: nowrap;
  }
  .foot__hint {
    display: inline-flex;
    flex: 0 0 auto;
    align-items: center;
    gap: 6px;
  }
  .foot__actions {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    gap: 6px;
    margin-left: auto;
  }
</style>
