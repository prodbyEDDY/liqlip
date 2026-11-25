<script lang="ts">
  // Toast dock at the bottom centre of the panel (it rides the panel's entrance), with an
  // optional action ("Deleted 3 · Undo"). The container is a polite live region, so screen
  // readers announce every toast. The shared `Toast` has no action slot, so the shelf keeps its
  // own (proposal for the coordinator: an `action` prop on `$lib/components/Toast`).
  import Ico from "$lib/components/Ico.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { toasts } from "./toasts.svelte";
</script>

<div class="dock" aria-live="polite" aria-atomic="true">
  {#if toasts.current}
    {#key toasts.current.id}
      <div class="toast squircle" class:toast--error={toasts.current.error} role="status">
        <span class="toast__icon"><Ico name={toasts.current.icon} size={15} /></span>
        <span class="toast__msg">{toasts.current.message}</span>
        {#if toasts.current.action}
          {@const action = toasts.current.action}
          <button type="button" class="toast__action" onmousedown={(e) => e.preventDefault()} onclick={() => action.run()}>
            <Ico name="undo" size={13} />{action.label}
          </button>
        {/if}
        <button type="button" class="toast__close" aria-label={t("common.dismiss")} onmousedown={(e) => e.preventDefault()} onclick={() => toasts.dismiss()}>
          <Ico name="x" size={13} />
        </button>
      </div>
    {/key}
  {/if}
</div>

<style>
  .dock {
    position: absolute;
    left: 50%;
    bottom: 46px;
    z-index: 30;
    display: flex;
    justify-content: center;
    width: max-content;
    max-width: calc(100% - 28px);
    transform: translateX(-50%);
    pointer-events: none;
  }
  .toast {
    display: inline-flex;
    align-items: center;
    gap: 9px;
    max-width: 100%;
    padding: 7px 7px 7px 12px;
    border-radius: 13px;
    background: var(--color-paper);
    box-shadow: var(--shadow-pop);
    pointer-events: auto;
    animation: toast-in var(--dur-slow) var(--ease-out-soft);
  }
  @keyframes toast-in {
    from {
      opacity: 0;
      transform: translateY(8px) scale(0.97);
      filter: blur(2px);
    }
  }
  .toast__icon {
    display: inline-flex;
    flex: 0 0 auto;
    color: var(--color-ink);
  }
  .toast__msg {
    min-width: 0;
    overflow: hidden;
    color: var(--color-ink);
    font-size: var(--text-secondary);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .toast__action,
  .toast__close {
    display: inline-flex;
    flex: 0 0 auto;
    align-items: center;
    justify-content: center;
    border: 0;
    font: inherit;
    cursor: pointer;
    transition:
      background-color var(--dur-fast) var(--ease-out-soft),
      color var(--dur-fast) var(--ease-out-soft),
      transform var(--dur-fast) var(--ease-out-soft);
  }
  .toast__action {
    gap: 5px;
    height: 26px;
    padding: 0 10px;
    border-radius: 999px;
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
    width: 24px;
    height: 24px;
    border-radius: 7px;
    background: transparent;
    color: var(--color-ink-3);
  }
  .toast__close:hover {
    background: var(--color-hover);
    color: var(--color-ink);
  }
  .toast__action:focus-visible,
  .toast__close:focus-visible {
    outline: 2px solid var(--color-ink);
    outline-offset: 2px;
  }
  @media (prefers-reduced-motion: reduce) {
    .toast {
      animation: none;
    }
  }
</style>
