<script lang="ts">
  // Hint row (SPEC §3.1): "↵ Paste into <icon> <app>" for the real target (no target →
  // "↵ Copy"; elevated → "↵ Copy · administrator window"), ⇧↵ plain text, Space preview,
  // Ctrl K actions; on the right the pause indicator (click → resume) and "Stack N" (click →
  // the stack popover, again → closes it; also a drop target for dragged cards; shown at 0
  // while "Collect new copies" is on, with a pulsing dot). A 32 px row with 4 px side
  // padding: its 24 px keycaps / status capsules sit 16 px from the panel's bottom corners —
  // concentric (12 + 16 = 28). Labels shorten on narrow strips (container queries). Hidden
  // when `settings.picker.hints` is off.
  import Ico from "$lib/components/Ico.svelte";
  import Kbd from "$lib/components/Kbd.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { shelf } from "./shelf.svelte";
  import { selection } from "./selection.svelte";
  import { overlays } from "./overlays.svelte";
  import { drag } from "./drag.svelte";
  import { feed } from "./feed.svelte";

  let iconFailed = $state(false);
  $effect(() => {
    void shelf.target?.iconUrl;
    iconFailed = false;
  });

  const target = $derived(shelf.target);
  const stackCount = $derived(shelf.stack.items.length);
  /** "Collect new copies" is on: the stack indicator stays visible (even at 0) and pulses a dot. */
  const collecting = $derived(shelf.capture.collectToStack);
  const selected = $derived(selection.ids.size);
</script>

