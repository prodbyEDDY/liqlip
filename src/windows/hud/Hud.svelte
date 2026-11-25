<script lang="ts">
  // W0 stub of the HUD toast (W1-D replaces it): shows the text of the last `hud:show`.
  // Browser preview: /hud.html?kind=copied|stack_empty|paused|elevated
  import Ico from "$lib/components/Ico.svelte";
  import { t, type TranslationKey } from "$lib/i18n/index.svelte";
  import { onHudShow } from "$lib/ipc/events";
  import type { HudKind, HudShow } from "$lib/ipc/types";
  import type { IconName } from "$lib/icons";

  const KEY: Record<HudKind, TranslationKey> = {
    copied: "hud.copied",
    stack_empty: "hud.stackEmpty",
    paused: "hud.paused",
    elevated: "hud.elevated",
  };
  const ICON: Record<HudKind, IconName> = { copied: "check", stack_empty: "stack", paused: "pause", elevated: "shield" };

  const initialKind = typeof location !== "undefined" ? new URLSearchParams(location.search).get("kind") : null;
  let shown = $state<HudShow | null>(initialKind && initialKind in KEY ? { kind: initialKind as HudKind } : null);

  $effect(() => {
    const off = onHudShow((p) => (shown = p));
    return () => void off.then((f) => f());
  });
</script>

{#if shown}
  <div class="hud" role="status" aria-live="polite">
    <Ico name={ICON[shown.kind]} size={16} />
    <span>{shown.text ?? t(KEY[shown.kind])}</span>
  </div>
{/if}

<style>
  :global(html),
  :global(body) {
    background: transparent;
  }
  .hud {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    margin: 8px;
    padding: 10px 16px;
    border-radius: 999px;
    background: var(--color-paper);
    color: var(--color-ink);
    box-shadow: var(--shadow-pop);
    font-size: var(--text-row);
    font-weight: 600;
    white-space: nowrap;
  }
</style>
