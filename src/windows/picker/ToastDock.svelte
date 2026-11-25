<script lang="ts">
  // Toast dock: bottom centre of the panel, above the hint row (it rides the panel's
  // entrance) and above the overlay layers (z 40/41) — a toast from Quick Look or the actions
  // menu stays visible. Renders the shared capsule `Toast` (with the "Undo" action). The container
  // is a polite live region, so screen readers announce every toast.
  import Toast from "$lib/components/Toast.svelte";
  import { toasts } from "./toasts.svelte";
</script>

<div class="dock" aria-live="polite" aria-atomic="true">
  {#if toasts.current}
    {@const cur = toasts.current}
    {#key cur.id}
      <Toast
        icon={cur.icon}
        message={cur.message}
        error={cur.error}
        action={cur.action ? { label: cur.action.label, icon: cur.action.icon ?? "undo", run: cur.action.run } : null}
        onClose={() => toasts.dismiss()}
      />
    {/key}
  {/if}
</div>

<style>
  .dock {
    position: absolute;
    left: 50%;
    bottom: calc(var(--pad-panel) + var(--h-md) + 12px);
    z-index: 50;
    display: flex;
    justify-content: center;
    width: max-content;
    max-width: calc(100% - 2 * var(--pad-panel));
    transform: translateX(-50%);
    pointer-events: none;
  }
  .dock > :global(*) {
    pointer-events: auto;
  }
</style>
