<script lang="ts">
  // One QAO card (SPEC §9.4): a raised squircle r16 with a 4 px frame around the picture well
  // (r12 — concentric). The picture is the click target (→ editor) and the drag source (> 6 px →
  // PNG file into any app). On hover: the tools capsule in the middle (ink, 40 px, 32 px circles
  // inset 4 — the toast geometry) — Copy · Edit · Pin · Save | Show in folder · Text · Delete —
  // and × in the top-left corner (24 px, inset 4: 12 + 4 = 16). The timer strip, the toasts and
  // the save notice live INSIDE the card, never over a neighbour (LT defect П-16). The window is
  // never activated, so there are no keys; every button has an `aria-label`.
  import Ico from "$lib/components/Ico.svelte";
  import type { IconName } from "$lib/icons";
  import { t, type TranslationKey } from "$lib/i18n/index.svelte";
  import PinGlyph from "./glyphs/pin.svelte";
  import type { OcrGate, Qao, ShotCmd } from "./qao";
  import { progress, type Shot } from "./stack";

  interface Props {
    shot: Shot;
    qao: Qao;
    ocr: OcrGate;
    dragging: boolean;
  }
  let { shot, qao, ocr, dragging }: Props = $props();

  const clip = $derived(shot.clip);
  const hasRecord = $derived(clip !== null);

  // ── the picture ────────────────────────────────────────────────────────────
  // Keyed by URL (not reset by an effect, which could run after a cached picture's `load`).
  let loadedSrc = $state<string | null>(null);
  let brokenSrc = $state<string | null>(null);
  const loaded = $derived(shot.preview !== null && loadedSrc === shot.preview);
  const broken = $derived(shot.preview !== null && brokenSrc === shot.preview);
  /** A picture already decoded before the listener was attached counts as loaded. */
  function settle(img: HTMLImageElement): void {
    if (img.complete && img.naturalWidth > 0) loadedSrc = img.getAttribute("src");
  }

  /** Well aspect 248 × 160; extreme shapes are shown whole (as the shelf's ImageCard does). */
  const BOX = 248 / 160;
  const fit = $derived.by(() => {
    const w = clip?.width;
    const h = clip?.height;
    if (!w || !h) return "cover";
    const r = w / h;
    return r > BOX * 2.2 || r < BOX / 2.6 || (w < 120 && h < 80) ? "contain" : "cover";
  });

  // ── the tools ──────────────────────────────────────────────────────────────
  interface Tool {
    cmd: ShotCmd;
    label: TranslationKey;
    icon: IconName | "pin";
    disabled?: boolean;
    hint?: TranslationKey;
  }
  const tools = $derived.by((): Tool[] => [
    { cmd: "copy", label: "shots.copy", icon: "copy" },
    { cmd: "edit", label: "shots.edit", icon: "edit" },
    { cmd: "pin", label: "shots.pin", icon: "pin" },
    shot.savedPath ? { cmd: "reveal", label: "shots.reveal", icon: "folderOpen" } : { cmd: "save", label: "shots.save", icon: "download" },
    {
      cmd: "text",
      label: "shots.text",
      icon: "ocr",
      disabled: ocr !== "ready",
      hint: ocr === "off" ? "shots.textOff" : ocr === "nolang" ? "shots.textNoLang" : undefined,
    },
  ]);

  function run(cmd: ShotCmd): void {
    void qao.act(shot.key, cmd);
  }

  // ── click vs drag-out (qao.ts owns the lifecycle) ──────────────────────────
  function onDown(e: PointerEvent): void {
    if (qao.press(shot.key, { button: e.button, pointerId: e.pointerId, x: e.clientX, y: e.clientY })) {
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    }
  }
  function onMove(e: PointerEvent): void {
    if (qao.move(shot.key, { buttons: e.buttons, pointerId: e.pointerId, x: e.clientX, y: e.clientY })) {
      const el = e.currentTarget as HTMLElement;
      if (el.hasPointerCapture(e.pointerId)) el.releasePointerCapture(e.pointerId);
    }
  }
  function onUp(e: PointerEvent): void {
    qao.release(shot.key, { pointerId: e.pointerId, x: e.clientX, y: e.clientY });
  }
