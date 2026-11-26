<script lang="ts">
  // QAO root — W5-A stub; W6-E builds the thumbnail stack here (ARCHITECTURE §18.8): cards on
  // `shot:added`, the timer, the buttons, the height via `shotsHudLayout`. The window is never
  // activated (no keys reach it in Tauri). Transparent window (`shots.html`); browser preview on
  // the mock: /shots.html — `shotCapture("screen")` adds a shot.
  import { isBrowserShortcut } from "$lib/browserKeys";
  import { t } from "$lib/i18n/index.svelte";

  /** WebView2 browser keys (reload, print, …) never run in this window (FIX-15). */
  function onKeydown(e: KeyboardEvent): void {
    if (isBrowserShortcut(e, { find: true })) e.preventDefault();
  }
</script>

<svelte:window onkeydown={onKeydown} />

<main class="stub">
  <p class="soon">{t("shots.soon")}</p>
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
    align-items: flex-end;
    justify-content: flex-start;
    padding: 24px;
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
