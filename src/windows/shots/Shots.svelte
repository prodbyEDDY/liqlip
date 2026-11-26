<script lang="ts">
  // QAO root (SPEC §9.4, ARCHITECTURE §18.8): the thumbnail stack after a shot. Rust (W6-B) builds
  // the `shots` window on the first shot, shows it without activation in the bottom corner of the
  // shot's monitor and sizes it from `shots_hud_layout({height})`; this page owns the stack
  // (`qao.ts`), its timers and the buttons. The window is never activated — no keys reach it in
  // Tauri. Window = stack + a transparent BLEED (24 px) on every side for the shadows (stack.ts).
  // Browser preview on the mock: /shots.html — small mock buttons add shots (also a failed save
  // and a shot kept out of history).
  import { onMount, untrack } from "svelte";
  import { flip } from "svelte/animate";
  import { cubicOut } from "svelte/easing";
  import { fly } from "svelte/transition";
  import { isBrowserShortcut } from "$lib/browserKeys";
  import { t } from "$lib/i18n/index.svelte";
  import {
    clipCopy,
    clipDetail,
    clipDragOut,
    clipOcr,
    clipsDelete,
    ocrStatus,
    settingsGet,
    settingsOpen,
    shotCapture,
    shotEdit,
    shotPin,
    shotReveal,
    shotSave,
    shotsHudHide,
    shotsHudLayout,
    textCopy,
  } from "$lib/ipc/commands";
  import { onClipsChanged, onSettingsChanged, onShotAdded, onShotEnd, onShotSession } from "$lib/ipc/events";
  import { isTauri } from "$lib/ipc/runtime";
  import type { Settings, ShotAdded } from "$lib/ipc/types";
  import { motionDuration } from "$lib/motion";
  import { createQao, initialState } from "./qao";
  import ShotCard from "./ShotCard.svelte";
  import { playShutter } from "./shutter";
  import { BLEED, GAP } from "./stack";

  const qs = $state(initialState());
  const qao = createQao(qs, {
    clipCopy,
    clipDetail,
    clipDragOut,
    clipOcr,
    clipsDelete,
    textCopy,
    shotEdit,
    shotPin,
    shotSave,
    shotReveal,
    shotsHudLayout,
    shotsHudHide,
    settingsOpen,
    shutter: playShutter,
    now: () => performance.now(),
    later: (fn, ms) => {
      const h = setTimeout(fn, ms);
      return () => clearTimeout(h);
    },
  });

  const mock = !isTauri();

  // ── settings and text recognition ──────────────────────────────────────────
  let ocrEnabled = true;
  let ocrAvailable = true;
  function gateOcr(): void {
    qs.ocr = !ocrEnabled ? "off" : ocrAvailable ? "ready" : "nolang";
  }
  function refreshOcr(): void {
    ocrStatus()
      .then((s) => {
        ocrAvailable = s.available;
        gateOcr();
      })
      .catch((e) => console.error("[shots] ocr_status", e));
  }
  function applySettings(s: Settings): void {
    qs.timeout = s.screenshot.timeout;
    qs.sound = s.screenshot.sound;
    qs.corner = s.screenshot.corner;
    ocrEnabled = s.ocr.enabled;
    gateOcr();
    refreshOcr();
  }

  // ── mock buttons (browser only) ────────────────────────────────────────────
  let devNext: "failed" | "temp" | null = null;
  let devSeq = 0;
  function devAdd(kind: "plain" | "failed" | "temp"): void {
    devNext = kind === "plain" ? null : kind;
    void shotCapture("screen").catch((e) => console.error("[shots] mock shot_capture", e));
  }
  function devShape(p: ShotAdded): ShotAdded {
    const kind = devNext;
    devNext = null;
    if (kind === "failed") return { ...p, savedPath: null, error: "save_failed" };
    if (kind === "temp") return { ...p, clip: null, savedPath: null, tempPath: `C:\\Users\\me\\AppData\\Local\\Temp\\Liqlip\\shots\\mock-${++devSeq}.png` };
    return p;
  }

  onMount(() => {
    void settingsGet()
      .then(applySettings)
      .catch((e) => console.error("[shots] settings_get", e));
    const offs = [
      onShotAdded((p) => qao.add(mock ? devShape(p) : p)),
      onShotSession(() => qao.sessionStarted()),
      onShotEnd(() => qao.sessionEnded()),
      onClipsChanged((p) => void qao.clipsChanged(p)),
      onSettingsChanged(applySettings),
    ];
    const onVisibility = () => qao.setHidden(document.hidden);
    document.addEventListener("visibilitychange", onVisibility);
    onVisibility();
    return () => {
      document.removeEventListener("visibilitychange", onVisibility);
      offs.forEach((p) => void p.then((off) => off()));
    };
  });

  // The timers tick only while there is a stack (the window lives hidden between shots).
  const ticking = $derived(qs.items.length > 0);
  $effect(() => {
    if (!ticking) return;
    untrack(() => qao.tick()); // the tick reads the stack — it must not become this effect's dependency
    const timer = setInterval(() => qao.tick(), 100);
    return () => {
      clearInterval(timer);
      qao.stopClock();
    };
  });

  /** WebView2 browser keys (reload, print, …) never run in this window (FIX-15). */
  function onKeydown(e: KeyboardEvent): void {
    if (isBrowserShortcut(e, { find: true })) e.preventDefault();
  }

  const side = $derived(qs.corner === "right" ? 1 : -1);
