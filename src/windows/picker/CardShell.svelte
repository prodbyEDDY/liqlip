<script lang="ts">
  // The card surface — port of Liquid Tools `CardShell.svelte` (square preview on the content
  // tone, 12 px radius, 36 px meta row: app icon · title · format badge). It is a listbox
  // `option`: focus stays in the search field (combobox) and the current card is announced
  // through `aria-activedescendant`. Interactive extras (hover actions, the rename field) are
  // rendered by `Card.svelte` as siblings, not inside the option.
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
  class="card"
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
  <div class="card__hit squircle">
    {@render children()}
    {#if checked}
      <span class="card__check" aria-hidden="true"><Ico name="check" size={12} /></span>
    {/if}
    {#if digit}
      <span class="card__digit" aria-hidden="true">{digit}</span>
    {/if}
  </div>
  <div class="meta" title={tooltip}>
    <span class="meta__app">
      {#if appIcon && !iconFailed}
        <img class="meta__appimg" src={appIcon} alt="" loading="lazy" decoding="async" draggable="false" onerror={() => (iconFailed = true)} />
      {:else}
        <Ico name="appWindow" size={13} />
      {/if}
    </span>
    <span class="meta__title" class:meta__title--hidden={renaming}><Hl text={title} {terms} /></span>
    {#if favorite}
      <span class="meta__fav" title={t("picker.card.favorite")}><Ico name="heart" size={11} /></span>
    {/if}
    <span class="meta__size">{badge}</span>
  </div>
  {#if dropSide}
    <span class="card__drop card__drop--{dropSide}" aria-hidden="true"></span>
  {/if}
</div>

<style>
  .card {
    position: relative;
    min-width: 0;
    height: 100%;
    border-radius: 12px;
    outline: none;
    cursor: grab;
    transition:
      background-color var(--dur-fast) var(--ease-out-soft),
      box-shadow var(--dur-fast) var(--ease-out-soft),
      opacity var(--dur-fast) var(--ease-out-soft);
  }
  .card.is-hovered {
    background: var(--color-panel-2);
  }
  /* Keyboard cursor: the product's focus ring (2 px ink) + the selected wash — the
     Liquid Tools `.kbd-wrap--selected` treatment with the focus-visible colour. */
  .card.is-current {
    background: var(--color-selected);
    box-shadow: 0 0 0 2px var(--color-ink);
  }
  .card.is-checked:not(.is-current) {
    background: var(--color-wash-sel);
  }
  .card.is-dragging {
    opacity: 0.45;
  }
  .card__hit {
    position: relative;
    display: block;
    width: 100%;
    aspect-ratio: 1;
    border-radius: 12px;
    background: var(--color-content);
    overflow: hidden;
    transition: transform var(--dur-fast) var(--ease-out-soft);
  }
  .card:active .card__hit {
    transform: scale(0.985);
  }
  .card__check {
    position: absolute;
    top: 7px;
    right: 7px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    border-radius: 999px;
    background: var(--color-ink);
    color: var(--color-paper);
    box-shadow: var(--shadow-on-lift);
    animation: pop-select 170ms var(--ease-out-soft);
  }
  .card__digit {
    position: absolute;
    top: 7px;
    left: 7px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 20px;
    height: 20px;
    padding: 0 5px;
    border-radius: 7px;
    background: var(--color-ink);
    color: var(--color-paper);
    font-family: var(--font-mono);
    font-size: var(--text-meta);
    font-weight: 600;
    line-height: 1;
    box-shadow: var(--shadow-on-lift);
  }
  .meta {
    display: flex;
    align-items: center;
    gap: 5px;
    height: 36px;
    min-width: 0;
    padding: 0 3px;
    font-size: 10px;
    color: var(--color-ink-3);
  }
  .meta__app {
    display: flex;
    align-items: center;
    justify-content: center;
    flex: 0 0 15px;
    width: 15px;
    height: 15px;
    overflow: hidden;
    border-radius: 3px;
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
    color: var(--color-ink-2);
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
  .meta__size {
    flex: 0 0 auto;
    margin-left: auto;
    padding: 3px 6px;
    border-radius: 999px;
    background: var(--color-track);
    color: var(--color-ink-2);
    font-size: 9px;
    font-weight: 650;
  }
  .card__drop {
    position: absolute;
    top: 6px;
    bottom: 42px;
    width: 3px;
    border-radius: 999px;
    background: var(--color-ink);
  }
  .card__drop--start {
    left: -7px;
  }
  .card__drop--end {
    right: -7px;
  }
</style>
