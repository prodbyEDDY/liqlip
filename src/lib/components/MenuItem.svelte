<script lang="ts">
  import Ico from "$lib/components/Ico.svelte";
  import type { IconName } from "$lib/icons";

  interface Props {
    icon: IconName;
    label: string;
    selected?: boolean;
    /** Show the trailing chevron (the item opens a detail panel). Defaults to `selected`. */
    chevron?: boolean;
    disabled?: boolean;
    onclick?: () => void;
  }

  let {
    icon,
    label,
    selected = false,
    chevron,
    disabled = false,
    onclick,
  }: Props = $props();

  const showChevron = $derived(chevron ?? selected);
</script>

<button
  type="button"
  class="item"
  class:item--selected={selected}
  {disabled}
  aria-current={selected ? "true" : undefined}
  {onclick}
>
  <span class="item__icon"><Ico name={icon} size={18} /></span>
  <span class="item__label">{label}</span>
  {#if showChevron}
    <span class="item__chev"><Ico name="chevronRight" size={15} /></span>
  {/if}
</button>

<style>
  .item {
    display: flex;
    align-items: center;
    gap: 11px;
    width: 100%;
    padding: 10px 11px;
    border: 0;
    border-radius: 13px;
    background: transparent;
    /* currentColor drives the leading icon. */
    color: var(--color-ink-2);
    font: inherit;
    text-align: left;
    cursor: default;
    transition:
      background-color var(--dur-fast) var(--ease-out-soft),
      color var(--dur-fast) var(--ease-out-soft);
  }
  .item:hover:not(:disabled) {
    background: var(--color-hover);
    color: var(--color-ink);
  }
  .item:active:not(:disabled) {
    background: var(--color-track);
  }
  .item--selected,
  .item--selected:hover {
    background: var(--color-selected);
    color: var(--color-ink);
  }
  .item:disabled {
    opacity: 0.45;
  }

  .item__icon {
    display: inline-flex;
    flex: 0 0 auto;
  }
  .item__label {
    flex: 1 1 auto;
    min-width: 0;
    font-size: var(--text-row);
    font-weight: 600;
    color: var(--color-ink);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .item--selected .item__label {
    font-weight: 700;
  }
  .item__chev {
    display: inline-flex;
    flex: 0 0 auto;
    color: var(--color-ink-3);
  }
</style>
