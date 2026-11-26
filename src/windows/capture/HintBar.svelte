<script lang="ts">
  // Stage-1 hints with keys (SPEC §9.2): what a click and a drag do, then F · R · M · Enter · Esc.
  // At the bottom centre of the monitor; moves to the top when the cursor comes near, hides while
  // an area is being drawn. Never takes the pointer.
  import Kbd from "$lib/components/Kbd.svelte";
  import { t } from "$lib/i18n/index.svelte";

  interface Props {
    text: boolean;
    last: boolean;
    top: boolean;
    hidden: boolean;
  }
  let { text, last, top, hidden }: Props = $props();

  const keys = $derived<[string, string][]>([
    ["F", t("capture.hint.screen")],
    ...(last ? ([["R", t("capture.hint.last")]] as [string, string][]) : []),
    ["M", t("capture.hint.magnifier")],
    ["↵", t("capture.hint.keyboard")],
    ["Esc", t("common.cancel")],
  ]);
</script>

<div class="hints" class:hints--top={top} class:hints--hidden={hidden} aria-hidden={hidden}>
  <span class="hints__lead">{text ? t("capture.hint.selectText") : t("capture.hint.select")}</span>
  {#each keys as [k, label] (k)}
    <span class="hint"><Kbd size="sm">{k}</Kbd><span class="hint__label">{label}</span></span>
  {/each}
</div>

<style>
  .hints {
    position: fixed;
    z-index: 6;
    left: 50%;
    bottom: 24px;
    display: flex;
    align-items: center;
    gap: 14px;
    max-width: calc(100vw - 32px);
    min-height: 40px;
    padding: 0 8px 0 16px;
    border-radius: 999px;
    background: var(--color-raised);
    box-shadow: var(--shadow-pop);
    color: var(--color-ink);
    font-size: var(--text-secondary);
    transform: translateX(-50%);
    pointer-events: none;
    transition: opacity var(--dur-base) var(--ease-out-soft);
    white-space: nowrap;
  }
  .hints--top {
    top: 24px;
    bottom: auto;
  }
  .hints--hidden {
    opacity: 0;
  }
  .hints__lead {
    font-weight: 600;
  }
  .hint {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--color-ink-2);
  }
  .hint:last-child {
    padding-right: 4px;
  }
</style>
