<script lang="ts">
  // A pinned picture `pin-<n>` (SPEC §9.5, ARCHITECTURE §18.8). Rust (W6-B) builds the window on
  // demand — the picture's size × scale + PIN_BLEED (24 px) of transparent margin on every side
  // for the shadow — and destroys it on close. Here: `shot_pin_get()` → the picture; drag the
  // window (`startDragging()` on press, like Tauri's own drag region: a double click arrives as
  // the second press with `detail === 2`); wheel → `shot_pin_resize({ scale })` (absolute,
  // pinMath.ts); Ctrl+wheel → opacity 20–100 % (CSS); double click → 100 % ↔ the opening size;
  // Esc / × (on hover) → `shot_pin_close()`; right click → a menu in the shelf's menu style.
  // Browser preview on the mock: /pin.html?clip=<id> — the picture is scaled inside the page.
  import { onMount, tick } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { isBrowserShortcut } from "$lib/browserKeys";
  import Ico from "$lib/components/Ico.svelte";
  import type { IconName } from "$lib/icons";
  import { t, type TranslationKey } from "$lib/i18n/index.svelte";
  import { clipCopy, shotEdit, shotPinClose, shotPinGet, shotPinResize, shotSaveAs } from "$lib/ipc/commands";
  import { errorCode } from "$lib/ipc/errors";
  import { isTauri } from "$lib/ipc/runtime";
  import type { PinPayload } from "$lib/ipc/types";
  import LayerItem from "../picker/overlays/kit/LayerItem.svelte";
  import {
    MENU_SIZE,
    PIN_BLEED,
    cornerRadius,
    fadeBy,
    fitScale,
    menuMode,
    placeMenu,
    scaleOfWindow,
    toggleActual,
    wheelPixels,
    zoomBy,
    type MenuMode,
  } from "./pinMath";

  const tauri = isTauri();

  let payload = $state<PinPayload | null>(null);
  let status = $state<"loading" | "ready" | "empty">("loading");
  let loaded = $state(false);
  let broken = $state(false);
  let closed = $state(false);

  let vw = $state(typeof window !== "undefined" ? window.innerWidth : 0);
  let vh = $state(typeof window !== "undefined" ? window.innerHeight : 0);
  let dpr = $state(typeof window !== "undefined" ? window.devicePixelRatio || 1 : 1);

  /** Current scale (physical px per picture px) and the one the pin opened at. */
  let scale = $state(1);
  let initial = 1;
  let opacity = $state(1);

  /** The picture box, logical px: the window minus the bleed (Tauri), or the scaled picture
   *  inside the page (browser preview). */
  const box = $derived.by(() => {
    if (!payload) return { w: 0, h: 0 };
    if (tauri) return { w: Math.max(0, vw - 2 * PIN_BLEED), h: Math.max(0, vh - 2 * PIN_BLEED) };
    return { w: (payload.width * scale) / dpr, h: (payload.height * scale) / dpr };
  });
  const radius = $derived(cornerRadius(box.w, box.h));
  /** × fits (24 px, inset 4) only on a pin of at least 56 px. */
  const closable = $derived(Math.min(box.w, box.h) >= 56);

  onMount(() => {
    shotPinGet()
      .then((p) => {
        payload = p;
        status = p ? "ready" : "empty";
        if (!p) return;
        initial = tauri ? scaleOfWindow(window.innerWidth, window.devicePixelRatio || 1, p.width) : fitScale(window.innerWidth, window.innerHeight, window.devicePixelRatio || 1, p.width, p.height);
        scale = initial;
      })
      .catch((e) => {
        console.error("[pin] shot_pin_get", e);
        status = "empty";
      });
  });

  /** A picture already decoded before the listener was attached counts as loaded. */
  function settle(img: HTMLImageElement): void {
    if (img.complete && img.naturalWidth > 0) loaded = true;
  }

  // ── scale (coalesced: one resize in flight, the latest wins) ──────────────
  let inflight = false;
  let wanted: number | null = null;
  async function pump(): Promise<void> {
    if (inflight || wanted === null) return;
    const s = wanted;
    wanted = null;
    inflight = true;
    try {
      await shotPinResize(s);
    } catch (e) {
      console.error("[pin] shot_pin_resize", e);
    } finally {
      inflight = false;
      void pump();
    }
  }
  function setScale(next: number): void {
    if (next === scale) return;
    scale = next;
    flash(t("pin.zoom", { value: Math.round(next * 100) }));
    if (!tauri) return;
    wanted = next;
    void pump();
  }
  function onResize(): void {
    vw = window.innerWidth;
    vh = window.innerHeight;
    dpr = window.devicePixelRatio || 1;
    // Rust may clamp the size (monitor, limits): the window is the truth once it settles.
    if (tauri && payload && !inflight && wanted === null) scale = scaleOfWindow(vw, dpr, payload.width);
  }

  // ── badge (zoom / opacity) and toast ──────────────────────────────────────
  let badge = $state<string | null>(null);
  let badgeTimer: ReturnType<typeof setTimeout> | undefined;
  function flash(text: string): void {
    badge = text;
    clearTimeout(badgeTimer);
    badgeTimer = setTimeout(() => (badge = null), 900);
  }
  let toast = $state<{ key: TranslationKey; icon: IconName; error: boolean } | null>(null);
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  function say(key: TranslationKey, icon: IconName, error = false): void {
    toast = { key, icon, error };
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = null), error ? 2600 : 1600);
  }
  function failed(e: unknown, key: TranslationKey): void {
    console.error("[pin]", key, e);
    say(errorCode(e) === "not_found" ? "pin.gone" : key, "warning", true);
  }

  // ── actions ───────────────────────────────────────────────────────────────
  type Cmd = "copy" | "edit" | "saveAs" | "close";
  const ITEMS: { cmd: Cmd; label: TranslationKey; icon: IconName; keys?: string }[] = [
    { cmd: "copy", label: "pin.copy", icon: "copy" },
    { cmd: "edit", label: "pin.edit", icon: "edit" },
    { cmd: "saveAs", label: "pin.saveAs", icon: "download" },
    { cmd: "close", label: "pin.close", icon: "x", keys: "Esc" },
  ];

  async function close(): Promise<void> {
    menu = null;
    try {
      await shotPinClose();
    } catch (e) {
      console.error("[pin] shot_pin_close", e);
    }
    if (!tauri) closed = true; // the browser preview has no window to destroy
  }

  async function run(cmd: Cmd): Promise<void> {
    menu = null;
    if (cmd === "close") return close();
    const id = payload?.clipId;
    if (id === undefined) return;
    if (cmd === "copy") {
      try {
        const out = await clipCopy({ ids: [id], mode: "auto" });
        if (out.result === "failed") say(out.reason === "not_found" ? "pin.gone" : "pin.copyFailed", "warning", true);
        else say("pin.copied", "check");
      } catch (e) {
        failed(e, "pin.copyFailed");
      }
    } else if (cmd === "edit") {
      try {
        await shotEdit(id);
      } catch (e) {
        failed(e, "pin.editFailed");
      }
    } else {
      try {
        const path = await shotSaveAs(id);
        if (path) say("pin.saved", "check"); // null = the dialog was cancelled
      } catch (e) {
        failed(e, "pin.saveFailed");
      }
    }
  }

  // ── pointer ───────────────────────────────────────────────────────────────
  function onMouseDown(e: MouseEvent): void {
    if (e.button !== 0 || status !== "ready") return;
    const target = e.target as Element;
    if (target.closest(".menu, .close, .fallback")) return;
    if (menu) {
      menu = null;
      return;
    }
    if (e.detail === 2) {
      setScale(toggleActual(scale, initial));
      return;
    }
    if (e.detail === 1 && tauri) {
      e.preventDefault();
      void getCurrentWindow()
        .startDragging()
        .catch((err) => console.error("[pin] startDragging", err));
    }
  }

  function onWheel(e: WheelEvent): void {
    e.preventDefault();
    if (status !== "ready" || menu) return;
    const px = wheelPixels(e.deltaY, e.deltaMode);
    if (px === 0) return;
    if (e.ctrlKey) {
      opacity = fadeBy(opacity, px);
      flash(t("pin.opacity", { value: Math.round(opacity * 100) }));
    } else {
      setScale(zoomBy(scale, px));
    }
  }

  // ── context menu ──────────────────────────────────────────────────────────
  let menu = $state<{ x: number; y: number; mode: MenuMode } | null>(null);
  let active = $state(0);
  let menuEl = $state<HTMLDivElement | null>(null);

  async function openMenu(e: MouseEvent): Promise<void> {
    e.preventDefault();
    if (status !== "ready") return;
    const view = { w: window.innerWidth, h: window.innerHeight };
    const mode = menuMode(view);
    active = 0;
    menu = { ...placeMenu({ x: e.clientX, y: e.clientY }, MENU_SIZE[mode], view), mode };
    await tick();
    // Place again with the measured size (labels differ by language).
    const r = menuEl?.getBoundingClientRect();
    if (menu && r) menu = { ...placeMenu({ x: e.clientX, y: e.clientY }, { w: r.width, h: r.height }, view), mode };
  }

  function onKeydown(e: KeyboardEvent): void {
    if (isBrowserShortcut(e, { find: true })) {
      e.preventDefault();
      return;
    }
    if (menu) {
      const n = ITEMS.length;
      const next = menu.mode === "row" ? "ArrowRight" : "ArrowDown";
      const prev = menu.mode === "row" ? "ArrowLeft" : "ArrowUp";
      if (e.code === "Escape") menu = null;
      else if (e.code === next) active = (active + 1) % n;
      else if (e.code === prev) active = (active - 1 + n) % n;
      else if (e.code === "Home") active = 0;
      else if (e.code === "End") active = n - 1;
      else if (e.code === "Enter" || e.code === "NumpadEnter" || e.code === "Space") void run(ITEMS[active].cmd);
      else return;
      e.preventDefault();
      return;
    }
    if (e.code === "Escape") {
      e.preventDefault();
      void close();
    }
  }
