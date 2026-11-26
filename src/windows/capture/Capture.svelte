<script lang="ts">
  // Capture overlay root — W5-A stub; W6-D builds the overlay here (ARCHITECTURE §18.7): it
  // listens to `shot:session`, asks `shotSession()` on load, draws the frozen frame, selects a
  // window or a region, hosts the annotation editor and commits with `shotCommit`. Transparent
  // window (`capture.html`); browser preview on the mock: /capture.html (?mock-shot=text,
  // ?mock-shot-quick=1, ?mock-scale=1.5).
  import { isBrowserShortcut } from "$lib/browserKeys";
  import { t } from "$lib/i18n/index.svelte";

  /** WebView2 browser keys (reload, print, …) never run in this window (FIX-15). */
  function onKeydown(e: KeyboardEvent): void {
    if (isBrowserShortcut(e, { find: true })) e.preventDefault();
  }
</script>

<svelte:window onkeydown={onKeydown} />

<main class="stub">
  <p class="soon">{t("capture.soon")}</p>
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
