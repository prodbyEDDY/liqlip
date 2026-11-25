<script lang="ts">
  // Snippets (SPEC §3.10, §4): searchable list (create, delete, reorder) + editor with the
  // placeholder reference and a live preview of the expansion.
  //
  // Reordering commits through `snippet_move(id, beforeId)` — a command the §9.6 contract
  // does not have yet (proposed to the coordinator, same shape as `folder_move`). Until it
  // exists the first attempt reverts and the list explains that reordering comes later.
  import Button from "$lib/components/Button.svelte";
  import EmptyState from "$lib/components/EmptyState.svelte";
  import Ico from "$lib/components/Ico.svelte";
  import SearchField from "$lib/components/SearchField.svelte";
  import Skeleton from "$lib/components/Skeleton.svelte";
  import { call, clipsList, snippetDelete, snippetSave, snippetsList } from "$lib/ipc/commands";
  import { errorCode, errorMessage } from "$lib/ipc/errors";
  import { onClipsChanged } from "$lib/ipc/events";
  import type { Snippet } from "$lib/ipc/types";
  import { t } from "$lib/i18n/index.svelte";
  import { createReorder } from "../reorder.svelte";
  import { dialogs, toasts } from "../ui.svelte";
  import SnippetEditor from "../snippets/SnippetEditor.svelte";

  /** Proposed contract addition (store, W1-A): reorder snippets like folders. */
  const snippetMove = (id: number, beforeId: number | null) => call<void>("snippet_move", { id, beforeId });

  let list = $state<Snippet[]>([]);
  let status = $state<"loading" | "ready" | "error">("loading");
  let query = $state("");
  let selectedId = $state<number | null>(null);
  let focusNew = $state<number | null>(null);
  let total = $state(0);
  let reorderAvailable = $state(true);
  let creating = $state(false);

  let seq = 0;
  async function load(quiet = false) {
    const my = ++seq;
    if (!quiet) status = "loading";
    try {
      const items = await snippetsList(query.trim());
      if (my !== seq) return;
      list = items;
      if (!query.trim()) total = items.length;
      status = "ready";
      if (selectedId === null || !items.some((s) => s.id === selectedId)) selectedId = items[0]?.id ?? null;
    } catch {
      if (my === seq && !quiet) status = "error";
    }
  }

  let searchTimer: ReturnType<typeof setTimeout> | undefined;
  function onSearch(v: string) {
    query = v;
    clearTimeout(searchTimer);
    searchTimer = setTimeout(() => void load(true), 150);
  }

  // Latest copied text = what {clipboard} would most likely be (preview only).
  let clipboard = $state<string | null>(null);
  $effect(() => {
    void load();
    void clipsList({ text: "", exclude: [], filter: { kinds: ["text", "link", "code", "color", "contact"] }, cursor: null, limit: 1 })
      .then((p) => (clipboard = p.items[0]?.preview ?? null))
      .catch(() => {});
    // The store signals folder/snippet edits (here or from the shelf's "Save as snippet")
    // as `clips:changed { change: 'updated' }` without ids: re-read the list quietly.
    let changedTimer: ReturnType<typeof setTimeout> | undefined;
    const off = onClipsChanged((e) => {
      if (e.change !== "updated" || e.ids) return;
      clearTimeout(changedTimer);
      changedTimer = setTimeout(() => void load(true), 300);
    });
    return () => {
      clearTimeout(searchTimer);
      clearTimeout(changedTimer);
      void off.then((f) => f());
    };
  });

  const selected = $derived(list.find((s) => s.id === selectedId) ?? null);

  async function create() {
    creating = true;
    try {
      const s = await snippetSave({ name: t("settings.snippets.untitled"), body: "" });
      query = "";
      await load(true);
      if (!list.some((x) => x.id === s.id)) list = [...list, s];
      selectedId = s.id;
      // Focus the new name once (the editor reads it on mount), never again on re-mounts.
      focusNew = s.id;
      setTimeout(() => (focusNew = null), 0);
      total = Math.max(total, list.length);
    } catch (e) {
      toasts.show({ message: errorMessage(e), icon: "warning" });
    } finally {
      creating = false;
    }
  }

  async function remove(s: Snippet) {
    const ok = await dialogs.confirm({
      title: t("settings.snippets.deleteTitle", { name: s.name }),
      body: t("settings.snippets.deleteBody"),
      confirm: t("common.delete"),
      danger: true,
    });
    if (!ok) return;
    try {
      await snippetDelete(s.id);
      const at = list.findIndex((x) => x.id === s.id);
      list = list.filter((x) => x.id !== s.id);
      total = Math.max(0, total - 1);
      if (selectedId === s.id) selectedId = (list[at] ?? list[at - 1])?.id ?? null;
      toasts.show({ message: t("settings.snippets.deleted"), icon: "trash" });
    } catch (e) {
      toasts.show({ message: errorMessage(e), icon: "warning" });
    }
  }

  function onSaved(s: Snippet) {
    list = list.map((x) => (x.id === s.id ? s : x));
  }

  const reorder = createReorder<Snippet>({
    get: () => list,
    set: (items) => (list = items),
    commit: (id, beforeId) => snippetMove(id, beforeId),
    enabled: () => reorderAvailable && !query.trim(),
    onError: (e) => {
      const raw = String(e ?? "");
      if (errorCode(e) === "unimplemented" || raw.includes("snippet_move")) {
        reorderAvailable = false;
        toasts.show({ message: t("settings.snippets.reorderUnavailable"), icon: "info" });
      } else {
        toasts.show({ message: errorMessage(e), icon: "warning" });
      }
    },
  });

  function onItemKey(s: Snippet, e: KeyboardEvent) {
    if (reorder.key(s.id, e)) return;
    if (e.key === "Delete") {
      e.preventDefault();
      void remove(s);
    }
  }

  const firstLine = (body: string) => body.split("\n").find((l) => l.trim()) ?? "";
