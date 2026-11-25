<script lang="ts">
  // Bottom-center toast of the settings window (the shared capsule Toast, with an optional
  // action such as Undo).
  import Toast from "$lib/components/Toast.svelte";
  import { toasts } from "../ui.svelte";

  const toast = $derived(toasts.current);
  let busy = false;

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
      <Toast
        icon={toast.icon}
        message={toast.message}
        action={toast.action ? { label: toast.action.label, run: () => void run() } : null}
        onClose={() => toasts.dismiss()}
      />
    {/key}
  {/if}
</div>

<style>
  .host {
    position: fixed;
    left: 50%;
    bottom: 20px;
    z-index: 50;
    display: flex;
    justify-content: center;
    width: max-content;
    max-width: calc(100vw - 32px);
    transform: translateX(-50%);
    pointer-events: none;
  }
  .host > :global(*) {
    pointer-events: auto;
  }
</style>
