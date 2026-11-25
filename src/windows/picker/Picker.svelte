<script lang="ts">
  // Thin root of the shelf: the stage (window minus the transparent BLEED margin that holds
  // the shadow), the panel and its parts. State lives in `*.svelte.ts` (shelf, feed,
  // selection, keyboard, overlays); see docs/picker.md.
  import { onMount } from "svelte";
  import Ico from "$lib/components/Ico.svelte";
  import { getLocale, t } from "$lib/i18n/index.svelte";
  import { isTauri } from "$lib/ipc/runtime";
  import SearchBar from "./SearchBar.svelte";
  import Pills from "./Pills.svelte";
  import Grid from "./Grid.svelte";
  import Footer from "./Footer.svelte";
  import ToastDock from "./ToastDock.svelte";
  import DragGhost from "./DragGhost.svelte";
  import OverlayHost from "./overlays/OverlayHost.svelte";
  import { registerDefaultLayers } from "./overlays/defaults";
  import { shelf } from "./shelf.svelte";
  import { keyboard } from "./keyboard.svelte";
  import { drag } from "./drag.svelte";
  import { clockTime } from "./format";

  registerDefaultLayers();

  let panel = $state<HTMLElement | null>(null);
  $effect(() => {
    shelf.panelEl = panel;
    drag.panel = panel;
  });

  onMount(() => {
    // Browser preview only: a neutral backdrop so the panel's shadow reads like on a desktop.
    // (picker.html paints the body transparent inline for the real, transparent window.)
    if (!isTauri()) document.body.style.background = "var(--color-page)";
    let stop: (() => void) | undefined;
    let disposed = false;
    void shelf.start().then((s) => {
      if (disposed) s();
      else stop = s;
    });
    return () => {
      disposed = true;
      stop?.();
      drag.cancel();
    };
  });

  // Close mid-drag (Esc, focus loss) must not leave the gesture armed.
  $effect(() => {
    if (!shelf.open) {
      drag.cancel();
      keyboard.releaseCtrl();
    }
  });

  const pausedText = $derived.by(() => {
    const until = shelf.capture.pausedUntil;
    if (until === null) return "";
    return until === -1 ? t("picker.paused.title") : t("picker.paused.until", { time: clockTime(until, getLocale()) });
  });
</script>

<svelte:window onkeydown={keyboard.onKeydown} onkeyup={keyboard.onKeyup} onblur={keyboard.releaseCtrl} />