</script>

<div
  class="card squircle"
  class:card--dragging={dragging}
  class:card--notice={shot.error !== null}
  role="group"
  aria-label={t("shots.card")}
>
  <button
    type="button"
    class="thumb squircle"
    class:thumb--empty={!shot.preview || broken}
    disabled={!hasRecord}
    title={hasRecord ? t("shots.thumb") : t("shots.notKept.desc")}
    aria-label={hasRecord ? t("shots.thumb") : t("shots.notKept")}
    onpointerdown={onDown}
    onpointermove={onMove}
    onpointerup={onUp}
    onpointercancel={() => qao.cancel()}
    onlostpointercapture={() => qao.cancel()}
    onclick={(e) => {
      if (e.detail === 0) run("edit"); // keyboard activation (browser preview only)
    }}
  >
    {#if shot.preview && !broken}
      {#if !loaded}<span class="skeleton" aria-hidden="true"></span>{/if}
      <img
        class="shot"
        class:shot--in={loaded}
        style:object-fit={fit}
        src={shot.preview}
        alt=""
        draggable="false"
        decoding="async"
        use:settle
        onload={(e) => (loadedSrc = (e.currentTarget as HTMLImageElement).getAttribute("src"))}
        onerror={(e) => (brokenSrc = (e.currentTarget as HTMLImageElement).getAttribute("src"))}
      />
    {:else}
      <span class="empty">
        <Ico name="image" size={22} />
        {#if !hasRecord}<span class="empty__text">{t("shots.notKept")}</span>{/if}
      </span>
    {/if}
  </button>

  <span class="veil" aria-hidden="true"></span>

  {#if hasRecord}
    <div class="tools">
      {#each tools as tool (tool.cmd)}
        <button
          type="button"
          class="tool"
          disabled={tool.disabled || (shot.busy !== null && shot.busy !== tool.cmd)}
          aria-busy={shot.busy === tool.cmd || undefined}
          title={tool.hint ? `${t(tool.label)} — ${t(tool.hint)}` : t(tool.label)}
          aria-label={t(tool.label)}
          onclick={() => run(tool.cmd)}
        >
          {#if shot.busy === tool.cmd}
            <span class="spin"><Ico name="loaderCircle" size={16} /></span>
          {:else if tool.icon === "pin"}
            <span class="ico ico--static" style="font-size:16px" aria-hidden="true"><PinGlyph /></span>
          {:else}
            <Ico name={tool.icon} size={16} />
          {/if}
        </button>
      {/each}
      <button
        type="button"
        class="tool tool--delete"
        disabled={shot.busy !== null && shot.busy !== "delete"}
        aria-busy={shot.busy === "delete" || undefined}
        title={t("shots.delete")}
        aria-label={t("shots.delete")}
        onclick={() => run("delete")}
      >
        {#if shot.busy === "delete"}<span class="spin"><Ico name="loaderCircle" size={16} /></span>{:else}<Ico name="trash" size={16} />{/if}
      </button>
    </div>
  {/if}

  <button type="button" class="close" title={t("shots.close")} aria-label={t("shots.close")} onclick={() => run("close")}>
    <Ico name="x" size={14} />
  </button>

  {#if shot.timeout > 0 && shot.error === null}
    <span class="timer" aria-hidden="true"
      ><span class="timer__track"><span class="timer__fill" style:transform="scaleX({progress(shot)})"></span></span></span
    >
  {/if}

  {#if shot.toast}
    <div class="toast-dock">
      <div class="toast" class:toast--error={shot.toast.error} role={shot.toast.error ? "alert" : "status"}>
        <Ico name={shot.toast.icon} size={14} />
        <span class="toast__text">{t(shot.toast.key)}</span>
      </div>
    </div>
  {/if}

  {#if shot.error === "save_failed"}
    <div class="notice squircle" role="alert">
      <span class="notice__msg"><Ico name="warning" size={14} /><span>{t("shots.saveFailed")}</span></span>
      <button type="button" class="notice__action" onclick={() => run("folder")}>{t("shots.changeFolder")}</button>
    </div>
  {/if}
</div>

<style>
  /* Card = raised squircle r16 (--r-card) + 4 px frame (--pad-card) → the well is r12. It fills
     its slot (Shots.svelte), which shrinks when the work area is shorter than the stack. */
  .card {
    position: relative;
    flex: 1 1 auto;
    min-height: 0;
    width: 256px;
    padding: var(--pad-card);
    border-radius: var(--r-card);
    background: var(--color-raised);
    box-shadow: var(--shadow-pop);
    transition: opacity var(--dur-fast) var(--ease-out-soft);
  }
  .card--dragging {
    opacity: 0.45;
  }

  .thumb {
    position: relative;
    display: block;
    width: 100%;
    height: 100%;
    padding: 0;
    overflow: hidden; /* the picture follows the well's squircle; nothing with a shadow is inside */
    border: 0;
    border-radius: var(--r-card-in);
    background: var(--color-card);
    cursor: pointer;
  }
  .thumb:disabled {
    cursor: default;
  }
  .thumb:focus-visible {
    outline-offset: -2px;
  }
  .skeleton {
    position: absolute;
    inset: 0;
    background: var(--color-track);
    animation: pulse 1.4s var(--ease-in-out) infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.5;
    }
  }
  .shot {
    display: block;
    width: 100%;
    height: 100%;
    opacity: 0;
    pointer-events: none;
    transition: opacity var(--dur-base) var(--ease-out-soft);
  }
  .shot--in {
    opacity: 1;
  }
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    height: 100%;
    color: var(--color-ink-3);
  }
  .empty__text {
    font-size: var(--text-secondary);
    font-weight: 500;
  }

  /* Hover: a light veil over the picture (half the scrim) so the tools read on any shot. */
  .veil {
    position: absolute;
    inset: var(--pad-card);
    border-radius: var(--r-card-in);
    background: var(--color-scrim);
    opacity: 0;
    pointer-events: none;
    transition: opacity var(--dur-fast) var(--ease-out-soft);
  }
  .card:hover .veil,
  .card:focus-within .veil {
    opacity: 0.5;
  }

  /* Tools: an ink capsule (40 px, padding 4, 32 px circles — the toast geometry), centred. */
  .tools,
  .close {
    opacity: 0;
    pointer-events: none;
    transition: opacity var(--dur-fast) var(--ease-out-soft);
  }
  .card:hover .tools,
  .card:focus-within .tools,
  .card:hover .close,
  .card:focus-within .close {
    opacity: 1;
    pointer-events: auto;
  }
  .card--dragging .tools,
  .card--dragging .close {
    opacity: 0;
    pointer-events: none;
  }
  .tools {
    position: absolute;
    top: 50%;
    left: 50%;
    z-index: 2;
    display: flex;
    align-items: center;
    height: 40px;
    padding: 4px;
    border-radius: 999px;
    background: var(--color-ink);
    box-shadow: var(--shadow-on-lift);
    transform: translate(-50%, -50%);
  }
  /* With the save notice at the bottom the tools move into the free space above it. */
  .card--notice .tools {
    top: 26%;
  }
  .tool,
  .close {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    justify-content: center;
    padding: 0;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: color-mix(in srgb, var(--color-paper) 78%, transparent);
    transition:
      background-color var(--dur-fast) var(--ease-out-soft),
      color var(--dur-fast) var(--ease-out-soft),
      transform var(--dur-fast) var(--ease-out-soft),
      opacity var(--dur-fast) var(--ease-out-soft);
  }
  .tool {
    width: 32px;
    height: 32px;
  }
  .tool--delete {
    margin-left: 6px; /* destructive action set apart by space (no dividers) */
  }
  .tool:hover:not(:disabled),
  .close:hover {
    background: color-mix(in srgb, var(--color-paper) 18%, transparent);
    color: var(--color-paper);
  }
  .tool:active:not(:disabled),
  .close:active {
    transform: scale(0.9);
  }
  .tool:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .tool[aria-busy="true"] {
    opacity: 1;
    color: var(--color-paper);
  }
  .tool:focus-visible,
  .close:focus-visible {
    outline-color: var(--color-paper);
    outline-offset: -2px;
  }
  .spin {
    display: inline-flex;
    animation: spin 0.9s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  /* × — an ink circle 24 px in the top-left corner, inset 4 (12 + 4 = 16: concentric). */
  .close {
    position: absolute;
    top: var(--pad-card);
    left: var(--pad-card);
    z-index: 2;
    width: calc(2 * var(--r-card-in));
    height: calc(2 * var(--r-card-in));
    background: var(--color-ink);
    box-shadow: var(--shadow-on-lift);
  }
  .close:hover {
    background: var(--color-ink-hover);
  }

  /* Timer: a 3 px strip on its own raised capsule plate (9 px, padding 3) along the bottom of the
     well — its own surface, so it reads on any shot, light or dark, in both themes. */
  .timer {
    position: absolute;
    right: 12px;
    bottom: 8px;
    left: 12px;
    height: 9px;
    padding: 3px;
    border-radius: 999px;
    background: var(--color-raised);
    box-shadow: var(--shadow-on-lift);
    pointer-events: none;
  }
  .timer__track {
    display: block;
    height: 100%;
    overflow: hidden;
    border-radius: 999px;
    background: var(--color-track);
  }
  .timer__fill {
    display: block;
    height: 100%;
    border-radius: 999px;
    background: var(--color-ink);
    transform-origin: left center;
    transition: transform 100ms linear;
  }

  /* Toast — a 32 px capsule inside the card, above the timer strip. */
  .toast-dock {
    position: absolute;
    right: 12px;
    bottom: 22px;
    left: 12px;
    z-index: 3;
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
    animation: toast-in var(--dur-base) var(--ease-liquid);
  }
  .toast--error :global(.ico) {
    color: var(--color-danger);
  }
  .toast__text {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  @keyframes toast-in {
    from {
      opacity: 0;
      transform: translateY(6px) scale(0.96);
    }
  }

  /* Save failed — a raised squircle at the bottom, inset 4 (r12: concentric with the card);
     it stays until the card is closed (an unseen failed save must not vanish). */
  .notice {
    position: absolute;
    right: calc(2 * var(--pad-card));
    bottom: calc(2 * var(--pad-card));
    left: calc(2 * var(--pad-card));
    z-index: 3;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 6px 8px;
    padding: 8px 8px 8px 12px;
    border-radius: calc(var(--r-card-in) - var(--pad-card));
    background: var(--color-raised);
    box-shadow: var(--shadow-pop);
    color: var(--color-ink);
    font-size: var(--text-secondary);
    font-weight: 600;
    animation: toast-in var(--dur-base) var(--ease-liquid);
  }
  .notice__msg {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .notice__msg :global(.ico) {
    color: var(--color-danger);
  }
  .notice__action {
    height: var(--h-sm);
    margin-left: auto;
    padding: 0 12px;
    border: 0;
    border-radius: 999px;
    background: var(--color-ink);
    color: var(--color-paper);
    font: inherit;
    font-weight: 600;
    transition:
      background-color var(--dur-fast) var(--ease-out-soft),
      transform var(--dur-fast) var(--ease-out-soft);
  }
  .notice__action:hover {
    background: var(--color-ink-hover);
  }
  .notice__action:active {
    background: var(--color-ink-active);
    transform: scale(0.97);
  }
  .notice__action:focus-visible {
    outline-offset: 2px;
  }
</style>
