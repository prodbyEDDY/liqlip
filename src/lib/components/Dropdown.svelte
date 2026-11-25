<script lang="ts" module>
  export interface DropdownOption {
    value: string;
    label: string;
  }
</script>

<script lang="ts">
  import { Select } from "bits-ui";
  import Ico from "$lib/components/Ico.svelte";

  interface Props {
    options: DropdownOption[];
    value: string;
    onChange?: (value: string) => void;
    placeholder?: string;
    ariaLabel?: string;
    disabled?: boolean;
  }
  let {
    options,
    value,
    onChange,
    placeholder = "",
    ariaLabel,
    disabled = false,
  }: Props = $props();

  const selectedLabel = $derived(
    options.find((o) => o.value === value)?.label ?? placeholder,
  );
</script>

<Select.Root
  type="single"
  {value}
  onValueChange={(v) => onChange?.(v)}
  items={options}
  {disabled}
>
  <Select.Trigger class="dd-trigger squircle" aria-label={ariaLabel}>
    <span class="dd-trigger__label" class:dd-trigger__label--ph={!options.some((o) => o.value === value)}>
      {selectedLabel}
    </span>
    <Ico name="chevronDown" size={16} class="dd-trigger__chev" tilt={false} />
  </Select.Trigger>

  <Select.Portal>
    <Select.Content class="dd-content squircle" sideOffset={6} align="start">
      <Select.Viewport class="dd-viewport">
        {#each options as opt (opt.value)}
          <Select.Item class="dd-item squircle" value={opt.value} label={opt.label}>
            {#snippet children({ selected })}
              <span class="dd-item__label">{opt.label}</span>
              {#if selected}
                <span class="dd-item__check"><Ico name="check" size={15} /></span>
              {/if}
            {/snippet}
          </Select.Item>
        {/each}
      </Select.Viewport>
    </Select.Content>
  </Select.Portal>
</Select.Root>

<style>
  :global(.dd-trigger) {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    width: 100%;
    height: 36px;
    padding: 0 12px;
    border: 0;
    border-radius: 12px;
    background: var(--color-track);
    color: var(--color-ink);
    font: inherit;
    font-size: var(--text-row);
    font-weight: 500;
    cursor: default;
    transition: background-color var(--dur-fast) var(--ease-out-soft);
  }
  :global(.dd-trigger:hover) {
    background: var(--color-hover);
  }
  :global(.dd-trigger[data-state="open"]) {
    background: var(--color-selected);
  }
  :global(.dd-trigger__label--ph) {
    color: var(--color-ink-3);
  }
  :global(.dd-trigger__chev) {
    flex: 0 0 auto;
    color: var(--color-ink-3);
    transition: transform var(--dur-base) var(--ease-out-soft);
  }
  :global(.dd-trigger[data-state="open"] .dd-trigger__chev) {
    transform: rotate(180deg);
  }

  :global(.dd-content) {
    z-index: 60;
    min-width: var(--bits-select-anchor-width, 200px);
    max-height: 280px;
    padding: 6px;
    border-radius: 14px;
    background: var(--color-paper);
    box-shadow: var(--shadow-pop);
    transform-origin: var(--bits-select-content-transform-origin);
    animation: dd-in var(--dur-base) var(--ease-out-soft);
  }
  /* Grows from the trigger (origin-aware) with a hair of blur that resolves —
     masks the swap so it reads as one motion (Jakub). */
  @keyframes dd-in {
    from {
      opacity: 0;
      transform: translateY(-5px) scale(0.97);
      filter: blur(3px);
    }
  }
  :global(.dd-viewport) {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  :global(.dd-item) {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    height: 34px;
    padding: 0 10px;
    border-radius: 10px;
    color: var(--color-ink);
    font-size: var(--text-row);
    font-weight: 500;
    cursor: default;
    user-select: none;
    outline: none;
  }
  :global(.dd-item[data-highlighted]) {
    background: var(--color-hover);
  }
  :global(.dd-item[data-selected]) {
    background: var(--color-selected);
  }
  :global(.dd-item__check) {
    display: inline-flex;
    color: var(--color-ink);
  }
  @media (prefers-reduced-motion: reduce) {
    :global(.dd-content) {
      animation: none;
    }
  }
</style>
