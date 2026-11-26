<script lang="ts">
  // Pinned picture root — W5-A stub; W6-E builds it here (ARCHITECTURE §18.8): `shotPinGet()` on
  // load, the picture, drag (`startDragging`), wheel → `shotPinResize`, Ctrl+wheel → opacity,
  // Esc / × → `shotPinClose`, the context menu. Transparent window (`pin.html`); browser preview
  // on the mock: /pin.html?clip=<id>.
  import { isBrowserShortcut } from "$lib/browserKeys";
  import { t } from "$lib/i18n/index.svelte";

  /** WebView2 browser keys (reload, print, …) never run in this window (FIX-15). */
  function onKeydown(e: KeyboardEvent): void {
    if (isBrowserShortcut(e, { find: true })) e.preventDefault();
  }
</script>

<svelte:window onkeydown={onKeydown} />

<main class="stub">
  <p class="soon">{t("pin.soon")}</p>
</main>

<style>
  :global(html),
  :global(body) {
    background: transparent;
    overflow: hidden;
    user-select: none;
  }
  .stub {
    position: fixed;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .soon {
    margin: 0;
    padding: 10px 18px;
    border-radius: 999px;
    background: var(--color-raised);
    color: var(--color-ink);
    box-shadow: var(--shadow-pop);
    font-size: var(--text-row);
    font-weight: 600;
  }
</style>
