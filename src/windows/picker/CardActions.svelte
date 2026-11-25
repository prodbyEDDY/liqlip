<script lang="ts">
  // Hover / selection tools (SPEC §3.2): Copy · Favorite · ⋯ — an ink capsule (Liquid Tools'
  // "black capsule with round actions") of 24 px circles in the card's top-right corner,
  // inset 4 px: concentric with the card (12 + 4 = 16). It exists in the DOM only for the
  // hovered / current card.
  import Ico from "$lib/components/Ico.svelte";
  import { t } from "$lib/i18n/index.svelte";

  interface Props {
    favorite: boolean;
    /** Snippets have no favorite flag. */
    canFavorite?: boolean;
    oncopy: () => void;
    onfavorite?: () => void;
    onmore: (e: MouseEvent) => void;
  }
  let { favorite, canFavorite = true, oncopy, onfavorite, onmore }: Props = $props();

  const favLabel = $derived(t(favorite ? "picker.card.unfavorite" : "picker.card.favoriteAdd"));
</script>

<div class="actions" role="toolbar" aria-label={t("picker.card.actions")}>
  <button type="button" title={t("picker.card.copy")} aria-label={t("picker.card.copy")} onmousedown={(e) => e.preventDefault()} onclick={oncopy}>
    <Ico name="copy" size={14} />
  </button>
  {#if canFavorite && onfavorite}
    <button
      type="button"
      class:active={favorite}
      title={favLabel}
      aria-label={favLabel}
      aria-pressed={favorite}
      onmousedown={(e) => e.preventDefault()}
      onclick={onfavorite}
    >
      <Ico name="heart" size={14} />
    </button>
  {/if}
  <button type="button" title={t("picker.card.more")} aria-label={t("picker.card.more")} aria-haspopup="menu" onmousedown={(e) => e.preventDefault()} onclick={onmore}>
    <Ico name="more" size={14} />
  </button>
</div>

<style>
  .actions {
    position: absolute;
    top: var(--pad-card);
    right: var(--pad-card);
    z-index: 2;
    display: flex;
    align-items: center;
    height: calc(2 * var(--r-card-in));
    border-radius: 999px;
    background: var(--color-ink);
    box-shadow: var(--shadow-on-lift);
    animation: actions-in var(--dur-fast) var(--ease-out-soft);
  }
  @keyframes actions-in {
    from {
      opacity: 0;
      transform: scale(0.92);
    }
  }
  .actions button {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    justify-content: center;
    width: calc(2 * var(--r-card-in));
    height: calc(2 * var(--r-card-in));
    padding: 0;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: color-mix(in srgb, var(--color-paper) 78%, transparent);
    transition:
      background-color var(--dur-fast) var(--ease-out-soft),
      color var(--dur-fast) var(--ease-out-soft),
      transform var(--dur-fast) var(--ease-out-soft);
  }
  .actions button:hover {
    background: color-mix(in srgb, var(--color-paper) 18%, transparent);
    color: var(--color-paper);
  }
  .actions button:active {
    transform: scale(0.9);
  }
  .actions button:focus-visible {
    outline-color: var(--color-paper);
    outline-offset: -2px;
  }
  .actions .active {
    color: var(--color-paper);
  }
  .actions .active :global(path) {
    fill: currentColor;
  }
</style>
