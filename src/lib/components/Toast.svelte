<script lang="ts" module>
  import type { IconName } from "$lib/icons";
  export interface ToastAction {
    label: string;
    icon?: IconName;
    run: () => void;
  }
</script>

<script lang="ts">
  // Transient notice: a raised capsule (40 px) with an icon, the message, an optional action
  // ("Undo") and a dismiss button — both 32 px capsules inset 4 px, concentric with the toast.
  // The caller owns placement and the live region; this is only the toast itself, shared by
  // the shelf and the settings window.
  import Ico from "$lib/components/Ico.svelte";
  import { t } from "$lib/i18n/index.svelte";

  interface Props {
    icon?: IconName;
    message: string;
    error?: boolean;
    action?: ToastAction | null;
    onClose?: () => void;
  }
  let { icon, message, error = false, action = null, onClose }: Props = $props();
</script>

<div class="toast" class:toast--error={error} role="status">
  {#if icon}<span class="toast__icon"><Ico name={icon} size={16} /></span>{/if}
  <span class="toast__msg" title={message}>{message}</span>
  {#if action}
    <button type="button" class="toast__action" onmousedown={(e) => e.preventDefault()} onclick={() => action.run()}>
      {#if action.icon}<Ico name={action.icon} size={14} />{/if}{action.label}
    </button>
  {/if}
  {#if onClose}
    <button type="button" class="toast__close" aria-label={t("common.dismiss")} title={t("common.dismiss")} onmousedown={(e) => e.preventDefault()} onclick={onClose}>
      <Ico name="x" size={14} />
    </button>
  {/if}
</div>

<style>
  .toast {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    max-width: 100%;
    min-width: 0;
    min-height: 40px;
    padding: 4px;
    padding-left: 14px;
    border-radius: 999px;
    background: var(--color-raised);
    box-shadow: var(--shadow-pop);
    color: var(--color-ink);
    animation: toast-in var(--dur-slow) var(--ease-out-soft);
  }
  .toast:not(:has(button)) {
    padding-right: 16px;
  }
  @keyframes toast-in {
    from {
      opacity: 0;
      transform: translateY(8px) scale(0.97);
    }
  }
  .toast__icon {
    display: inline-flex;
    flex: 0 0 auto;
    color: var(--color-ink);
  }
  .toast--error .toast__icon {
    color: var(--color-danger);
  }
  .toast__msg {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    padding-right: 4px;
    font-size: var(--text-secondary);
    font-weight: 500;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .toast__action,
  .toast__close {
    display: inline-flex;
    flex: 0 0 auto;
    align-items: center;
    justify-content: center;
    gap: 5px;
    height: var(--h-md);
    border: 0;
    border-radius: 999px;
    font: inherit;
    transition:
      background-color var(--dur-fast) var(--ease-out-soft),
      color var(--dur-fast) var(--ease-out-soft),
      transform var(--dur-fast) var(--ease-out-soft);
  }
  .toast__action {
    padding: 0 13px;
    background: var(--color-ink);
    color: var(--color-paper);
    font-size: var(--text-secondary);
    font-weight: 600;
  }
  .toast__action:hover {
    background: var(--color-ink-hover);
  }
  .toast__action:active {
    background: var(--color-ink-active);
    transform: scale(0.97);
  }
  .toast__close {
    width: var(--h-md);
    background: transparent;
    color: var(--color-ink-3);
  }
  .toast__close:hover {
    background: var(--color-track);
    color: var(--color-ink);
  }
  .toast__close:active {
    background: var(--color-hover);
  }
</style>
