<script lang="ts">
  // The card surface (W2-E): ONE squircle tile (r16 = panel radius − panel padding) holding
  // the preview on top and the 32 px meta row (app icon · title · ♥ · format badge) at the
  // bottom — no box-in-a-box, no ring. States are tone + lift: hover → card-hover; current
  // (keyboard cursor, what Enter acts on) → lifted card with ink title and an inverted badge;
  // multi-selected → wash + ✓. Corner marks (digit, ✓) are 24 px circles inset 4 px and the
  // badge sits 7 px from the bottom-right corner — all concentric with the card.
  // It is a listbox `option`: focus stays in the search field (combobox) and the current card
  // is announced through `aria-activedescendant`. Interactive extras (hover tools, the rename
  // field) are rendered by `Card.svelte` as siblings, not inside the option.
  import type { Snippet } from "svelte";
  import Ico from "$lib/components/Ico.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import Hl from "./Hl.svelte";

  interface Props {
    id: number;
    title: string;
    terms: readonly string[];
    /** Accessible name of the card (kind + title + preview start). */
    label: string;
    tooltip: string;
    appIcon: string | null;
    badge: string;
    favorite?: boolean;
    /** Keyboard cursor (what Enter acts on). */
    current?: boolean;
    /** Member of the multi-selection (✓). */
    checked?: boolean;
    hovered?: boolean;
    dragging?: boolean;
    /** Digit 1…9 while Ctrl is held. */
    digit?: number | null;
    /** Insertion marker while reordering Favorites. */
    dropSide?: "start" | "end" | null;
    /** Hide the title (the rename field covers the meta row). */
    renaming?: boolean;
    children: Snippet;
    onpointerdown?: (e: PointerEvent) => void;
    onclick?: (e: MouseEvent) => void;
    oncontextmenu?: (e: MouseEvent) => void;
    onpointerenter?: () => void;
    onpointerleave?: () => void;
  }
  let {
    id,
    title,
    terms,
    label,
    tooltip,
    appIcon,
    badge,
    favorite = false,
    current = false,
    checked = false,
    hovered = false,
    dragging = false,
    digit = null,
    dropSide = null,
    renaming = false,
    children,
    onpointerdown,
    onclick,
    oncontextmenu,
    onpointerenter,
    onpointerleave,
  }: Props = $props();

  let iconFailed = $state(false);
  $effect(() => {
    void appIcon;
    iconFailed = false;
  });
</script>

<!-- Keyboard users act through the search field (combobox) and the window key map; the
     option itself is never focused (aria-activedescendant). -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  class="card squircle"
  class:is-current={current}
  class:is-checked={checked}
  class:is-hovered={hovered}
  class:is-dragging={dragging}
  id={`card-${id}`}
  role="option"
  tabindex="-1"
  aria-selected={current || checked}
  aria-label={label}
  data-card={id}
  {onpointerdown}
  {onclick}
  {oncontextmenu}
  {onpointerenter}
  {onpointerleave}
  onmousedown={(e) => e.preventDefault()}
