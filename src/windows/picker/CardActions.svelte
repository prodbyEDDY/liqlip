<script lang="ts">
  // Hover / selection actions over the bottom of the preview (SPEC §3.2): Copy · Favorite · ⋯.
  // Geometry is Liquid Tools' CardActions bar (5 px insets, 29 px, paper, card shadow, 8 px
  // radius) without its border; it exists in the DOM only for the hovered / current card.
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
    <Ico name="copy" size={15} />
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
      <Ico name="heart" size={15} />
    </button>
  {/if}
  <button type="button" title={t("picker.card.more")} aria-label={t("picker.card.more")} aria-haspopup="menu" onmousedown={(e) => e.preventDefault()} onclick={onmore}>
    <Ico name="more" size={15} />
  </button>
</div>

<style>
  .actions {
    position: absolute;
    left: 5px;
    right: 5px;
    bottom: 41px;
    z-index: 2;
    display: flex;
    align-items: center;
    justify-content: space-around;
    gap: 0;
    height: 29px;
    padding: 1px 2px;
    border-radius: 8px;
    background: var(--color-paper);
    box-shadow: var(--shadow-card);
    animation: actions-in var(--dur-fast) var(--ease-out-soft);
  }
  @keyframes actions-in {
    from {
      opacity: 0;
      transform: translateY(3px);
    }
  }
  .actions button {
    display: flex;
    flex: 1 0 22px;
    align-items: center;
    justify-content: center;
    min-width: 22px;
    height: 26px;
    padding: 0;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--color-ink-2);
    cursor: pointer;
    transition:
      background-color var(--dur-fast) var(--ease-out-soft),
      color var(--dur-fast) var(--ease-out-soft),
      box-shadow var(--dur-fast) var(--ease-out-soft),
      transform var(--dur-fast) var(--ease-out-soft);
  }
  .actions button:hover {
    background: var(--color-paper);
    color: var(--color-ink);
    box-shadow: var(--shadow-on-lift);
  }
  .actions button:active {
    transform: scale(0.92);
  }
  .actions button:focus-visible {
    outline: 2px solid var(--color-ink);
    outline-offset: -2px;
  }
  .actions button:disabled {
    opacity: 0.45;
  }
  .actions .active {
    background: var(--color-selected);
    color: var(--color-ink);
  }
  .actions .active :global(path) {
    fill: currentColor;
  }
</style>
