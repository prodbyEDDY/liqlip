<script lang="ts">
  import Ico from "$lib/components/Ico.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import type { IconName } from "$lib/icons";

  interface Props {
    icon?: IconName;
    message: string;
    onClose?: () => void;
  }
  let { icon, message, onClose }: Props = $props();
</script>

<div class="toast squircle" role="status">
  {#if icon}<span class="toast__icon"><Ico name={icon} size={16} /></span>{/if}
  <span class="toast__msg">{message}</span>
  {#if onClose}
    <button type="button" class="toast__close" aria-label={t("common.dismiss")} onclick={onClose}>
      <Ico name="x" size={14} />
    </button>
  {/if}
</div>

<style>
  .toast {
    display: inline-flex;
    align-items: center;
    gap: 9px;
    max-width: 360px;
    padding: 10px 12px;
    border-radius: 13px;
    background: var(--color-paper);
    box-shadow: var(--shadow-pop);
    /* Toasts are transient — they animate in when mounted (Sonner-style). */
    animation: toast-in var(--dur-slow) var(--ease-out-soft);
  }
  @keyframes toast-in {
    from {
      opacity: 0;
      transform: translateY(8px) scale(0.97);
      filter: blur(2px);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .toast {
      animation: none;
    }
  }
  .toast__icon {
    display: inline-flex;
    flex: 0 0 auto;
    color: var(--color-ink);
  }
  .toast__msg {
    flex: 1 1 auto;
    font-size: var(--text-secondary);
    color: var(--color-ink);
  }
  .toast__close {
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
  .toast__close:hover {
    background: var(--color-hover);
    color: var(--color-ink);
  }
</style>
