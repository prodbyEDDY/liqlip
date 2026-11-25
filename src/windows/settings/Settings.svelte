<script lang="ts">
  // W0 stub of the settings window (W1-F replaces it): the section sidebar and a content
  // header that follows `settings:navigate` and `?section=`. Not the final design.
  import Menu, { type MenuItemDef } from "$lib/components/Menu.svelte";
  import { t, type TranslationKey } from "$lib/i18n/index.svelte";
  import { onSettingsNavigate } from "$lib/ipc/events";
  import type { IconName } from "$lib/icons";

  const SECTIONS: [string, IconName][] = [
    ["general", "sliders"],
    ["picker", "grid"],
    ["hotkeys", "keyboard"],
    ["storage", "database"],
    ["privacy", "shield"],
    ["snippets", "document"],
    ["ocr", "ocr"],
    ["import", "download"],
    ["about", "info"],
  ];

  const keys = SECTIONS.map(([k]) => k);
  const fromUrl = typeof location !== "undefined" ? new URLSearchParams(location.search).get("section") : null;
  let section = $state(fromUrl && keys.includes(fromUrl) ? fromUrl : "general");

  const items = $derived<MenuItemDef[]>(
    SECTIONS.map(([key, icon]) => ({ key, icon, label: t(`settings.section.${key}` as TranslationKey) })),
  );

  $effect(() => {
    const off = onSettingsNavigate(({ section: s }) => {
      if (keys.includes(s)) section = s;
    });
    return () => void off.then((f) => f());
  });
</script>

<div class="settings">
  <aside class="sidebar">
    <Menu {items} selectedItem={section} onSelectItem={(k) => (section = k)} ariaLabel={t("settings.nav.label")} />
  </aside>
  <main class="content">
    <h1 class="title">{t(`settings.section.${section}` as TranslationKey)}</h1>
  </main>
</div>

<style>
  :global(#app) {
    height: 100%;
  }
  .settings {
    display: grid;
    grid-template-columns: 240px 1fr;
    height: 100%;
    background: var(--color-page);
  }
  .sidebar {
    padding: 16px 12px;
    background: var(--color-panel-2);
    overflow-y: auto;
  }
  .content {
    padding: 28px 32px;
    background: var(--color-content);
    overflow-y: auto;
  }
  .title {
    margin: 0;
    font-size: var(--text-section);
    font-weight: 700;
    color: var(--color-ink);
  }
</style>
