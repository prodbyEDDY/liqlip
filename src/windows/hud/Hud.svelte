<script lang="ts">
  // HUD toast (SPEC §1, ARCHITECTURE §7): a monochrome capsule at the cursor, never focused
  // (Rust shows the window with SWP_NOACTIVATE; it is click-through). Each `hud:show` shows
  // the toast for ~1.4 s, then the exit transition runs and Rust hides the window at 1.66 s.
  // Browser preview on the mock: /hud.html?kind=copied (static) · &text=… · ?demo=1.
  import { onMount } from "svelte";
  import Ico from "$lib/components/Ico.svelte";
  import Kbd from "$lib/components/Kbd.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { onHudShow } from "$lib/ipc/events";
  import type { HudShow } from "$lib/ipc/types";
  import { HUD_KINDS, KIND_ICON, KIND_KEY, VISIBLE_MS, isSameToast, previewFromQuery, segments } from "./toast";

  const preview = typeof location !== "undefined" ? previewFromQuery(location.search) : { toast: null, demo: false };

  let toast = $state<HudShow | null>(preview.toast);
  let visible = $state(preview.toast !== null);
  /** Re-keys the capsule so a different toast plays its entry again. */
  let seq = $state(0);
  let hideTimer: ReturnType<typeof setTimeout> | undefined;

  function present(next: HudShow, autoHide = true): void {
    if (!isSameToast(toast, visible, next)) {
      toast = next;
      seq += 1;
    }
    visible = true;
    clearTimeout(hideTimer);
    if (autoHide) hideTimer = setTimeout(() => (visible = false), VISIBLE_MS);
  }

  // onMount, not $effect: present() reads state, which an effect would track and re-run on.
  onMount(() => {
    const off = onHudShow((p) => present(p));
    // ?demo=1 — every kind in turn with the real timing (checks the motion in a browser).
    let i = 0;
    const demo = preview.demo ? setInterval(() => present({ kind: HUD_KINDS[i++ % HUD_KINDS.length] }), 2200) : undefined;
    if (preview.demo) present({ kind: HUD_KINDS[i++] });
    return () => {
      clearTimeout(hideTimer);
      clearInterval(demo);
      void off.then((f) => f());
    };
  });

  const parts = $derived(toast && !toast.text ? segments(t(KIND_KEY[toast.kind])) : null);
</script>

<div class="stage">
  {#if toast}
    {#key seq}
      <div class="hud" class:out={!visible} class:hud--keys-end={parts?.at(-1)?.kind === "keys"} role="status" aria-live="polite">
        <span class="hud__icon"><Ico name={KIND_ICON[toast.kind]} size={16} /></span>
        <span class="msg">
          {#if parts}
            {#each parts as part, i (i)}
              {#if part.kind === "keys"}<Kbd>{t("hud.pasteKeys")}</Kbd>{:else}<span class="msg__text">{part.text.trim()}</span>{/if}
            {/each}
          {:else}
            <span class="msg__text">{toast.text}</span>
          {/if}
        </span>
      </div>
    {/key}
  {/if}
</div>

<style>
  :global(html),
  :global(body) {
    background: transparent;
    overflow: hidden;
    user-select: none;
  }
  /* The window is larger than the capsule (picker/geometry.rs HUD_SIZE 400×112): the
     transparent margin (36 px above/below, ≥ 32 px at the sides) holds --shadow-pop, so the
     shadow is never cut by the window edge. The capsule is 40 px tall, centred; its icon
     circle (32 px) and a trailing keycap (24 px) are inset so they stay concentric with the
     capsule ends (16 + 4 = 20, 12 + 8 = 20). */
  .stage {
    position: fixed;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    pointer-events: none;
  }
  .hud {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    height: 40px;
    max-width: calc(100% - 64px);
    box-sizing: border-box;
    padding: 4px 18px 4px 4px;
    border-radius: 999px;
    background: var(--color-raised);
    color: var(--color-ink);
    box-shadow: var(--shadow-pop);
    font-size: var(--text-row);
    font-weight: 600;
    line-height: 1;
    white-space: nowrap;
    /* No fill-mode: a kept end frame would override the .out transition. */
    animation: hud-in 190ms var(--ease-liquid);
    transition:
      opacity 170ms var(--ease-out-soft),
      transform 170ms var(--ease-out-soft);
  }
  .hud--keys-end {
    padding-right: 8px;
  }
  .hud.out {
    opacity: 0;
    transform: translateY(4px) scale(0.97);
  }
  .hud__icon {
    display: inline-flex;
    flex: 0 0 auto;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border-radius: 999px;
    background: var(--color-ink);
    color: var(--color-paper);
  }
  .msg {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .msg__text {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  @keyframes hud-in {
    from {
      opacity: 0;
      transform: translateY(6px) scale(0.94);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }
  /* app.css collapses durations under reduced motion; also drop the movement itself. */
  @media (prefers-reduced-motion: reduce) {
    .hud,
    .hud.out {
      transform: none;
    }
  }
</style>
