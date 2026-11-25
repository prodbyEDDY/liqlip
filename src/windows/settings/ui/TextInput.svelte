<script lang="ts">
  // Single-line text field of the settings window (folder names, snippet names): a filled
  // 36 px capsule with the concentric focus ring while it has focus; `aria-invalid` +
  // `aria-describedby` for errors (invalid = a raised face, so it reads without colour).
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
  class="input selectable"
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
    height: var(--h-lg);
    padding: 0 16px;
    border: 0;
    border-radius: 999px;
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
  .input:hover:not(:disabled):not(:focus) {
    background: var(--color-hover);
  }
  /* Text entry: the ring shows whenever the field has focus (typing is keyboard work). */
  .input:focus {
    outline: var(--focus-ring);
    outline-offset: var(--focus-offset);
  }
  .input--invalid {
    background: var(--color-raised);
    box-shadow: var(--shadow-on-lift);
  }
  .input:disabled {
    opacity: 0.5;
  }
</style>