</script>

<svelte:window onkeydown={onKeydown} />

<main class="stage" class:stage--right={qs.corner === "right"} style:--bleed="{BLEED}px" style:--gap="{GAP}px">
  {#if qs.items.length}
    <section
      class="stack"
      aria-label={t("shots.region")}
      onpointerenter={() => qao.enter()}
      onpointermove={() => qao.enter()}
      onpointerleave={() => qao.leave()}
    >
      {#each qs.items as shot (shot.key)}
        <div
          class="slot"
          animate:flip={{ duration: motionDuration(200), easing: cubicOut }}
          in:fly={{ y: 24, duration: motionDuration(220), opacity: 0 }}
          out:fly={{ x: 24 * side, duration: motionDuration(160), opacity: 0 }}
        >
          <ShotCard {shot} {qao} ocr={qs.ocr} dragging={qs.dragging === shot.key} />
        </div>
      {/each}
    </section>
  {/if}
</main>

{#if mock}
  <div class="dev" role="group" aria-label={t("shots.dev.tools")}>
    <span class="dev__label">{t("shots.dev.tools")}</span>
    <button type="button" onclick={() => devAdd("plain")}>{t("shots.dev.add")}</button>
    <button type="button" onclick={() => devAdd("failed")}>{t("shots.dev.addFailed")}</button>
    <button type="button" onclick={() => devAdd("temp")}>{t("shots.dev.addTemp")}</button>
  </div>
{/if}

<style>
  :global(html),
  :global(body) {
    background: transparent;
    overflow: hidden;
    user-select: none;
  }
  /* The window is the stack plus BLEED on every side (stack.ts → W6-B). The stack is anchored at
     the bottom: the window grows upward and a new card comes in at the bottom. */
  .stage {
    position: fixed;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    justify-content: flex-end;
    padding: var(--bleed);
    pointer-events: none;
  }
  .stage--right {
    align-items: flex-end;
  }
  .stack {
    display: flex;
    flex-direction: column;
    gap: var(--gap);
    min-height: 0;
    max-height: 100%;
    pointer-events: auto;
  }
  /* The slot carries the list motion; the card inside it shrinks with the slot when the work area
     is shorter than the requested stack (flex), never below its own minimum. */
  .slot {
    display: flex;
    flex: 0 1 168px;
    flex-direction: column;
    min-height: 96px;
  }

  /* Browser preview only (the mock): a quiet capsule row at the top-left. */
  .dev {
    position: fixed;
    top: 12px;
    left: 12px;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px;
    border-radius: 999px;
    background: var(--color-raised);
    box-shadow: var(--shadow-pop);
    font-size: var(--text-secondary);
  }
  .dev__label {
    padding: 0 8px 0 10px;
    color: var(--color-ink-3);
    font-family: var(--font-mono);
    font-size: var(--text-meta);
  }
  .dev button {
    height: var(--h-md);
    padding: 0 12px;
    border: 0;
    border-radius: 999px;
    background: var(--color-track);
    color: var(--color-ink);
    font: inherit;
    font-weight: 600;
  }
  .dev button:hover {
    background: var(--color-hover);
  }
  .dev button:active {
    background: var(--color-selected);
  }
</style>
