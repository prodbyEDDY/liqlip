<script lang="ts">
  // A small status capsule ("Always on", counters). Not interactive.
  import type { Snippet } from "svelte";
  import Ico from "$lib/components/Ico.svelte";
  import type { IconName } from "$lib/icons";

  interface Props {
    icon?: IconName;
    mono?: boolean;
    children?: Snippet;
  }
  let { icon, mono = false, children }: Props = $props();
</script>

<span class="badge" class:badge--mono={mono}>
  {#if icon}<span class="badge__icon"><Ico name={icon} size={12} /></span>{/if}
  {@render children?.()}
</span>

<style>
  .badge {
    display: inline-flex;
    flex: none;
    align-items: center;
    gap: 5px;
    height: var(--h-sm);
    padding: 0 10px;
    border-radius: 999px;
    background: var(--color-track);
    color: var(--color-ink-2);
    font-size: var(--text-secondary);
    font-weight: 600;
    white-space: nowrap;
  }
  .badge:has(.badge__icon) {
    padding-left: 8px;
  }
  .badge--mono {
    font-family: var(--font-mono);
    font-size: var(--text-meta);
    font-weight: 500;
    font-variant-numeric: tabular-nums;
  }
  .badge__icon {
    display: inline-flex;
    color: var(--color-ink-2);
  }
</style>
