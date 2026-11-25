<script lang="ts" module>
  export interface DropdownOption {
    value: string;
    label: string;
  }
</script>

<script lang="ts">
  // Single-choice select (bits-ui Select): a capsule trigger and a raised layer surface whose
  // rows are concentric with it (--r-layer − --pad-layer). The list is portalled to <body>,
  // so no container can clip it. `id` / `describedby` let a settings row label it.
  import { Select } from "bits-ui";
  import Ico from "$lib/components/Ico.svelte";

  interface Props {
    options: DropdownOption[];
    value: string;
    onChange?: (value: string) => void;
    placeholder?: string;
    ariaLabel?: string;
    id?: string;
    describedby?: string;
    disabled?: boolean;
  }
  let { options, value, onChange, placeholder = "", ariaLabel, id, describedby, disabled = false }: Props = $props();

  const selectedLabel = $derived(options.find((o) => o.value === value)?.label ?? placeholder);
</script>

<Select.Root type="single" {value} onValueChange={(v) => onChange?.(v)} items={options} {disabled}>
  <Select.Trigger class="dd-trigger" {id} aria-label={ariaLabel} aria-describedby={describedby} title={selectedLabel}>
    <span class="dd-trigger__label" class:dd-trigger__label--ph={!options.some((o) => o.value === value)}>
      {selectedLabel}
    </span>
    <Ico name="chevronDown" size={14} class="dd-trigger__chev" />
  </Select.Trigger>

  <Select.Portal>
    <Select.Content class="dd-content squircle" sideOffset={6} align="start" collisionPadding={8}>
      <Select.Viewport class="dd-viewport">
        {#each options as opt (opt.value)}
          <Select.Item class="dd-item squircle" value={opt.value} label={opt.label}>
            {#snippet children({ selected })}
              <span class="dd-item__label">{opt.label}</span>
              <span class="dd-item__check" class:dd-item__check--on={selected}><Ico name="check" size={14} /></span>
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
    min-width: 0;
    height: var(--h-lg);
    padding: 0 12px 0 16px;
    border: 0;
    border-radius: 999px;
    background: var(--color-track);
    color: var(--color-ink);
    font: inherit;
    font-size: var(--text-row);
    font-weight: 500;
    transition: background-color var(--dur-fast) var(--ease-out-soft);
  }
  :global(.dd-trigger:hover:not(:disabled)) {
    background: var(--color-hover);
  }
  :global(.dd-trigger[data-state="open"]) {
    background: var(--color-selected);
  }
  :global(.dd-trigger:disabled) {
    opacity: 0.45;
  }
  :global(.dd-trigger__label) {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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
    max-width: min(420px, calc(100vw - 16px));
    max-height: min(320px, var(--bits-select-content-available-height, 320px));
    padding: var(--pad-layer);
    overflow-y: auto;
    border-radius: var(--r-layer);
    background: var(--color-raised);
    box-shadow: var(--shadow-pop);
    transform-origin: var(--bits-select-content-transform-origin);
    animation: dd-in var(--dur-base) var(--ease-out-soft);
  }
  @keyframes dd-in {
    from {
      opacity: 0;
      transform: translateY(-4px) scale(0.98);
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
    gap: 10px;
    min-height: var(--h-md);
    padding: 0 10px 0 12px;
    border-radius: var(--r-layer-in);
    color: var(--color-ink);
    font-size: var(--text-row);
    font-weight: 500;
    cursor: pointer;
    user-select: none;
    outline: none;
  }
  :global(.dd-item[data-highlighted]) {
    background: var(--color-hover);
  }
  :global(.dd-item[data-disabled]) {
    opacity: 0.45;
  }
  :global(.dd-item__label) {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  :global(.dd-item__check) {
    display: inline-flex;
    flex: 0 0 auto;
    color: var(--color-ink);
    opacity: 0;
  }
  :global(.dd-item__check--on) {
    opacity: 1;
  }
</style>
