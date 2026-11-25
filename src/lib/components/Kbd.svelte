<script lang="ts">
  // A keycap. Capsule (999px — "max roundness"), so it sits concentric wherever it lands and
  // a single glyph is a circle. Functional mono at the 11 px service size; one height per
  // context: `sm` 20 px (hint rows, menus), `md` 24 px (default), `lg` 32 px (hero keys).
  import type { Snippet } from "svelte";
  interface Props {
    size?: "sm" | "md" | "lg";
    /** Inverted (ink) keycap — for the "press this now" key. */
    strong?: boolean;
    children?: Snippet;
  }
  let { size = "md", strong = false, children }: Props = $props();
</script>

<kbd class="kbd kbd--{size}" class:kbd--strong={strong}>{@render children?.()}</kbd>

<style>
  .kbd {
    display: inline-flex;
    flex: none;
    align-items: center;
    justify-content: center;
    border-radius: 999px;
    background: var(--color-track);
    color: var(--color-ink-2);
    font-family: var(--font-mono);
    font-size: var(--text-meta);
    font-weight: 500;
    line-height: 1;
    white-space: nowrap;
  }
  .kbd--sm {
    min-width: var(--h-xs);
    height: var(--h-xs);
    padding: 0 6px;
  }
  .kbd--md {
    min-width: var(--h-sm);
    height: var(--h-sm);
    padding: 0 8px;
  }
  .kbd--lg {
    min-width: var(--h-md);
    height: var(--h-md);
    padding: 0 12px;
    font-size: var(--text-row);
    font-weight: 600;
  }
  .kbd--strong {
    background: var(--color-ink);
    color: var(--color-paper);
  }
</style>
