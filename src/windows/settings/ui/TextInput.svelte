<script lang="ts">
  // Single-line text field of the settings window (folder names, snippet names): filled
  // track surface, ink focus ring, `aria-invalid` + `aria-describedby` for errors.
  interface Props {
    value: string;
    id?: string;
    placeholder?: string;
    ariaLabel?: string;
    describedby?: string;
    invalid?: boolean;
    disabled?: boolean;
    /** Focus (and select) on mount. */
    focus?: boolean;
    oninput?: (value: string) => void;
    onkeydown?: (e: KeyboardEvent) => void;
    onblur?: () => void;
  }
  let {
    value = $bindable(),
    id,
    placeholder,
    ariaLabel,
    describedby,
    invalid = false,
    disabled = false,
    focus = false,
    oninput,
    onkeydown,
    onblur,
  }: Props = $props();

  function autofocus(node: HTMLInputElement) {
    if (!focus) return;
    queueMicrotask(() => {
      node.focus();
      node.select();
    });
  }
</script>

<input
  class="input squircle selectable"
  class:input--invalid={invalid}
  type="text"
  {id}
  {placeholder}
  {disabled}
  aria-label={ariaLabel}
  aria-describedby={describedby}
  aria-invalid={invalid || undefined}
  spellcheck="false"
  autocomplete="off"
  bind:value
  use:autofocus
  oninput={() => oninput?.(value)}
  {onkeydown}
  {onblur}
/>

<style>
  .input {
    width: 100%;
    height: 36px;
    padding: 0 12px;
    border: 0;
    border-radius: 12px;
    background: var(--color-track);
    color: var(--color-ink);
    font: inherit;
    font-size: var(--text-row);
    outline: none;
    transition: background-color var(--dur-fast) var(--ease-out-soft);
  }
  .input::placeholder {
    color: var(--color-ink-3);
  }
  .input:hover:not(:disabled) {
    background: var(--color-hover);
  }
  /* Same focus treatment as SearchField: a real ring, also on mouse focus (text entry). */
  .input:focus {
    background: var(--color-selected);
    outline: 2px solid var(--color-ink);
    outline-offset: 2px;
  }
  .input--invalid {
    background: var(--color-paper);
  }
  .input:disabled {
    opacity: 0.5;
  }
</style>