<footer class="footer">
  <div class="hints">
    <span class="hint hint--main">
      <Kbd>↵</Kbd>
      {#if target && !target.elevated}
        <span class="hint__label">{t("picker.hint.pasteInto")}</span>
        <span class="target" title={target.title}>
          {#if target.iconUrl && !iconFailed}
            <img src={target.iconUrl} alt="" onerror={() => (iconFailed = true)} />
          {:else}
            <Ico name="appWindow" size={14} />
          {/if}
          <span class="target__name">{target.name}</span>
        </span>
      {:else if target?.elevated}
        <span class="hint__label">{t("picker.hint.copy")}</span>
        <span class="hint__sub" title={`${t("picker.hint.elevated")} · ${target.title}`}
          ><Ico name="shield" size={13} /><span class="hint__subtext">{t("picker.hint.elevated")}</span></span
        >
      {:else}
        <span class="hint__label">{t("picker.hint.copy")}</span>
      {/if}
    </span>
    {#if selected > 1}
      <span class="hint hint--selected">{t("picker.hint.selected", { n: selected })}</span>
    {/if}
    {#if feed.mode === "clips"}
      <span class="hint hint--plain"><Kbd>⇧↵</Kbd><span class="hint__label">{t("picker.hint.plain")}</span></span>
    {/if}
    <span class="hint hint--preview"><Kbd>Space</Kbd><span class="hint__label">{t("picker.hint.preview")}</span></span>
    <span class="hint hint--actions"><Kbd>Ctrl K</Kbd><span class="hint__label">{t("picker.hint.actions")}</span></span>
  </div>

  <div class="status">
    {#if shelf.paused}
      <button type="button" class="status__btn" title={t("picker.hint.resumeTitle")} onclick={() => void shelf.resume()}>
        <Ico name="pause" size={12} />
        <span class="status__label">{t("picker.hint.paused")}</span>
      </button>
    {/if}
    {#if stackCount > 0 || drag.active || collecting}
      <button
        type="button"
        class="status__btn"
        class:status__btn--collect={collecting && !drag.active}
        class:status__btn--drop={drag.active}
        class:status__btn--over={drag.active && drag.over === "stack"}
        data-drop={drag.active ? "stack" : undefined}
        title={t(collecting ? "picker.hint.stackCollecting" : "picker.hint.stackTitle")}
        aria-haspopup="dialog"
        onmousedown={(e) => e.preventDefault()}
        onclick={() => (overlays.isOpen("stack") ? overlays.close(overlays.layers.find((l) => l.kind === "stack")) : overlays.open("stack"))}
      >
        {#if collecting}<span class="status__dot" aria-hidden="true"></span>{/if}
        <Ico name="stack" size={12} />
        <span class="status__label">{t("picker.hint.stack")}</span>
        <span class="status__n">{stackCount}</span>
      </button>
    {/if}
  </div>
</footer>

<style>
  .footer {
    container-type: inline-size;
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    height: var(--h-md);
    margin-top: 4px;
    padding: 0 4px;
    color: var(--color-ink-2);
    font-size: var(--text-secondary);
    white-space: nowrap;
  }
  /* Hints in priority order (target first). One that doesn't fit wraps onto a second line
     that is clipped away whole — a hint shows completely or not at all, and the target app's
     name never gets squeezed to make room for a secondary hint. The 4 px block padding keeps
     the first line where a single line would sit (24 px keycaps centred in the 32 px row). */
  .hints {
    display: flex;
    flex: 1 1 auto;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 16px;
    min-width: 0;
    height: var(--h-md);
    padding-block: calc((var(--h-md) - var(--h-sm)) / 2);
    overflow: hidden;
  }
  .hint {
    display: inline-flex;
    flex: 0 0 auto;
    align-items: center;
    gap: 7px;
    min-width: 0;
  }
  .hint--main {
    flex: 0 1 auto;
    max-width: 100%;
  }
  .hint__label {
    color: var(--color-ink-2);
  }
  .hint__sub {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    min-width: 0;
    color: var(--color-ink-2);
  }
  /* The ellipsis needs the text in its own box (a flex container never shows one). */
  .hint__subtext {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .hint--selected {
    color: var(--color-ink);
    font-weight: 600;
  }
  .target {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-width: 16px;
    /* Squeezed by "N selected": the name ellipsizes, nothing spills onto the next hint. */
    overflow: hidden;
    color: var(--color-ink);
    font-weight: 600;
  }
  .target img {
    flex: 0 0 auto;
    width: 16px;
    height: 16px;
    object-fit: contain;
  }
  .target__name {
    min-width: 0;
    max-width: 200px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .status {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    gap: 6px;
  }
  .status__btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: var(--h-sm);
    padding: 0 10px 0 8px;
    border: 0;
    border-radius: 999px;
    background: var(--color-track);
    color: var(--color-ink);
    font: inherit;
    font-size: var(--text-secondary);
    font-weight: 600;
    transition:
      background-color var(--dur-fast) var(--ease-out-soft),
      color var(--dur-fast) var(--ease-out-soft),
      transform var(--dur-fast) var(--ease-out-soft);
  }
  .status__btn:hover {
    background: var(--color-hover);
  }
  .status__btn:active {
    transform: scale(0.96);
  }
  .status__n {
    font-variant-numeric: tabular-nums;
  }
  .status__dot {
    width: 6px;
    height: 6px;
    border-radius: 999px;
    background: var(--color-ink);
    animation: collect-pulse 1.6s var(--ease-in-out) infinite;
  }
  @keyframes collect-pulse {
    50% {
      opacity: 0.25;
    }
  }
  .status__btn--drop {
    transform: scale(1.04);
  }
  .status__btn--over {
    background: var(--color-ink);
    color: var(--color-paper);
  }

  /* Narrow panels / strips: drop the labels of the secondary hints, then the hints. */
  @container (max-width: 600px) {
    .hint--plain .hint__label,
    .hint--preview .hint__label,
    .hint--actions .hint__label {
      display: none;
    }
    .hints {
      column-gap: 10px;
    }
  }
  @container (max-width: 330px) {
    .hint--plain,
    .hint--preview,
    .hint--actions,
    .status__label {
      display: none;
    }
    .target__name {
      max-width: 120px;
    }
  }
</style>
