<script lang="ts">
  // Folders (SPEC §3.9: "create … in settings"): list with counts, create, rename (inline,
  // F2), reorder (drag or Alt+↑/↓ → `folder_move`), delete with a choice (keep items /
  // delete with items) and an Undo toast (`clips_undo`). Counts refresh on clips:changed.
  import Badge from "$lib/components/Badge.svelte";
  import Button from "$lib/components/Button.svelte";
  import Ico from "$lib/components/Ico.svelte";
  import { clipsUndo, folderCreate, folderDelete, folderMove, folderRename, foldersList } from "$lib/ipc/commands";
  import { errorCode, errorMessage } from "$lib/ipc/errors";
  import { onClipsChanged } from "$lib/ipc/events";
  import type { Folder } from "$lib/ipc/types";
  import { getLocale, t } from "$lib/i18n/index.svelte";
  import { FOLDER_NAME_MAX, formatCount, validateFolderName, type NameIssue } from "../logic";
  import { createReorder } from "../reorder.svelte";
  import { dialogs, toasts } from "../ui.svelte";
  import Group from "../ui/Group.svelte";
  import SkeletonRows from "../ui/SkeletonRows.svelte";
  import TextInput from "../ui/TextInput.svelte";

  let folders = $state<Folder[]>([]);
  let status = $state<"loading" | "ready" | "error">("loading");

  async function load(quiet = false) {
    if (!quiet) status = "loading";
    try {
      folders = await foldersList();
      status = "ready";
    } catch {
      if (!quiet) status = "error";
    }
  }

  $effect(() => {
    void load();
    let timer: ReturnType<typeof setTimeout> | undefined;
    const off = onClipsChanged(() => {
      clearTimeout(timer);
      timer = setTimeout(() => void load(true), 400);
    });
    return () => {
      clearTimeout(timer);
      void off.then((f) => f());
    };
  });

  const nameMessage = (issue: NameIssue) =>
    issue === "tooLong" ? t("settings.name.tooLong", { max: FOLDER_NAME_MAX }) : t(`settings.name.${issue}`);

  // ── create ──
  let creating = $state(false);
  let draft = $state("");
  let createError = $state<string | null>(null);
  let busy = $state(false);
  const draftIssue = $derived(draft.trim() ? validateFolderName(draft, folders) : null);

  async function create() {
    const issue = validateFolderName(draft, folders);
    if (issue) {
      createError = nameMessage(issue);
      return;
    }
    busy = true;
    try {
      const f = await folderCreate(draft.trim());
      folders = [...folders, f];
      creating = false;
      draft = "";
      createError = null;
    } catch (e) {
      createError = errorCode(e) === "invalid" && String(e).includes("exists") ? t("settings.name.exists") : errorMessage(e);
    } finally {
      busy = false;
    }
  }

  // ── rename ──
  let editing = $state<number | null>(null);
  let editDraft = $state("");
  let editError = $state<string | null>(null);

  function startRename(f: Folder) {
    editing = f.id;
    editDraft = f.name;
    editError = null;
  }

  async function rename(f: Folder) {
    if (editDraft.trim() === f.name) {
      editing = null;
      return;
    }
    const issue = validateFolderName(editDraft, folders, f.id);
    if (issue) {
      editError = nameMessage(issue);
      return;
    }
    try {
      const updated = await folderRename(f.id, editDraft.trim());
      folders = folders.map((x) => (x.id === f.id ? updated : x));
      editing = null;
    } catch (e) {
      editError = errorCode(e) === "invalid" && String(e).includes("exists") ? t("settings.name.exists") : errorMessage(e);
    }
  }

  // ── delete ──
  async function remove(f: Folder) {
    const choice = await dialogs.ask({
      title: t("settings.picker.folders.deleteTitle", { name: f.name }),
      body: t("settings.picker.folders.deleteBody"),
      actions: [
        { value: "with", label: t("settings.picker.folders.withItems"), variant: "danger" },
        { value: "keep", label: t("settings.picker.folders.keepItems"), variant: "primary" },
      ],
    });
    if (!choice) return;
    try {
      const { undoToken } = await folderDelete(f.id, choice === "with");
      folders = folders.filter((x) => x.id !== f.id);
      toasts.show({
        message: t("settings.picker.folders.deleted", { name: f.name }),
        icon: "trash",
        action: {
          label: t("common.undo"),
          run: async () => {
            const ok = await clipsUndo(undoToken).catch(() => false);
            toasts.show({ message: t(ok ? "settings.toast.restored" : "settings.toast.undoExpired"), icon: ok ? "check" : "info" });
            void load(true);
          },
        },
      });
    } catch (e) {
      toasts.show({ message: errorMessage(e), icon: "warning" });
    }
  }

  const reorder = createReorder<Folder>({
    get: () => folders,
    set: (items) => (folders = items),
    commit: (id, beforeId) => folderMove(id, beforeId),
    enabled: () => editing === null,
    onError: (e) => toasts.show({ message: errorMessage(e), icon: "warning" }),
  });

  function onRowKey(f: Folder, e: KeyboardEvent) {
    if (reorder.key(f.id, e)) return;
    if (e.key === "F2") {
      e.preventDefault();
      startRename(f);
    } else if (e.key === "Delete") {
      e.preventDefault();
      void remove(f);
    }
  }

  const createId = $props.id();