</script>

<svelte:window onkeydown={onKeydown} onresize={onResize} onblur={() => (menu = null)} />

{#if !closed}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <main
    class="stage"
    style:--bleed="{PIN_BLEED}px"
    title={status === "ready" && !menu ? t("pin.hint") : undefined}
    aria-label={t("pin.label")}
    onmousedown={onMouseDown}
    onwheel={onWheel}
    oncontextmenu={openMenu}
  >
    {#if status === "loading"}
      <span class="skeleton squircle" class:skeleton--browser={!tauri} aria-busy="true" aria-label={t("common.loading")}></span>
    {:else if status === "empty" || broken}
      <div class="fallback squircle" role="alert">
        <Ico name="image" size={20} />
        <span class="fallback__text">{t("pin.unavailable")}</span>
        <button type="button" class="fallback__close" onclick={() => void close()}>{t("pin.close")}</button>
      </div>
    {:else if payload}
      <div class="frame" style:width="{box.w}px" style:height="{box.h}px">
        <div class="pic squircle" style:border-radius="{radius}px" style:opacity>
          {#if !loaded}<span class="pic__skeleton" aria-hidden="true"></span>{/if}
          <img
            src={payload.imageUrl}
            alt={t("pin.label")}
            draggable="false"
            class:in={loaded}
            use:settle
            onload={() => (loaded = true)}
            onerror={() => (broken = true)}
          />
        </div>
        {#if closable}
          <button type="button" class="close" title={t("pin.close")} aria-label={t("pin.close")} onclick={() => void close()}>
            <Ico name="x" size={14} />
          </button>
        {/if}
        {#if badge}<span class="badge" aria-live="polite">{badge}</span>{/if}
        {#if toast}
          <div class="toast-dock">
            <div class="toast" class:toast--error={toast.error} role={toast.error ? "alert" : "status"}>
              <Ico name={toast.icon} size={14} />
              <span class="toast__text">{t(toast.key)}</span>
            </div>
          </div>
        {/if}
      </div>
    {/if}

    {#if menu}
      <div
        bind:this={menuEl}
        class="menu menu--{menu.mode}"
        class:squircle={menu.mode === "list"}
        role="menu"
        aria-label={t("pin.menu")}
        aria-orientation={menu.mode === "row" ? "horizontal" : "vertical"}
        style:left="{menu.x}px"
        style:top="{menu.y}px"
      >
        {#if menu.mode === "list"}
          {#each ITEMS as item, i (item.cmd)}
            <LayerItem
              label={t(item.label)}
              icon={item.icon}
              keys={item.keys}
              active={active === i}
              onpointermove={() => (active = i)}
              onclick={() => void run(item.cmd)}
            />
          {/each}
        {:else}
          {#each ITEMS as item, i (item.cmd)}
            <button
              type="button"
              role="menuitem"
              class="menu__icon"
              class:menu__icon--active={active === i}
              title={t(item.label)}
              aria-label={t(item.label)}
              onmousedown={(e) => e.preventDefault()}
              onpointermove={() => (active = i)}
              onclick={() => void run(item.cmd)}
            >
              <Ico name={item.icon} size={16} />
            </button>
          {/each}
        {/if}
      </div>
    {/if}
  </main>
{/if}

<style>
  :global(html),
  :global(body) {
    background: transparent;
    overflow: hidden;
    user-select: none;
  }
  /* Window = picture + PIN_BLEED on every side (the shadow field; W6-B sizes it). */
  .stage {
    position: fixed;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--bleed);
  }

  .frame {
    position: relative;
    flex: none;
  }
  /* The picture: a squircle (r16, smaller on a tiny pin) lifted by --shadow-lift; Ctrl+wheel
     fades it together with its shadow. The × and the menu stay opaque. */
  .pic {
    position: relative;
    width: 100%;
    height: 100%;
    overflow: hidden;
    box-shadow: var(--shadow-lift);
    transition: opacity var(--dur-fast) var(--ease-out-soft);
  }
  .pic img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: contain;
    opacity: 0;
    transition: opacity var(--dur-base) var(--ease-out-soft);
  }
  .pic img.in {
    opacity: 1;
  }
  .pic__skeleton,
  .skeleton {
    background: var(--color-track);
    animation: pulse 1.4s var(--ease-in-out) infinite;
  }
  .pic__skeleton {
    position: absolute;
    inset: 0;
  }
  .skeleton {
    width: 100%;
    height: 100%;
    border-radius: var(--r-card);
  }
  .skeleton--browser {
    flex: none;
    width: 240px;
    height: 160px;
  }
  @keyframes pulse {
    50% {
      opacity: 0.5;
    }
  }

  /* × — an ink circle 24 px in the top-right corner, inset 4 (12 + 4 = 16), on hover only. */
  .close {
    position: absolute;
    top: 4px;
    right: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    padding: 0;
    border: 0;
    border-radius: 999px;
    background: var(--color-ink);
    color: color-mix(in srgb, var(--color-paper) 78%, transparent);
    box-shadow: var(--shadow-on-lift);
    opacity: 0;
    transition:
      opacity var(--dur-fast) var(--ease-out-soft),
      background-color var(--dur-fast) var(--ease-out-soft),
      color var(--dur-fast) var(--ease-out-soft),
      transform var(--dur-fast) var(--ease-out-soft);
  }
  .stage:hover .close,
  .close:focus-visible {
    opacity: 1;
  }
  .close:hover {
    background: var(--color-ink-hover);
    color: var(--color-paper);
  }
  .close:active {
    transform: scale(0.9);
  }
  .close:focus-visible {
    outline-color: var(--color-paper);
    outline-offset: -2px;
  }

  /* Zoom / opacity readout — a capsule in the middle (tabular figures: the width holds still). */
  .badge {
    position: absolute;
    top: 50%;
    left: 50%;
    padding: 6px 12px;
    border-radius: 999px;
    background: var(--color-raised);
    box-shadow: var(--shadow-pop);
    color: var(--color-ink);
    font-size: var(--text-secondary);
    font-variant-numeric: tabular-nums;
    font-weight: 600;
    white-space: nowrap;
    transform: translate(-50%, -50%);
    pointer-events: none;
    animation: fade-in var(--dur-fast) var(--ease-out-soft);
  }
  .toast-dock {
    position: absolute;
    right: 8px;
    bottom: 8px;
    left: 8px;
    display: flex;
    justify-content: center;
    pointer-events: none;
  }
  .toast {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    max-width: 100%;
    height: var(--h-md);
    padding: 0 14px 0 10px;
    border-radius: 999px;
    background: var(--color-raised);
    box-shadow: var(--shadow-pop);
    color: var(--color-ink);
    font-size: var(--text-secondary);
    font-weight: 600;
    white-space: nowrap;
    animation: fade-in var(--dur-base) var(--ease-out-soft);
  }
  .toast--error :global(.ico) {
    color: var(--color-danger);
  }
  .toast__text {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  @keyframes fade-in {
    from {
      opacity: 0;
    }
  }

  /* No picture: a raised squircle with the reason and a way out. */
  .fallback {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    max-width: 100%;
    padding: 16px;
    border-radius: var(--r-card);
    background: var(--color-raised);
    box-shadow: var(--shadow-lift);
    color: var(--color-ink-3);
    text-align: center;
  }
  .fallback__text {
    color: var(--color-ink);
    font-size: var(--text-secondary);
    font-weight: 600;
  }
  .fallback__close {
    height: var(--h-md);
    padding: 0 14px;
    border: 0;
    border-radius: 999px;
    background: var(--color-track);
    color: var(--color-ink);
    font: inherit;
    font-size: var(--text-secondary);
    font-weight: 600;
  }
  .fallback__close:hover {
    background: var(--color-hover);
  }
  .fallback__close:active {
    background: var(--color-selected);
  }

  /* The menu — the shelf's action-menu geometry: raised squircle r22, padding 6, 32 px capsule
     rows (LayerItem, --layer-r-in 16). Compact forms are a capsule of 32 px circles (40, pad 4). */
  .menu {
    --layer-r-in: max(4px, calc(var(--r-layer) - var(--pad-layer)));
    position: fixed;
    z-index: 10;
    display: flex;
    background: var(--color-raised);
    box-shadow: var(--shadow-pop);
    color: var(--color-ink);
    animation: menu-in var(--dur-base) var(--ease-out-soft);
  }
  .menu--list {
    flex-direction: column;
    min-width: 176px;
    padding: var(--pad-layer);
    border-radius: var(--r-layer);
  }
  .menu--row,
  .menu--column {
    padding: 4px;
    border-radius: 999px;
  }
  .menu--column {
    flex-direction: column;
  }
  @keyframes menu-in {
    from {
      opacity: 0;
      transform: translateY(-4px) scale(0.98);
    }
  }
  .menu__icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    padding: 0;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: var(--color-ink-2);
    transition: background-color var(--dur-fast) var(--ease-out-soft);
  }
  .menu__icon--active {
    background: var(--color-track);
    color: var(--color-ink);
  }
  .menu__icon:active {
    background: var(--color-hover);
  }
  .menu__icon:focus-visible {
    outline-offset: -2px;
  }
</style>
