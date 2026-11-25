<script lang="ts">
  // A list of executables with icons (ignored apps, paste keys): remove per row, add from
  // the running apps or through the native "choose an .exe" dialog. No free text entry —
  // names always come from a real process or file. Stores file names ("keepass.exe").
  import type { Snippet } from "svelte";
  import Button from "$lib/components/Button.svelte";
  import Ico from "$lib/components/Ico.svelte";
  import { pickExe } from "$lib/ipc/commands";
  import { errorMessage } from "$lib/ipc/errors";
  import type { RunningApp } from "$lib/ipc/types";
  import { t } from "$lib/i18n/index.svelte";
  import { appDirectory } from "../apps.svelte";
  import { exeFileName, sameExe } from "../logic";
  import { toasts } from "../ui.svelte";
  import AppIcon from "./AppIcon.svelte";
  import RunningAppsDialog from "./RunningAppsDialog.svelte";

  interface Props {
    items: string[];
    label: string;
    empty: string;
    onAdd: (exe: string) => void;
    onRemove: (exe: string) => void;
    /** Extra control per row (e.g. the paste chord). */
    trailing?: Snippet<[string]>;
    disabled?: boolean;
  }
  let { items, label, empty, onAdd, onRemove, trailing, disabled = false }: Props = $props();

  let dialogOpen = $state(false);
  let browsing = $state(false);

  $effect(() => appDirectory.ensure());

  const has = (exe: string) => items.some((x) => sameExe(x, exe));

  /** Add once; a duplicate is reported instead. Returns true when added. */
  function add(exe: string, name?: string): boolean {
    const file = exeFileName(exe);
    if (!file) return false;
    if (has(file)) {
      toasts.show({ message: t("settings.apps.exists", { app: name ?? appDirectory.look(file).name }), icon: "info" });
      return false;
    }
    onAdd(file);
    return true;
  }

  function pickRunning(app: RunningApp) {
    add(app.exe, app.name);
  }

  async function browse() {
    browsing = true;
    try {
      const path = await pickExe();
      if (path) appDirectory.merge([{ exe: path }]);
      if (path && add(path)) {
        toasts.show({ message: t("settings.apps.added", { app: appDirectory.look(path).name }), icon: "check" });
      }
    } catch (e) {
      toasts.show({ message: errorMessage(e), icon: "warning" });
    } finally {
      browsing = false;
    }
  }
</script>

<div class="exe">
  {#if items.length === 0}
    <p class="exe__empty">{empty}</p>
  {:else}
    <ul class="exe__list" aria-label={label}>
      <!-- Keyed by position too: a hand-edited settings.json may repeat a name. -->
      {#each items as exe, i (i + ":" + exe)}
        {@const look = appDirectory.look(exe)}
        <li class="exe__item squircle" title={look.path ?? exe}>
          <AppIcon name={look.name} url={look.iconUrl} />
          <span class="exe__text">
            <span class="exe__name">{look.name}</span>
            <span class="exe__file">{exe}</span>
          </span>
          {#if trailing}<span class="exe__trailing">{@render trailing(exe)}</span>{/if}
          <button
            type="button"
            class="exe__remove"
            aria-label={t("settings.apps.remove", { app: look.name })}
            title={t("settings.apps.remove", { app: look.name })}
            {disabled}
            onclick={() => onRemove(exe)}
          >
            <Ico name="x" size={14} />
          </button>
        </li>
      {/each}
    </ul>
  {/if}
  <div class="exe__actions">
    <Button variant="secondary" size="sm" icon="plus" {disabled} onclick={() => (dialogOpen = true)}>
      {t("settings.apps.addRunning")}
    </Button>
    <Button variant="ghost" size="sm" icon="folderOpen" {disabled} loading={browsing} onclick={browse}>
      {t("settings.apps.browse")}
    </Button>
  </div>
</div>

<RunningAppsDialog bind:open={dialogOpen} isAdded={has} onPick={pickRunning} />

<style>
  .exe {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .exe__empty {
    margin: 0;
    padding: 12px 14px;
    border-radius: var(--radius-md);
    corner-shape: squircle;
    background: var(--color-track);
    color: var(--color-ink-2);
    font-size: var(--text-secondary);
    line-height: var(--text-secondary--line-height);
  }
  .exe__list {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  /* List entry: r12 squircle on the control wash; its remove circle (32 px) is inset 8 px
     from the right edge, mid-height. */
  .exe__item {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 48px;
    padding: 8px 8px 8px 10px;
    border-radius: var(--radius-md);
    background: var(--color-track);
  }
  .exe__text {
    display: flex;
    flex: 1 1 auto;
    flex-direction: column;
    min-width: 0;
  }
  .exe__name,
  .exe__file {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .exe__name {
    color: var(--color-ink);
    font-size: var(--text-row);
    font-weight: 600;
  }
  .exe__file {
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
  }
  .exe__trailing {
    flex: 0 0 auto;
    width: 180px;
  }
  .exe__remove {
    display: inline-flex;
    flex: 0 0 auto;
    align-items: center;
    justify-content: center;
    width: var(--h-md);
    height: var(--h-md);
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: var(--color-ink-3);
    transition:
      background-color var(--dur-fast) var(--ease-out-soft),
      color var(--dur-fast) var(--ease-out-soft);
  }
  .exe__remove:hover:not(:disabled) {
    background: var(--color-hover);
    color: var(--color-ink);
  }
  .exe__remove:active:not(:disabled) {
    background: var(--color-selected);
  }
  .exe__remove:disabled {
    opacity: 0.45;
  }
  .exe__actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  @media (max-width: 859px) {
    .exe__trailing {
      width: 150px;
    }
  }
</style>
