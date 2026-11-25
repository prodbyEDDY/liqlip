<script lang="ts" module>
  import type { IconName } from "$lib/icons";
  export interface NavItem {
    key: string;
    label: string;
    icon: IconName;
  }
</script>

<script lang="ts">
  // Settings sidebar: one tab stop with roving focus —
  // ↑/↓ move, Home/End jump, Enter/Space open the section. `compact` shows icons only
  // (narrow windows); the label stays as the accessible name and tooltip.
  import Ico from "$lib/components/Ico.svelte";
  import { stepIndex } from "../logic";

  interface Props {
    items: NavItem[];
    selected: string;
    onSelect: (key: string) => void;
    ariaLabel: string;
    compact?: boolean;
  }
  let { items, selected, onSelect, ariaLabel, compact = false }: Props = $props();

  let focusKey = $state<string | null>(null);
  const tabKey = $derived(focusKey ?? selected);

  function onKeydown(e: KeyboardEvent) {
    const i = items.findIndex((it) => it.key === tabKey);
    let next = -1;
    if (e.key === "ArrowDown") next = stepIndex(i, 1, items.length);
    else if (e.key === "ArrowUp") next = stepIndex(i, -1, items.length);
    else if (e.key === "Home") next = 0;
    else if (e.key === "End") next = items.length - 1;
    if (next < 0) return;
    e.preventDefault();
    focusKey = items[next].key;
    (e.currentTarget as HTMLElement).querySelectorAll<HTMLButtonElement>("button")[next]?.focus();
  }
</script>

<nav class="nav" class:nav--compact={compact} aria-label={ariaLabel}>
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <ul class="nav__list" onkeydown={onKeydown}>
    {#each items as item (item.key)}
      {@const sel = item.key === selected}
      <li>
        <button
          type="button"
          class="nav__item"
          class:nav__item--sel={sel}
          tabindex={item.key === tabKey ? 0 : -1}
          aria-current={sel ? "page" : undefined}
          aria-label={compact ? item.label : undefined}
          title={compact ? item.label : undefined}
          onfocus={() => (focusKey = item.key)}
          onblur={() => (focusKey = null)}
          onclick={() => onSelect(item.key)}
        >
          <span class="nav__icon"><Ico name={item.icon} size={17} /></span>
          {#if !compact}<span class="nav__label">{item.label}</span>{/if}
        </button>
      </li>
    {/each}
  </ul>
</nav>

<style>
  .nav__list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  /* 36 px capsules. Chosen = the raised surface of the window (a lifted pill), ink label and
     icon — a clear state without a ring or a stroke. */
  .nav__item {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    height: var(--h-lg);
    padding: 0 12px;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: var(--color-ink-2);
    font: inherit;
    text-align: left;
    transition:
      background-color var(--dur-fast) var(--ease-out-soft),
      box-shadow var(--dur-fast) var(--ease-out-soft),
      color var(--dur-fast) var(--ease-out-soft);
  }
  .nav__item:hover {
    background: var(--color-track);
    color: var(--color-ink);
  }
  .nav__item:active {
    background: var(--color-hover);
  }
  .nav__item--sel,
  .nav__item--sel:hover,
  .nav__item--sel:active {
    background: var(--color-raised);
    box-shadow: var(--shadow-card);
    color: var(--color-ink);
  }
  .nav__icon {
    display: inline-flex;
    flex: 0 0 auto;
  }
  .nav__label {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    color: inherit;
    font-size: var(--text-row);
    font-weight: 500;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .nav__item--sel .nav__label {
    font-weight: 600;
  }
  .nav--compact .nav__item {
    justify-content: center;
    width: var(--h-lg);
    padding: 0;
  }
</style>
