<script lang="ts">
  import Ico from "$lib/components/Ico.svelte";
  import { t } from "$lib/i18n/index.svelte";

  // Filled capsule search field (settings lists). The input itself draws no ring; the
  // capsule carries the concentric focus ring while the input has :focus-visible.
  interface Props {
    value?: string;
    placeholder?: string;
    disabled?: boolean;
    /** Show the leading search icon. Default true. */
    icon?: boolean;
    /** Kept for API compatibility (every field is filled now). */
    filled?: boolean;
    ariaLabel?: string;
    onInput?: (value: string) => void;
  }
  let { value = $bindable(""), placeholder = "", disabled = false, icon = true, ariaLabel, onInput }: Props = $props();

  function handleInput(e: Event) {
    value = (e.currentTarget as HTMLInputElement).value;
    onInput?.(value);
  }
  function clear() {
    value = "";
    onInput?.("");
  }
</script>

<div class="field" class:field--disabled={disabled}>
  {#if icon}<span class="field__icon"><Ico name="search" size={15} /></span>{/if}
  <input
    class="field__input selectable"
    type="text"
    {placeholder}
    {disabled}
    {value}
    aria-label={ariaLabel ?? (placeholder || undefined)}
    spellcheck="false"
    autocomplete="off"
    oninput={handleInput}
  />
  {#if value}
    <button type="button" class="field__clear" aria-label={t("common.clear")} title={t("common.clear")} onclick={clear}>
      <Ico name="x" size={13} />
    </button>
  {/if}
</div>

<style>
  .field {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    height: var(--h-lg);
    padding: 0 6px 0 14px;
    border-radius: 999px;
    background: var(--color-track);
    color: var(--color-ink-3);
    transition: background-color var(--dur-fast) var(--ease-out-soft);
  }
  .field:not(:has(.field__clear)) {
    padding-right: 14px;
  }
  .field:hover:not(.field--disabled) {
    background: var(--color-hover);
  }
  .field:has(.field__input:focus-visible) {
    background: var(--color-track);
    color: var(--color-ink-2);
    outline: var(--focus-ring);
    outline-offset: var(--focus-offset);
  }
  .field--disabled {
    opacity: 0.5;
  }
  .field__icon {
    display: inline-flex;
    flex: 0 0 auto;
  }
  .field__input {
    flex: 1 1 auto;
    min-width: 0;
    height: 100%;
    border: 0;
    background: transparent;
    color: var(--color-ink);
    font: inherit;
    font-size: var(--text-row);
    outline: none;
  }
  .field__input::placeholder {
    color: var(--color-ink-3);
  }
  .field__clear {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: 0 0 auto;
    width: var(--h-sm);
    height: var(--h-sm);
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: var(--color-ink-3);
    transition: background-color var(--dur-fast) var(--ease-out-soft);
  }
  .field__clear:hover {
    background: var(--color-hover);
    color: var(--color-ink);
  }
  .field__clear:focus-visible {
    outline-offset: 0;
  }
</style>