</script>

{#if status === "ready" && total === 0 && !query.trim()}
  <div class="empty squircle">
    <EmptyState icon="document" title={t("settings.snippets.empty")} desc={t("settings.snippets.emptyDesc")}>
      <Button variant="primary" icon="plus" loading={creating} onclick={create}>{t("settings.snippets.new")}</Button>
    </EmptyState>
  </div>
{:else}
  <div class="snip squircle">
    <aside class="pane pane--list">
      <div class="pane__top">
        <SearchField value={query} placeholder={t("settings.snippets.search")} onInput={onSearch} />
        <Button variant="secondary" icon="plus" loading={creating} onclick={create} aria-label={t("settings.snippets.new")} title={t("settings.snippets.new")} />
      </div>
      {#if status === "loading"}
        <div class="items">
          {#each [0, 1, 2, 3] as i (i)}
            <div class="sk"><Skeleton width={`${50 + ((i * 17) % 35)}%`} height="12px" /><Skeleton width="80%" height="10px" /></div>
          {/each}
        </div>
      {:else if status === "error"}
        <div class="pane__msg">
          <Ico name="warning" size={16} />
          <span>{t("settings.snippets.error")}</span>
          <Button variant="secondary" size="sm" icon="refresh" onclick={() => load()}>{t("common.retry")}</Button>
        </div>
      {:else if list.length === 0}
        <div class="pane__msg"><Ico name="search" size={16} /><span>{t("settings.snippets.noMatch", { q: query.trim() })}</span></div>
      {:else}
        <ul class="items" aria-label={t("settings.snippets.list")}>
          {#each list as s (s.id)}
            <li
              class="item-wrap"
              class:item-wrap--drag={reorder.dragId === s.id}
              draggable={reorderAvailable && !query.trim()}
              ondragstart={(e) => reorder.start(s.id, e)}
              ondragover={(e) => reorder.over(s.id, e)}
              ondrop={(e) => reorder.drop(e)}
              ondragend={() => reorder.end()}
            >
              <button
                type="button"
                class="item"
                class:item--sel={s.id === selectedId}
                aria-current={s.id === selectedId ? "true" : undefined}
                onclick={() => (selectedId = s.id)}
                onkeydown={(e) => onItemKey(s, e)}
              >
                <span class="item__name">{s.name}</span>
                <span class="item__body">{firstLine(s.body) || "—"}</span>
              </button>
            </li>
          {/each}
        </ul>
        <p class="pane__hint">
          {reorderAvailable ? (query.trim() ? "" : t("settings.list.reorderHint")) : t("settings.snippets.reorderUnavailable")}
        </p>
      {/if}
    </aside>
    <section class="pane pane--editor">
      {#if selected}
        {#key selected.id}
          <SnippetEditor snippet={selected} {clipboard} focusName={focusNew === selected.id} {onSaved} onDelete={() => remove(selected)} />
        {/key}
      {:else if status === "loading"}
        <div class="editor-sk">
          <Skeleton width="30%" height="12px" />
          <Skeleton height="38px" radius={12} />
          <Skeleton width="20%" height="12px" />
          <Skeleton height="168px" radius={12} />
        </div>
      {:else}
        <div class="pane__placeholder"><Ico name="document" size={22} /><span>{t("settings.snippets.select")}</span></div>
      {/if}
    </section>
  </div>
{/if}

<style>
  .empty {
    padding: 32px 0;
    border-radius: 16px;
    background: var(--s-tile);
  }
  .empty :global(.empty) {
    max-width: 380px;
  }
  .snip {
    display: grid;
    grid-template-columns: 250px minmax(0, 1fr);
    gap: 2px;
    min-height: 480px;
    border-radius: 16px;
    overflow: hidden;
  }
  .pane {
    min-width: 0;
    background: var(--s-tile);
  }
  .pane--list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 10px;
  }
  .pane__top {
    display: flex;
    gap: 6px;
  }
  .pane__top :global(.field) {
    flex: 1 1 0;
    min-width: 0;
  }
  .pane__top :global(.btn) {
    flex: 0 0 auto;
    width: 36px;
    padding: 0;
  }
  .items {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
    overflow-y: auto;
    max-height: 560px;
  }
  .sk {
    display: flex;
    flex-direction: column;
    gap: 7px;
    padding: 10px;
  }
  .item-wrap--drag {
    opacity: 0.5;
  }
  .item {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 100%;
    padding: 8px 10px;
    border: 0;
    border-radius: 10px;
    background: transparent;
    color: var(--color-ink);
    font: inherit;
    text-align: left;
    cursor: default;
    transition: background-color var(--dur-fast) var(--ease-out-soft);
  }
  .item:hover {
    background: var(--color-hover);
  }
  .item:active {
    background: var(--color-track);
  }
  .item--sel,
  .item--sel:hover {
    background: var(--color-selected);
  }
  .item__name,
  .item__body {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .item__name {
    font-size: var(--text-row);
    font-weight: 600;
  }
  .item__body {
    font-family: var(--font-mono);
    font-size: var(--text-meta);
    color: var(--color-ink-3);
  }
  .pane__msg {
    display: flex;
    align-items: flex-start;
    flex-wrap: wrap;
    gap: 8px;
    padding: 10px;
    font-size: var(--text-secondary);
    line-height: 1.45;
    color: var(--color-ink-2);
  }
  .pane__msg span {
    flex: 1 1 140px;
  }
  .pane__hint {
    margin: auto 0 0;
    padding: 6px 8px 0;
    font-size: var(--text-meta);
    line-height: 1.4;
    color: var(--color-ink-3);
  }
  .pane__placeholder {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    height: 100%;
    min-height: 320px;
    padding: 24px;
    font-size: var(--text-secondary);
    color: var(--color-ink-3);
    text-align: center;
  }
  .editor-sk {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 18px;
  }
  @media (max-width: 859px) {
    .snip {
      grid-template-columns: minmax(0, 1fr);
    }
    .items {
      max-height: 220px;
    }
  }
</style>
