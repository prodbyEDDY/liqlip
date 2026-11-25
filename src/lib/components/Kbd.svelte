<script lang="ts">
  import type { Snippet } from "svelte";
  interface Props {
    children?: Snippet;
  }
  let { children }: Props = $props();
</script>

<kbd class="kbd">{@render children?.()}</kbd>

<style>
  .kbd {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 22px;
    height: 22px;
    padding: 0 6px;
    border-radius: 8px;
    background: var(--color-track);
    color: var(--color-ink-2);
    font-family: var(--font-mono);
    font-size: var(--text-meta);
    font-weight: 500;
    line-height: 1;
  }
</style>
