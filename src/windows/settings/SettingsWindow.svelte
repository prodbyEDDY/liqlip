<script lang="ts">
  // Settings window (SPEC §4): a classic Windows window — system title bar drawn by Windows,
  // sections on the left, grouped settings on the right, every change applied at once.
  // The onboarding (first launch) takes the same window, in place of the sidebar.
  //
  // Navigation: sidebar (↑/↓/Enter), Ctrl+Tab / Ctrl+Shift+Tab (also Ctrl+PgDn/PgUp),
  // `settings:navigate { section }` (from `settings_open`) and `?section=` in the URL
  // (browser checks; kept in sync with replaceState). `?onboarding=1` forces the onboarding.
  import Button from "$lib/components/Button.svelte";
  import EmptyState from "$lib/components/EmptyState.svelte";
  import { applyLanguage, t, type TranslationKey } from "$lib/i18n/index.svelte";
  import { onSettingsNavigate } from "$lib/ipc/events";
  import { applyThemePref } from "$lib/theme.svelte";
  import { resolveSection, stepIndex } from "./logic";
  import { ONBOARDING, SECTION_KEYS, SECTIONS, type SectionKey } from "./sections";
  import { settings } from "./settings.svelte";
  import { dialogs } from "./ui.svelte";
  import DialogHost from "./ui/DialogHost.svelte";
  import SidebarNav, { type NavItem } from "./ui/SidebarNav.svelte";
  import SkeletonRows from "./ui/SkeletonRows.svelte";
  import ToastHost from "./ui/ToastHost.svelte";
  import Onboarding from "./onboarding/Onboarding.svelte";
  import General from "./sections/General.svelte";
  import PickerSection from "./sections/Picker.svelte";
  import Hotkeys from "./sections/Hotkeys.svelte";
  import Storage from "./sections/Storage.svelte";
  import Privacy from "./sections/Privacy.svelte";
  import Snippets from "./sections/Snippets.svelte";
  import Ocr from "./sections/Ocr.svelte";
  import Import from "./sections/Import.svelte";
  import About from "./sections/About.svelte";

  settings.start();

  const params = typeof location !== "undefined" ? new URLSearchParams(location.search) : new URLSearchParams();
  let section = $state<SectionKey>(resolveSection(params.get("section"), SECTION_KEYS) ?? "general");
  let forceOnboarding = $state(params.get("onboarding") === "1" || params.get("section") === ONBOARDING);
  let onboardingClosed = $state(false);

  const onboarding = $derived(
    settings.status === "ready" && !onboardingClosed && (forceOnboarding || settings.value?.general.onboardingDone === false),
  );

  const items = $derived<NavItem[]>(
    SECTIONS.map((s) => ({ key: s.key, icon: s.icon, label: t(`settings.section.${s.key}` as TranslationKey) })),
  );

  // Optimistic theme/language: the window repaints on the edit, and rolls back with it.
  $effect(() => {
    const g = settings.value?.general;
    if (!g) return;
    applyThemePref(g.theme);
    applyLanguage(g.language);
  });

  $effect(() => {
    const off = onSettingsNavigate(({ section: raw }) => {
      if (raw === ONBOARDING) {
        forceOnboarding = true;
        onboardingClosed = false;
        return;
      }
      const s = resolveSection(raw, SECTION_KEYS);
      if (s) {
        section = s;
        onboardingClosed = true;
      }
    });
    return () => void off.then((f) => f());
  });

  // Keep ?section= in the URL (reload keeps the section in a browser).
  $effect(() => {
    if (typeof history === "undefined") return;
    const url = new URL(location.href);
    if (url.searchParams.get("section") === section) return;
    url.searchParams.set("section", section);
    history.replaceState(history.state, "", url);
  });

  let contentEl = $state<HTMLElement>();
  $effect(() => {
    void section;
    contentEl?.scrollTo({ top: 0 });
  });

  let compact = $state(false);
  $effect(() => {
    const mq = matchMedia("(max-width: 859px)");
    const sync = () => (compact = mq.matches);
    sync();
    mq.addEventListener("change", sync);
    return () => mq.removeEventListener("change", sync);
  });

  function onKeydown(e: KeyboardEvent) {
    if (onboarding || dialogs.current || document.querySelector("dialog[open]")) return;
    const next = (e.ctrlKey && e.code === "Tab") || (e.ctrlKey && (e.code === "PageDown" || e.code === "PageUp"));
    if (!next || e.altKey || e.metaKey) return;
    e.preventDefault();
    const back = e.code === "PageUp" || (e.code === "Tab" && e.shiftKey);
    const i = SECTION_KEYS.indexOf(section);
    section = SECTION_KEYS[stepIndex(i, back ? -1 : 1, SECTION_KEYS.length)];
  }

  function finishOnboarding() {
    onboardingClosed = true;
    forceOnboarding = false;
    section = "general";
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#if onboarding}
  <Onboarding onFinish={finishOnboarding} />
{:else}
  <div class="settings" class:settings--compact={compact}>
    <aside class="sidebar">
      <SidebarNav {items} selected={section} onSelect={(k) => (section = k as SectionKey)} ariaLabel={t("settings.nav.label")} {compact} />
    </aside>
    <main class="content" bind:this={contentEl} aria-labelledby="section-title">
      <div class="content__inner">
        <header class="head">
          <h1 class="head__title" id="section-title">{t(`settings.section.${section}` as TranslationKey)}</h1>
          <p class="head__desc">{t(`settings.section.${section}.desc` as TranslationKey)}</p>
        </header>
        {#if settings.status === "loading"}
          <SkeletonRows rows={3} />
          <SkeletonRows rows={2} />
        {:else if settings.status === "error"}
          <div class="failed squircle">
            <EmptyState icon="warning" title={t("settings.load.error")} desc={t("settings.load.errorDesc")}>
              <Button variant="primary" icon="refresh" onclick={() => settings.load()}>{t("common.retry")}</Button>
            </EmptyState>
          </div>
        {:else}
          {#key section}
            <div class="section">
              {#if section === "general"}
                <General />
              {:else if section === "picker"}
                <PickerSection />
              {:else if section === "hotkeys"}
                <Hotkeys />
              {:else if section === "storage"}
                <Storage />
              {:else if section === "privacy"}
                <Privacy />
              {:else if section === "snippets"}
                <Snippets />
              {:else if section === "ocr"}
                <Ocr />
              {:else if section === "import"}
                <Import />
              {:else}
                <About />
              {/if}
            </div>
          {/key}
        {/if}
      </div>
    </main>
  </div>
{/if}

<ToastHost />
<DialogHost />

<style>
  :global(#app) {
    height: 100%;
  }
  /* Surface aliases of this window (existing tokens only). Light follows the brief —
     content ground, panel-2 tiles. In dark, panel-2 (#1c1c1c) and track (#1a1a1a) are
     indistinguishable, so controls would vanish on tiles: the ground drops to page (#000)
     and tiles lift to content (#242424), as the dark palette intends ("surfaces lift by
     getting lighter"). */
  :global(:root) {
    --s-ground: var(--color-content);
    --s-side: var(--color-panel-2);
    --s-tile: var(--color-panel-2);
    --s-inset: var(--color-content);
    --s-dialog: var(--color-paper);
  }
  :global(:root[data-theme="dark"]) {
    --s-ground: var(--color-page);
    --s-side: var(--color-panel-2);
    --s-tile: var(--color-content);
    --s-inset: var(--color-page);
    --s-dialog: var(--color-content);
  }
  .settings {
    display: grid;
    grid-template-columns: 232px minmax(0, 1fr);
    height: 100%;
    background: var(--s-ground);
  }
  .settings--compact {
    grid-template-columns: 60px minmax(0, 1fr);
  }
  .sidebar {
    padding: 14px 10px;
    background: var(--s-side);
    overflow-y: auto;
  }
  .settings--compact .sidebar {
    padding: 14px 8px;
  }
  .content {
    overflow-y: auto;
    scrollbar-gutter: stable;
  }
  .content__inner {
    max-width: 780px;
    margin: 0 auto;
    padding: 26px 32px 56px;
  }
  .settings--compact .content__inner {
    padding: 22px 22px 48px;
  }
  .head {
    margin-bottom: 22px;
    padding: 0 4px;
  }
  .head__title {
    margin: 0;
    font-size: var(--text-section);
    font-weight: 700;
    color: var(--color-ink);
  }
  .head__desc {
    margin: 4px 0 0;
    font-size: var(--text-row);
    line-height: 1.45;
    color: var(--color-ink-3);
  }
  .section {
    animation: section-in var(--dur-base) var(--ease-out-soft);
  }
  @keyframes section-in {
    from {
      opacity: 0;
      transform: translateY(4px);
    }
  }
  .failed {
    padding: 24px 0;
    border-radius: 16px;
    background: var(--s-tile);
  }
  .failed :global(.empty) {
    max-width: 360px;
  }
</style>
