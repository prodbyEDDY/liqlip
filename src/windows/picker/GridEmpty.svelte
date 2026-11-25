<script lang="ts">
  // Empty / error states of the grid (SPEC §3.13): load error + "Try again"; no results +
  // "Clear search" / "Search all types"; first run (teaches the hotkey and three keys); and a
  // dedicated text for every pill. Each state has a small icon composition (the kind's icon
  // with two quieter neighbours) instead of a lone glyph floating in the middle. A button
  // that changes the feed hands the keyboard back to the search (it unmounts with the state).
  import EmptyState from "$lib/components/EmptyState.svelte";
  import Button from "$lib/components/Button.svelte";
  import Kbd from "$lib/components/Kbd.svelte";
  import { settingsOpen } from "$lib/ipc/commands";
  import { errorMessage } from "$lib/ipc/errors";
  import { t, type TranslationKey } from "$lib/i18n/index.svelte";
  import type { IconName } from "$lib/icons";
  import { feed } from "./feed.svelte";
  import { shelf } from "./shelf.svelte";
  import { folderIdOf } from "./pills";

  const PILL_EMPTY: Record<string, { icon: IconName; key: string; around: [IconName, IconName] }> = {
    text: { icon: "text", key: "text", around: ["document", "at"] },
    link: { icon: "link", key: "link", around: ["text", "document"] },
    image: { icon: "image", key: "image", around: ["file", "palette"] },
    file: { icon: "file", key: "file", around: ["folder", "archive"] },
    code: { icon: "code", key: "code", around: ["text", "document"] },
    color: { icon: "palette", key: "color", around: ["image", "code"] },
    favorites: { icon: "heart", key: "favorites", around: ["text", "image"] },
    snippets: { icon: "document", key: "snippets", around: ["text", "code"] },
  };

  const kind = $derived.by(() => {
    if (feed.status === "error") return "error";
    if (shelf.source.mode === "none" || shelf.parsed.text || shelf.chips.length || shelf.parsed.exclude.length) return "noResults";
    if (shelf.pill === "all") return "firstRun";
    return "pill";
  });
  const pillEmpty = $derived(
    folderIdOf(shelf.pill) !== null
      ? { icon: "folderOpen" as IconName, key: "folder", around: ["text", "image"] as [IconName, IconName] }
      : (PILL_EMPTY[shelf.pill] ?? PILL_EMPTY.text),
  );
  const hotkey = $derived(shelf.settings?.hotkeys.summon ?? "Alt + V");

  /** Run a button's action, then give the keyboard back to the search field. */
  function andSearch(run: () => void) {
    run();
    shelf.focusSearch(false);
  }
</script>

<div class="empty-wrap" class:empty-wrap--strip={shelf.horizontal}>
  {#if kind === "error"}
    <EmptyState icon="warning" title={t("picker.error.title")} desc={errorMessage(feed.error)} compact={shelf.horizontal}>
      <Button variant="secondary" size="sm" icon="refresh" onclick={() => andSearch(() => shelf.retry())}>{t("common.retry")}</Button>
    </EmptyState>
  {:else if kind === "noResults"}
    <EmptyState icon="search" companions={["text", "image"]} title={t("picker.noResults.title")} desc={t("picker.noResults.desc")} compact={shelf.horizontal}>
      {#if !shelf.searchEmpty}
        <Button variant="secondary" size="sm" icon="x" onclick={() => andSearch(() => shelf.clearSearch())}>{t("picker.noResults.clear")}</Button>
      {/if}
      {#if shelf.pill !== "all"}
        <Button variant="ghost" size="sm" icon="grid" onclick={() => andSearch(() => shelf.setPill("all"))}>{t("picker.noResults.allTypes")}</Button>
      {/if}
    </EmptyState>
  {:else if kind === "firstRun"}
    <EmptyState
      icon="clipboard"
      companions={["text", "image"]}
      title={t("picker.empty.title")}
      desc={hotkey ? t("picker.empty.desc", { hotkey }) : t("picker.empty.descNoHotkey")}
      compact={shelf.horizontal}
    >
      {#if !shelf.horizontal}
        <ul class="hints">
          <li><Kbd>↵</Kbd><span>{t("picker.empty.hint.paste")}</span></li>
          <li><Kbd>Aa</Kbd><span>{t("picker.empty.hint.search")}</span></li>
          <li><Kbd>Ctrl D</Kbd><span>{t("picker.empty.hint.favorite")}</span></li>
        </ul>
      {/if}
    </EmptyState>
  {:else}
    <EmptyState
      icon={pillEmpty.icon}
      companions={pillEmpty.around}
      title={t(`picker.empty.${pillEmpty.key}.title` as TranslationKey)}
      desc={t(`picker.empty.${pillEmpty.key}.desc` as TranslationKey)}
      compact={shelf.horizontal}
    >
      {#if shelf.pill === "snippets"}
        <Button variant="secondary" size="sm" icon="settings" onclick={() => void settingsOpen("snippets").catch(() => {})}>
          {t("picker.empty.snippets.action")}
        </Button>
      {/if}
    </EmptyState>
  {/if}
</div>

<style>
  .empty-wrap {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    min-height: 0;
    padding-right: var(--pad-panel);
  }
  .empty-wrap--strip {
    padding: 0 var(--pad-panel);
  }
  .hints {
    display: grid;
    grid-template-columns: auto auto;
    gap: 8px 12px;
    align-items: center;
    margin: 6px 0 0;
    padding: 0;
    list-style: none;
  }
  .hints li {
    display: contents;
    color: var(--color-ink-2);
    font-size: var(--text-secondary);
  }
  .hints li > :global(.kbd) {
    justify-self: end;
  }
  .hints li > span {
    text-align: left;
  }
</style>
