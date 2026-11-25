<script lang="ts">
  // The product button: a capsule (999px), so it nests concentric in any container. Without a
  // label it is a circle. Variants: primary (ink), secondary (control wash), ghost, danger
  // (control wash + danger text). Sizes: sm 32 px, md 36 px.
  import type { Snippet } from "svelte";
  import Ico from "$lib/components/Ico.svelte";
  import type { IconName } from "$lib/icons";

  interface Props {
    variant?: "primary" | "secondary" | "ghost" | "danger";
    size?: "md" | "sm";
    icon?: IconName;
    disabled?: boolean;
    loading?: boolean;
    type?: "button" | "submit";
    onclick?: (e: MouseEvent) => void;
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
  class="btn btn--{variant} btn--{size}"
  class:btn--icon={!children}
  class:btn--loading={loading}
  disabled={disabled || loading}
  aria-busy={loading || undefined}
  {onclick}
  {...rest}
>
  {#if loading}
    <span class="btn__spin"><Ico name="loaderCircle" size={size === "sm" ? 14 : 16} /></span>
  {:else if icon}
    <span class="btn__icon"><Ico name={icon} size={size === "sm" ? 14 : 16} /></span>
  {/if}
  {#if children}<span class="btn__label">{@render children()}</span>{/if}
</button>

<style>
  .btn {
    display: inline-flex;
    flex: none;
    align-items: center;
    justify-content: center;
    gap: 6px;
    min-width: 0;
    border: 0;
    border-radius: 999px;
    font: inherit;
    font-weight: 600;
    white-space: nowrap;
    transition:
      background-color var(--dur-fast) var(--ease-out-soft),
      color var(--dur-fast) var(--ease-out-soft),
      transform var(--dur-fast) var(--ease-out-soft),
      opacity var(--dur-fast) var(--ease-out-soft);
  }
  .btn:active:not(:disabled) {
    transform: scale(0.97);
  }
  .btn--md {
    height: var(--h-lg);
    padding: 0 16px;
    font-size: var(--text-row);
  }
  .btn--sm {
    height: var(--h-md);
    padding: 0 13px;
    font-size: var(--text-secondary);
  }
  .btn--md:has(.btn__icon, .btn__spin):not(.btn--icon) {
    padding-left: 13px;
  }
  .btn--sm:has(.btn__icon, .btn__spin):not(.btn--icon) {
    padding-left: 10px;
  }
  .btn--icon.btn--md {
    width: var(--h-lg);
    padding: 0;
  }
  .btn--icon.btn--sm {
    width: var(--h-md);
    padding: 0;
  }
  .btn__label {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

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

  .btn--secondary,
  .btn--danger {
    background: var(--color-track);
    color: var(--color-ink);
  }
  .btn--danger {
    color: var(--color-danger);
  }
  .btn--secondary:hover:not(:disabled),
  .btn--danger:hover:not(:disabled) {
    background: var(--color-hover);
  }
  .btn--secondary:active:not(:disabled),
  .btn--danger:active:not(:disabled) {
    background: var(--color-selected);
  }

  .btn--ghost {
    background: transparent;
    color: var(--color-ink-2);
  }
  .btn--ghost:hover:not(:disabled) {
    background: var(--color-track);
    color: var(--color-ink);
  }
  .btn--ghost:active:not(:disabled) {
    background: var(--color-hover);
  }

  .btn:disabled {
    opacity: 0.45;
  }
  .btn--loading:disabled {
    opacity: 0.8;
  }

  .btn__icon,
  .btn__spin {
    display: inline-flex;
    flex: 0 0 auto;
  }
  .btn__spin {
    animation: btn-spin 0.8s linear infinite;
  }
  @keyframes btn-spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
