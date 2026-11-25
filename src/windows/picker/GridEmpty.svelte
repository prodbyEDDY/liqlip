<script lang="ts">
  // Empty / error states of the grid (SPEC §3.13): load error + "Try again"; no results +
  // "Clear search" / "Search all types"; first run (teaches the hotkey and three keys); and a
  // dedicated text for every pill.
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

  const PILL_EMPTY: Record<string, { icon: IconName; key: string }> = {
    text: { icon: "text", key: "text" },
    link: { icon: "link", key: "link" },
    image: { icon: "image", key: "image" },
    file: { icon: "file", key: "file" },
    code: { icon: "code", key: "code" },
    color: { icon: "palette", key: "color" },
    favorites: { icon: "heart", key: "favorites" },
    snippets: { icon: "document", key: "snippets" },
  };

  const kind = $derived.by(() => {
    if (feed.status === "error") return "error";
    if (shelf.source.mode === "none" || shelf.parsed.text || shelf.chips.length || shelf.parsed.exclude.length) return "noResults";
    if (shelf.pill === "all") return "firstRun";
    return "pill";
  });
  const pillEmpty = $derived(
    folderIdOf(shelf.pill) !== null ? { icon: "folderOpen" as IconName, key: "folder" } : (PILL_EMPTY[shelf.pill] ?? PILL_EMPTY.text),
  );
  const hotkey = $derived(shelf.settings?.hotkeys.summon ?? "Alt + V");
</script>

<div class="empty-wrap">
  {#if kind === "error"}
    <EmptyState icon="warning" title={t("picker.error.title")} desc={errorMessage(feed.error)}>
      <Button variant="secondary" size="sm" icon="refresh" onclick={() => shelf.retry()}>{t("common.retry")}</Button>
    </EmptyState>
  {:else if kind === "noResults"}
    <EmptyState icon="search" title={t("picker.noResults.title")} desc={t("picker.noResults.desc")}>
      <div class="row">
        {#if !shelf.searchEmpty}
          <Button variant="secondary" size="sm" icon="x" onclick={() => shelf.clearSearch()}>{t("picker.noResults.clear")}</Button>
        {/if}
        {#if shelf.pill !== "all"}
          <Button variant="ghost" size="sm" icon="grid" onclick={() => shelf.setPill("all")}>{t("picker.noResults.allTypes")}</Button>
        {/if}
      </div>
    </EmptyState>
  {:else if kind === "firstRun"}
    <div class="first">
      <EmptyState
        icon="clipboard"
        title={t("picker.empty.title")}
        desc={hotkey ? t("picker.empty.desc", { hotkey }) : t("picker.empty.descNoHotkey")}
      />
      <ul class="hints">
        <li><Kbd>↵</Kbd><span>{t("picker.empty.hint.paste")}</span></li>
        <li><Kbd>Aa</Kbd><span>{t("picker.empty.hint.search")}</span></li>
        <li><Kbd>Ctrl D</Kbd><span>{t("picker.empty.hint.favorite")}</span></li>
      </ul>
    </div>
  {:else}
    <EmptyState
      icon={pillEmpty.icon}
      title={t(`picker.empty.${pillEmpty.key}.title` as TranslationKey)}
      desc={t(`picker.empty.${pillEmpty.key}.desc` as TranslationKey)}
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
    min-height: 200px;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 6px;
  }
  .first {
    display: flex;
    flex-direction: column;
    align-items: center;
  }
  .hints {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .hints li {
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--color-ink-2);
    font-size: var(--text-secondary);
  }
  .hints :global(.kbd) {
    min-width: 52px;
  }
</style>
