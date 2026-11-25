<script lang="ts" module>
  export interface BigTypeItem {
    value: string;
    label: string;
  }
</script>

<script lang="ts">
  import Ico from "$lib/components/Ico.svelte";

  interface Props {
    items: BigTypeItem[];
    selected: string;
    onSelect?: (value: string) => void;
    /** Type size in px. Default 17. */
    size?: number;
    /** Cap height before the list scrolls. */
    maxHeight?: string;
    ariaLabel?: string;
  }
  let {
    items,
    selected,
    onSelect,
    size = 17,
    maxHeight = "232px",
    ariaLabel,
  }: Props = $props();
</script>

<!--
  Selection choreography (synchronized within the 150–250ms band):
  the checkmark slot is ALWAYS mounted and reserves zero width when unselected,
  so swapping selection animates two items at once — the old item's check
  collapses (width→0, opacity→0, scale→.6) while its label slides back to the
  start, and the new item's label slides right to make room while its check
  grows in and its color transitions ink-3 → ink. Class-driven CSS transitions
  guarantee the in/out run on the same clock.
-->
<div class="biglist" role="listbox" aria-label={ariaLabel} style={`max-height:${maxHeight}`}>
  {#each items as it (it.value)}
    {@const sel = it.value === selected}
    <button
      type="button"
      role="option"
      aria-selected={sel}
      class="bigitem"
      class:bigitem--sel={sel}
      style={`font-size:${size}px`}
      onclick={() => onSelect?.(it.value)}
    >
      <span class="bigitem__ck" aria-hidden="true">
        <Ico name="check" size={16} />
      </span>
      <span class="bigitem__t">{it.label}</span>
    </button>
  {/each}
</div>

<style>
  .biglist {
    display: flex;
    flex-direction: column;
    gap: 6px;
    overflow-y: auto;
    margin: 0 -6px;
    padding: 0 6px;
  }
  .bigitem {
    display: flex;
    align-items: center;
    width: 100%;
    padding: 3px 0;
    border: 0;
    background: transparent;
    font: inherit;
    font-weight: 700;
    text-align: left;
    color: var(--color-ink-3);
    cursor: default;
    transition: color var(--dur-base) var(--ease-out-soft);
  }
  .bigitem:hover:not(.bigitem--sel) {
    color: var(--color-ink-2);
  }
  .bigitem--sel {
    color: var(--color-ink);
  }

  /* Always present. Collapsed to zero width when unselected so the label sits
     at the start; expands to make room when selected. width+opacity+transform
     all share one transition so the morph reads as a single liquid motion. */
  .bigitem__ck {
    display: inline-flex;
    flex: 0 0 auto;
    align-items: center;
    justify-content: flex-start;
    width: 0;
    color: var(--color-ink);
    opacity: 0;
    transform: scale(0.6);
    transform-origin: left center;
    overflow: hidden;
    transition:
      width var(--dur-slow) var(--ease-out-soft),
      opacity var(--dur-base) var(--ease-out-soft),
      transform var(--dur-slow) var(--ease-pill);
  }
  .bigitem--sel .bigitem__ck {
    width: 24px; /* 16px glyph + 8px gap */
    opacity: 1;
    transform: scale(1);
  }

  .bigitem__t {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    /* Label slides back to the start when deselected, right to clear the check
       when selected — matched to the checkmark's width travel. */
    transform: translateX(0);
    transition: transform var(--dur-slow) var(--ease-out-soft);
  }

  @media (prefers-reduced-motion: reduce) {
    .bigitem,
    .bigitem__ck,
    .bigitem__t {
      transition: none;
    }
  }
</style>
