<script lang="ts">
  // Tooltip of a toolbar button: the name + the key (every editor button has one). Lives inside
  // the button (which is `position: relative`), shows on hover after a short delay and at once on
  // keyboard focus; never takes the pointer. A raised capsule like the HUD; the key is a Kbd.
  import Kbd from "$lib/components/Kbd.svelte";

  interface Props {
    label: string;
    keys?: string[];
    side?: "below" | "above";
  }
  let { label, keys = [], side = "below" }: Props = $props();
</script>

<span class="tip tip--{side}" aria-hidden="true">
  <span class="tip__label">{label}</span>
  {#each keys as k (k)}<Kbd size="sm">{k}</Kbd>{/each}
</span>

<style>
  .tip {
    position: absolute;
    left: 50%;
    z-index: 30;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 28px;
    padding: 0 4px 0 10px;
    border-radius: 999px;
    background: var(--color-raised);
    box-shadow: var(--shadow-pop);
    color: var(--color-ink);
    font-size: var(--text-secondary);
    font-weight: 500;
    white-space: nowrap;
    pointer-events: none;
    opacity: 0;
    transform: translate(-50%, 0) scale(0.96);
    transition:
      opacity var(--dur-fast) var(--ease-out-soft),
      transform var(--dur-fast) var(--ease-out-soft);
  }
  .tip:not(:has(:global(kbd))) {
    padding-right: 10px;
  }
  .tip__label {
    margin-right: 2px;
  }
  .tip--below {
    top: calc(100% + 8px);
  }
  .tip--above {
    bottom: calc(100% + 8px);
  }
  :global(:hover) > .tip {
    opacity: 1;
    transform: translate(-50%, 0) scale(1);
    transition-delay: 450ms;
  }
  :global(:focus-visible) > .tip {
    opacity: 1;
    transform: translate(-50%, 0) scale(1);
  }
  :global(:active) > .tip {
    opacity: 0;
    transition-delay: 0ms;
  }
  @media (prefers-reduced-motion: reduce) {
    .tip {
      transition: none;
    }
  }
</style>
