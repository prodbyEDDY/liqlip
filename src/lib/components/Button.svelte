<script lang="ts">
  import type { Snippet } from "svelte";
  import Ico from "$lib/components/Ico.svelte";
  import type { IconName } from "$lib/icons";

  interface Props {
    variant?: "primary" | "secondary" | "ghost";
    size?: "md" | "sm";
    icon?: IconName;
    disabled?: boolean;
    loading?: boolean;
    type?: "button" | "submit";
    onclick?: () => void;
    children?: Snippet;
    [key: string]: unknown;
  }

  let {
    variant = "primary",
    size = "md",
    icon,
    disabled = false,
    loading = false,
    type = "button",
    onclick,
    children,
    ...rest
  }: Props = $props();
</script>

<button
  {type}
  class="btn squircle btn--{variant} btn--{size}"
  class:btn--loading={loading}
  disabled={disabled || loading}
  {onclick}
  {...rest}
>
  {#if loading}
    <span class="btn__spin"><Ico name="loaderCircle" size={size === "sm" ? 14 : 16} tilt={false} /></span>
  {:else if icon}
    <span class="btn__icon"><Ico name={icon} size={size === "sm" ? 14 : 16} /></span>
  {/if}
  {#if children}<span class="btn__label">{@render children()}</span>{/if}
</button>

<style>
  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
    border: 0;
    border-radius: 14px;
    font: inherit;
    font-weight: 600;
    cursor: default;
    white-space: nowrap;
    transition:
      background-color var(--dur-fast) var(--ease-out-soft),
      color var(--dur-fast) var(--ease-out-soft),
      transform var(--dur-fast) var(--ease-out-soft),
      opacity var(--dur-fast) var(--ease-out-soft);
  }
  /* Immediate tactile feedback on press (Emil tip #1) — subtle, never < 0.97. */
  .btn:active:not(:disabled):not(.btn--loading) {
    transform: scale(0.97);
  }
  .btn--md {
    height: 36px;
    padding: 0 16px;
    font-size: var(--text-row);
  }
  .btn--sm {
    height: 30px;
    padding: 0 12px;
    border-radius: 12px;
    font-size: var(--text-secondary);
  }

  /* Primary — ink fill */
  .btn--primary {
    background: var(--color-ink);
    color: var(--color-paper);
  }
  .btn--primary:hover:not(:disabled) {
    background: var(--color-ink-hover);
  }
  .btn--primary:active:not(:disabled) {
    background: var(--color-ink-active);
  }

  /* Secondary — filled surface */
  .btn--secondary {
    background: var(--color-track);
    color: var(--color-ink);
  }
  .btn--secondary:hover:not(:disabled) {
    background: var(--color-hover);
  }
  .btn--secondary:active:not(:disabled) {
    background: var(--color-track);
  }

  /* Ghost */
  .btn--ghost {
    background: transparent;
    color: var(--color-ink-2);
  }
  .btn--ghost:hover:not(:disabled) {
    background: var(--color-hover);
    color: var(--color-ink);
  }
  .btn--ghost:active:not(:disabled) {
    background: var(--color-track);
  }

  .btn:disabled {
    opacity: 0.45;
  }
  .btn--loading {
    cursor: default;
  }

  .btn__icon,
  .btn__spin {
    display: inline-flex;
    flex: 0 0 auto;
  }
  .btn__spin {
    animation: btn-spin 0.7s linear infinite;
  }
  @keyframes btn-spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .btn__spin {
      animation-duration: 1.4s;
    }
  }

</style>