</script>

<Group title={t("settings.picker.folders")} desc={t("settings.picker.folders.desc")}>
  {#snippet aside()}
    {#if status === "ready" && !creating}
      <Button variant="secondary" size="sm" icon="folderPlus" onclick={() => (creating = true)}>{t("settings.picker.folders.new")}</Button>
    {/if}
  {/snippet}
  {#if status === "loading"}
    <SkeletonRows rows={2} title={false} />
  {:else if status === "error"}
    <div class="tile tile--msg">
      <Ico name="warning" size={16} />
      <span>{t("settings.picker.folders.error")}</span>
      <Button variant="secondary" size="sm" icon="refresh" onclick={() => load()}>{t("common.retry")}</Button>
    </div>
  {:else}
    {#if folders.length === 0 && !creating}
      <div class="tile tile--msg">
        <Ico name="folder" size={16} />
        <span>{t("settings.picker.folders.empty")}</span>
      </div>
    {/if}
    {#if folders.length > 0}
      <ul class="list" aria-label={t("settings.picker.folders")}>
        {#each folders as f (f.id)}
          <li
            class="tile folder"
            class:folder--drag={reorder.dragId === f.id}
            draggable={editing !== f.id}
            ondragstart={(e) => reorder.start(f.id, e)}
            ondragover={(e) => reorder.over(f.id, e)}
            ondrop={(e) => reorder.drop(e)}
            ondragend={() => reorder.end()}
          >
            <span class="folder__grip" aria-hidden="true"><Ico name="arrowsUpDown" size={14} /></span>
            <span class="folder__icon"><Ico name="folder" size={17} /></span>
            {#if editing === f.id}
              <div class="folder__edit">
                <TextInput
                  bind:value={editDraft}
                  focus
                  ariaLabel={t("settings.picker.folders.name")}
                  invalid={!!editError}
                  describedby={editError ? `${createId}-ee` : undefined}
                  oninput={() => (editError = null)}
                  onkeydown={(e) => {
                    if (e.key === "Enter") void rename(f);
                    if (e.key === "Escape") {
                      e.stopPropagation();
                      editing = null;
                    }
                  }}
                  onblur={() => void rename(f)}
                />
                {#if editError}<span class="msg" id="{createId}-ee" role="alert">{editError}</span>{/if}
              </div>
            {:else}
              <button
                type="button"
                class="folder__name"
                title={t("settings.picker.folders.rename")}
                aria-describedby="{createId}-hint"
                onclick={() => startRename(f)}
                onkeydown={(e) => onRowKey(f, e)}>{f.name}</button
              >
              <span title={t("settings.picker.folders.count", { n: formatCount(f.count, getLocale()) })}>
                <Badge mono>{formatCount(f.count, getLocale())}</Badge>
              </span>
              <span class="folder__tools">
                <button type="button" class="tool" aria-label={t("settings.list.moveUp")} title={t("settings.list.moveUp")} onclick={() => reorder.step(f.id, -1)}>
                  <Ico name="chevronUp" size={15} />
                </button>
                <button type="button" class="tool" aria-label={t("settings.list.moveDown")} title={t("settings.list.moveDown")} onclick={() => reorder.step(f.id, 1)}>
                  <Ico name="chevronDown" size={15} />
                </button>
                <button type="button" class="tool" aria-label={t("settings.picker.folders.rename")} title={t("settings.picker.folders.rename")} onclick={() => startRename(f)}>
                  <Ico name="pen" size={15} />
                </button>
                <button type="button" class="tool" aria-label={t("settings.picker.folders.delete")} title={t("settings.picker.folders.delete")} onclick={() => remove(f)}>
                  <Ico name="trash" size={15} />
                </button>
              </span>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
    {#if creating}
      <div class="tile create">
        <span class="folder__icon"><Ico name="folderPlus" size={17} /></span>
        <div class="folder__edit">
          <TextInput
            bind:value={draft}
            focus
            placeholder={t("settings.picker.folders.name")}
            ariaLabel={t("settings.picker.folders.name")}
            invalid={!!(createError || draftIssue)}
            describedby={createError || draftIssue ? `${createId}-ce` : undefined}
            oninput={() => (createError = null)}
            onkeydown={(e) => {
              if (e.key === "Enter") void create();
              if (e.key === "Escape") {
                e.stopPropagation();
                creating = false;
                draft = "";
              }
            }}
          />
          {#if createError || draftIssue}
            <span class="msg" id="{createId}-ce" role="alert">{createError ?? nameMessage(draftIssue!)}</span>
          {/if}
        </div>
        <Button variant="ghost" size="sm" onclick={() => ((creating = false), (draft = ""), (createError = null))}>{t("common.cancel")}</Button>
        <Button variant="primary" size="sm" loading={busy} disabled={!draft.trim() || !!draftIssue} onclick={create}>{t("common.add")}</Button>
      </div>
    {/if}
    <p class="hint" id="{createId}-hint">{t("settings.list.reorderHint")}</p>
  {/if}
</Group>

<style>
  .list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .tile {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 52px;
    padding: 8px 10px 8px 12px;
    background: var(--s-tile);
  }
  .tile--msg {
    gap: 10px;
    padding: 14px 16px;
    font-size: var(--text-secondary);
    line-height: 1.45;
    color: var(--color-ink-2);
  }
  .tile--msg span {
    flex: 1 1 auto;
  }
  .folder {
    transition:
      background-color var(--dur-fast) var(--ease-out-soft),
      opacity var(--dur-fast) var(--ease-out-soft);
  }
  .folder:hover {
    background: var(--color-hover);
  }
  .folder--drag {
    opacity: 0.5;
  }
  .folder__grip {
    display: inline-flex;
    color: var(--color-ink-3);
    opacity: 0;
    transition: opacity var(--dur-fast) var(--ease-out-soft);
  }
  .folder:hover .folder__grip,
  .folder:focus-within .folder__grip {
    opacity: 1;
  }
  .folder__icon {
    display: inline-flex;
    color: var(--color-ink-2);
  }
  .folder__name {
    flex: 1 1 auto;
    min-width: 0;
    padding: 4px 0;
    border: 0;
    background: transparent;
    font: inherit;
    text-align: left;
    cursor: default;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--text-row);
    font-weight: 600;
    color: var(--color-ink);
  }
  .folder__edit {
    display: flex;
    flex-direction: column;
    gap: 6px;
    flex: 1 1 auto;
    min-width: 0;
  }
  .folder__tools {
    display: inline-flex;
    gap: 2px;
    opacity: 0;
    transition: opacity var(--dur-fast) var(--ease-out-soft);
  }
  .folder:hover .folder__tools,
  .folder:focus-within .folder__tools {
    opacity: 1;
  }
  .tool {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: var(--color-ink-3);
    cursor: default;
    transition:
      background-color var(--dur-fast) var(--ease-out-soft),
      color var(--dur-fast) var(--ease-out-soft);
  }
  .tool:hover {
    background: var(--color-track);
    color: var(--color-ink);
  }
  .tool:active {
    background: var(--color-selected);
  }
  .msg {
    font-size: var(--text-secondary);
    font-weight: 600;
    color: var(--color-ink);
  }
  .hint {
    margin: 0;
    padding: 8px 16px 10px;
    background: var(--s-tile);
    font-size: var(--text-meta);
    color: var(--color-ink-3);
  }
  .create {
    align-items: flex-start;
  }
  .create .folder__icon {
    margin-top: 9px;
  }
</style>