>
  <div class="card__preview">
    {@render children()}
  </div>
  <div class="meta" title={tooltip}>
    <span class="meta__app">
      {#if appIcon && !iconFailed}
        <img class="meta__appimg" src={appIcon} alt="" loading="lazy" decoding="async" draggable="false" onerror={() => (iconFailed = true)} />
      {:else}
        <Ico name="appWindow" size={14} />
      {/if}
    </span>
    <span class="meta__title" class:meta__title--hidden={renaming}><Hl text={title} {terms} /></span>
    {#if favorite}
      <span class="meta__fav" title={t("picker.card.favorite")}><Ico name="heart" size={12} /></span>
    {/if}
    <span class="meta__badge">{badge}</span>
  </div>
  {#if digit}
    <span class="card__mark card__mark--digit" aria-hidden="true">{digit}</span>
  {:else if checked}
    <span class="card__mark" aria-hidden="true"><Ico name="check" size={13} /></span>
  {/if}
  {#if dropSide}
    <span class="card__drop card__drop--{dropSide}" aria-hidden="true"></span>
  {/if}
</div>

<style>
  .card {
    position: relative;
    display: flex;
    flex-direction: column;
    min-width: 0;
    height: 100%;
    overflow: clip;
    border-radius: var(--r-card);
    background: var(--color-card);
    outline: none;
    cursor: grab;
    transition:
      background-color var(--dur-fast) var(--ease-out-soft),
      box-shadow var(--dur-fast) var(--ease-out-soft),
      opacity var(--dur-fast) var(--ease-out-soft);
  }
  .card.is-hovered {
    background: var(--color-card-hover);
  }
  .card.is-checked {
    background: color-mix(in srgb, var(--color-ink) 9%, var(--color-card));
  }
  /* Keyboard cursor = the selection state of the grid: lifted (shadow; the tile moves up
     2 px in Grid.svelte), brighter face, ink title, inverted badge. No ring. */
  .card.is-current {
    background: var(--color-card-lift);
    box-shadow: var(--shadow-lift);
  }
  .card.is-current.is-checked {
    background: color-mix(in srgb, var(--color-ink) 6%, var(--color-card-lift));
  }
  .card.is-dragging {
    opacity: 0.45;
    box-shadow: none;
  }
  .card:active {
    cursor: grabbing;
  }

  .card__preview {
    position: relative;
    flex: 1 1 auto;
    min-height: 0;
    overflow: hidden;
  }

  /* Corner mark (Ctrl digit or ✓): a 24 px ink circle inset 4 px → concentric (12 + 4 = 16). */
  .card__mark {
    position: absolute;
    top: var(--pad-card);
    left: var(--pad-card);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: calc(2 * var(--r-card-in));
    height: calc(2 * var(--r-card-in));
    border-radius: 999px;
    background: var(--color-ink);
    color: var(--color-paper);
    box-shadow: var(--shadow-on-lift);
    animation: pop-select 170ms var(--ease-out-soft);
  }
  .card__mark--digit {
    font-family: var(--font-mono);
    font-size: var(--text-secondary);
    font-weight: 600;
    line-height: 1;
  }

  .meta {
    display: flex;
    flex: 0 0 32px; /* = layout.ts CARD_FOOTER */
    align-items: center;
    gap: 6px;
    min-width: 0;
    padding: 0 7px 0 9px;
    color: var(--color-ink-2);
    font-size: var(--text-secondary);
  }
  .meta__app {
    display: flex;
    flex: 0 0 16px;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    color: var(--color-ink-3);
  }
  .meta__appimg {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }
  .meta__title {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .is-current .meta__title {
    color: var(--color-ink);
    font-weight: 500;
  }
  .meta__title--hidden {
    visibility: hidden;
  }
  .meta__fav {
    display: inline-flex;
    flex: 0 0 auto;
    color: var(--color-ink);
  }
  .meta__fav :global(path) {
    fill: currentColor;
  }
  /* Format badge: 18 px capsule, 7 px from the card's bottom-right corner (9 + 7 = 16). */
  .meta__badge {
    display: inline-flex;
    flex: 0 0 auto;
    align-items: center;
    height: 18px;
    margin-left: auto;
    padding: 0 6px;
    border-radius: 999px;
    background: var(--color-track);
    color: var(--color-ink-2);
    font-size: var(--text-meta);
    font-weight: 650;
    letter-spacing: 0.02em;
    line-height: 1;
  }
  .is-current .meta__badge {
    background: var(--color-ink);
    color: var(--color-paper);
  }
  .card__drop {
    position: absolute;
    top: 8px;
    bottom: 8px;
    width: 3px;
    border-radius: 999px;
    background: var(--color-ink);
  }
  .card__drop--start {
    left: 2px;
  }
  .card__drop--end {
    right: 2px;
  }
</style>
