<script lang="ts" module>
  export type NoticeData = { kind: "confirm" } | { kind: "error"; message: string };
</script>

<script lang="ts">
  // Overlay notices at the top centre: "Close without saving? Esc — yes" (the last step of the
  // Esc ladder with objects drawn, SPEC §9.2) and errors (a failed commit — the overlay stays, so
  // nothing is lost). Announced to screen readers (`Toast` is role=status).
  import Kbd from "$lib/components/Kbd.svelte";
  import Toast from "$lib/components/Toast.svelte";
  import { t } from "$lib/i18n/index.svelte";

  let { notice, onclose }: { notice: NoticeData; onclose: () => void } = $props();

  /** "… {keys} …" → text, key chip, text. */
  const confirmParts = $derived(t("capture.confirmClose").split("{keys}"));
</script>

<div class="notice">
  {#if notice.kind === "confirm"}
    <div class="confirm" role="alert">
      <span>{confirmParts[0]}</span><Kbd size="sm" strong>Esc</Kbd>{#if confirmParts[1]}<span>{confirmParts[1]}</span>{/if}
    </div>
  {:else}
    <Toast icon="warning" error message={notice.message} onClose={onclose} />
  {/if}
</div>

<style>
  .notice {
    position: fixed;
    z-index: 7;
    top: 24px;
    left: 50%;
    max-width: calc(100vw - 32px);
    transform: translateX(-50%);
  }
  .confirm {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-height: 40px;
    padding: 0 16px;
    border-radius: 999px;
    background: var(--color-raised);
    box-shadow: var(--shadow-pop);
    color: var(--color-ink);
    font-size: var(--text-row);
    font-weight: 600;
    white-space: nowrap;
    animation: notice-in var(--dur-slow) var(--ease-liquid);
  }
  @keyframes notice-in {
    from {
      opacity: 0;
      transform: translateY(-6px) scale(0.97);
    }
  }
</style>