<!-- Closed (and before the first `picker:open`): nothing is rendered — the armed hover window
     is visible, transparent and click-through, so any pixel would show at the screen edge. -->
{#if shelf.rendered}
<div class="stage" class:stage--open={shelf.shown} data-edge={shelf.layout.position} style="--bleed:{shelf.layout.bleed}px">
  <section
    bind:this={panel}
    class="panel squircle"
    class:panel--strip={shelf.horizontal}
    style="width:{shelf.layout.width}px;height:{shelf.layout.height}px"
    aria-label={t("picker.panel.label")}
    aria-hidden={!shelf.open}
    inert={!shelf.open}
  >
    <SearchBar />
    {#if shelf.paused}
      <div class="banner" role="status">
        <span class="banner__icon"><Ico name="pause" size={14} /></span>
        <span class="banner__text" title={pausedText}>{pausedText}</span>
        <button type="button" class="banner__btn" onmousedown={(e) => e.preventDefault()} onclick={() => void shelf.resume()}>
          <Ico name="play" size={12} />{t("picker.paused.resume")}
        </button>
      </div>
    {/if}
    <Pills />
    <Grid />
    {#if shelf.settings?.picker.hints ?? true}
      <Footer />
    {/if}
    <ToastDock />
    <OverlayHost />
  </section>

  <!-- The ghost lives at the stage level (outside the panel): the panel's transform would
       clip it and shift it off the cursor. -->
  {#if drag.active}
    <DragGhost />
  {/if}
</div>
{/if}

<style>
  :global(html),
  :global(body) {
    background: transparent;
  }
  :global(#app) {
    height: 100%;
  }

  /* The window is the panel + a transparent BLEED margin on the non-docked sides (all four
     for the centre) so the shadow renders inside the window. In a browser tab the viewport
     is larger: the panel keeps its size from `picker:layout` and docks to the same edge. */
  .stage {
    position: fixed;
    inset: 0;
    display: flex;
    background: transparent;
  }
  .stage[data-edge="left"] {
    align-items: center;
    justify-content: flex-start;
    padding: var(--bleed) var(--bleed) var(--bleed) 0;
  }
  .stage[data-edge="right"] {
    align-items: center;
    justify-content: flex-end;
    padding: var(--bleed) 0 var(--bleed) var(--bleed);
  }
  .stage[data-edge="top"] {
    align-items: flex-start;
    justify-content: center;
    padding: 0 var(--bleed) var(--bleed) var(--bleed);
  }
  .stage[data-edge="bottom"] {
    align-items: flex-end;
    justify-content: center;
    padding: var(--bleed) var(--bleed) 0 var(--bleed);
  }
  .stage[data-edge="center"] {
    align-items: center;
    justify-content: center;
    padding: var(--bleed);
  }

  /* Panel: one geometry for every position — radius --r-panel (28), padding --pad-panel (12)
     on all sides, so whatever sits in its corners (the settings circle, the hint-row
     capsules, a sheet) is concentric. No overflow clipping: rings, lifts and shadows of the
     content are never cut; scroll areas clip themselves. */
  .panel {
    position: relative;
    display: flex;
    flex-direction: column;
    max-width: 100%;
    max-height: 100%;
    padding: var(--pad-panel);
    border-radius: var(--r-panel);
    background: var(--color-paper);
    box-shadow: var(--shadow-summoned);
    opacity: 0;
    pointer-events: none;
    /* Exit: the soft ease (a dismiss never bounces). */
    transition:
      transform var(--dur-base) var(--ease-in-out),
      opacity var(--dur-base) var(--ease-out-soft);
  }
  .stage[data-edge="center"] .panel {
    transform: scale(0.92);
  }
  /* Edges slide out of the docked screen edge (no overshoot: it would open a gap at the edge). */
  .stage[data-edge="left"] .panel {
    transform: translateX(calc(-100% - 4px));
  }
  .stage[data-edge="right"] .panel {
    transform: translateX(calc(100% + 4px));
  }
  .stage[data-edge="top"] .panel {
    transform: translateY(calc(-100% - 4px));
  }
  .stage[data-edge="bottom"] .panel {
    transform: translateY(calc(100% + 4px));
  }
  .stage--open[data-edge] .panel {
    opacity: 1;
    pointer-events: auto;
    transform: translate(0, 0) scale(1);
    transition:
      transform var(--dur-slow) var(--ease-out-soft),
      opacity var(--dur-fast) var(--ease-out-soft);
  }
  /* Centre: summoned, never hovered — it settles with the liquid overshoot. */
  .stage--open[data-edge="center"] .panel {
    transition:
      transform var(--dur-slow) var(--ease-liquid),
      opacity var(--dur-base) var(--ease-out-soft);
  }

  /* "Recording is paused · Resume" under the search (SPEC §2.4.4, §3.13): a 32 px capsule
     with a 24 px capsule button inset 4 px (concentric). */
  .banner {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    gap: 8px;
    height: var(--h-md);
    margin-top: 8px;
    padding: 0 4px 0 12px;
    border-radius: 999px;
    background: var(--color-track);
    color: var(--color-ink-2);
    font-size: var(--text-secondary);
    animation: banner-in var(--dur-base) var(--ease-out-soft);
  }
  @keyframes banner-in {
    from {
      opacity: 0;
      transform: translateY(-4px);
    }
  }
  .banner__icon {
    display: inline-flex;
    flex: 0 0 auto;
  }
  .banner__text {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    color: var(--color-ink);
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .banner__btn {
    display: inline-flex;
    flex: 0 0 auto;
    align-items: center;
    gap: 5px;
    height: var(--h-sm);
    padding: 0 10px 0 8px;
    border: 0;
    border-radius: 999px;
    background: var(--color-ink);
    color: var(--color-paper);
    font: inherit;
    font-size: var(--text-secondary);
    font-weight: 600;
    transition:
      background-color var(--dur-fast) var(--ease-out-soft),
      transform var(--dur-fast) var(--ease-out-soft);
  }
  .banner__btn:hover {
    background: var(--color-ink-hover);
  }
  .banner__btn:active {
    background: var(--color-ink-active);
    transform: scale(0.97);
  }
</style>
