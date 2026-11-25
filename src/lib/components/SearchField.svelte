<script lang="ts">
  import Ico from "$lib/components/Ico.svelte";
  import { t } from "$lib/i18n/index.svelte";

  interface Props {
    value?: string;
    placeholder?: string;
    disabled?: boolean;
    /** Show the leading search icon. Default true. */
    icon?: boolean;
    /** Filled grey field with no border (toolbars). Default false. */
    filled?: boolean;
    onInput?: (value: string) => void;
  }
  let {
    value = $bindable(""),
    placeholder = "",
    disabled = false,
    icon = true,
    filled = false,
    onInput,
  }: Props = $props();

  function handleInput(e: Event) {
    value = (e.currentTarget as HTMLInputElement).value;
    onInput?.(value);
  }
  function clear() {
    value = "";
    onInput?.("");
  }
</script>

<div class="field squircle" class:field--disabled={disabled} class:field--filled={filled}>
  {#if icon}<span class="field__icon"><Ico name="search" size={15} /></span>{/if}
  <input
    class="field__input selectable"
    type="text"
    {placeholder}
    {disabled}
    {value}
    oninput={handleInput}
  />
  {#if value}
    <button type="button" class="field__clear" aria-label={t("common.clear")} onclick={clear}>
      <Ico name="x" size={14} />
    </button>
  {/if}
</div>

<style>
  .field {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 36px;
    padding: 0 10px;
    border-radius: 12px;
    background: var(--color-track);
    color: var(--color-ink-3);
    transition: background-color var(--dur-fast) var(--ease-out-soft);
  }
  .field:hover:not(.field--disabled) {
    background: var(--color-hover);
  }
  .field:focus-within {
    background: var(--color-selected);
    color: var(--color-ink-2);
    /* The inner input swallows the global :focus-visible ring (outline: none),
       so the container carries a real keyboard-focus indicator itself. */
    outline: 2px solid var(--color-ink);
    outline-offset: 2px;
  }
  .field--filled {
    height: 36px;
    border-radius: 12px;
    background: var(--color-hover);
  }
  .field--filled:hover:not(.field--disabled) {
    background: var(--color-selected);
  }
  .field--filled:focus-within {
    background: var(--color-selected);
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
    width: 20px;
    height: 20px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--color-ink-3);
    cursor: default;
    transition: background-color var(--dur-fast) var(--ease-out-soft);
  }
  .field__clear:hover {
    background: var(--color-hover);
    color: var(--color-ink);
  }
</style>
