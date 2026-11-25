<script lang="ts">
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
  {#if icon}<span class="badge__icon"><Ico name={icon} size={11} /></span>{/if}
  {@render children?.()}
</span>

<style>
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 20px;
    padding: 0 8px;
    border-radius: 7px;
    background: var(--color-track);
    color: var(--color-ink-2);
    font-size: var(--text-meta);
    font-weight: 500;
    white-space: nowrap;
  }
  .badge--mono {
    font-family: var(--font-mono);
    font-weight: 500;
  }
  .badge__icon {
    display: inline-flex;
    color: var(--color-ink-3);
  }
</style>
