<script lang="ts">
  // Bottom-center toast of the settings window, with an optional action (Undo).
  import Ico from "$lib/components/Ico.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { toasts } from "../ui.svelte";

  const toast = $derived(toasts.current);
  let busy = $state(false);

  async function run() {
    const a = toast?.action;
    if (!a || busy) return;
    busy = true;
    try {
      await a.run();
    } finally {
      busy = false;
    }
  }
</script>

<div class="host" aria-live="polite">
  {#if toast}
    {#key toast.id}
      <div class="toast squircle" role="status">
        {#if toast.icon}<span class="toast__icon"><Ico name={toast.icon} size={16} /></span>{/if}
        <span class="toast__msg">{toast.message}</span>
        {#if toast.action}
          <button type="button" class="toast__action" disabled={busy} onclick={run}>{toast.action.label}</button>
        {/if}
        <button type="button" class="toast__close" aria-label={t("common.dismiss")} onclick={() => toasts.dismiss()}>
          <Ico name="x" size={14} />
        </button>
      </div>
    {/key}
  {/if}
</div>

<style>
  .host {
    position: fixed;
    left: 50%;
    bottom: 20px;
    z-index: 50;
    transform: translateX(-50%);
    max-width: calc(100vw - 32px);
    pointer-events: none;
  }
  .toast {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 44px;
    padding: 6px 6px 6px 14px;
    border-radius: 14px;
    background: var(--color-ink);
    color: var(--color-paper);
    box-shadow: var(--shadow-pop);
    pointer-events: auto;
    animation: toast-in var(--dur-slow) var(--ease-out-soft);
  }
  @keyframes toast-in {
    from {
      opacity: 0;
      transform: translateY(8px) scale(0.97);
    }
  }
  .toast__icon {
    display: inline-flex;
  }
  .toast__msg {
    font-size: var(--text-row);
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 520px;
  }
  .toast__action {
    height: 32px;
    padding: 0 12px;
    border: 0;
    border-radius: 999px;
    background: color-mix(in srgb, var(--color-paper) 16%, transparent);
    color: var(--color-paper);
    font: inherit;
    font-size: var(--text-row);
    font-weight: 700;
    cursor: default;
    transition: background-color var(--dur-fast) var(--ease-out-soft);
  }
  .toast__action:hover:not(:disabled) {
    background: color-mix(in srgb, var(--color-paper) 26%, transparent);
  }
  .toast__action:disabled {
    opacity: 0.6;
  }
  .toast__action:focus-visible,
  .toast__close:focus-visible {
    outline-color: var(--color-paper);
  }
  .toast__close {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: color-mix(in srgb, var(--color-paper) 70%, transparent);
    cursor: default;
    transition:
      background-color var(--dur-fast) var(--ease-out-soft),
      color var(--dur-fast) var(--ease-out-soft);
  }
  .toast__close:hover {
    background: color-mix(in srgb, var(--color-paper) 16%, transparent);
    color: var(--color-paper);
  }
</style>
