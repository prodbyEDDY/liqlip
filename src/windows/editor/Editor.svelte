<script lang="ts">
  // Editor window root — W5-A stub; W6-C builds the window here (ARCHITECTURE §18.9) around the
  // annotation editor (`src/lib/editor/`): `shotEditorGet()` on load, `shot:editor` to switch,
  // Copy / Pin / Save as… / Done through `shotEditorCommit`. An ordinary window with the system
  // title bar; browser preview on the mock: /editor.html?clip=<id>.
  import { isBrowserShortcut } from "$lib/browserKeys";
  import { t } from "$lib/i18n/index.svelte";

  /** WebView2 browser keys (reload, print, …) never run in this window (FIX-15). */
  function onKeydown(e: KeyboardEvent): void {
    if (isBrowserShortcut(e, { find: true })) e.preventDefault();
  }
</script>

<svelte:window onkeydown={onKeydown} />

<main class="stub">
  <p class="soon">{t("editor.soon")}</p>
</main>

<style>
  :global(html),
  :global(body) {
    background: var(--color-ground);
    overflow: hidden;
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
    color: var(--color-ink-2);
    font-size: var(--text-row);
  }
</style>
