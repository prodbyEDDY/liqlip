<script lang="ts">
  // Snippets (SPEC §3.10, §4): searchable list (create, delete, reorder) + editor with the
  // placeholder reference and a live preview of the expansion.
  //
  // Reordering commits through `snippetMove(id, beforeId)` (W2-B, same shape as `folder_move`);
  // a failed move reverts and says why.
  import Button from "$lib/components/Button.svelte";
  import EmptyState from "$lib/components/EmptyState.svelte";
  import Ico from "$lib/components/Ico.svelte";
  import SearchField from "$lib/components/SearchField.svelte";
  import Skeleton from "$lib/components/Skeleton.svelte";
  import { clipboardText, clipsList, snippetDelete, snippetMove, snippetSave, snippetsList } from "$lib/ipc/commands";
  import { errorMessage } from "$lib/ipc/errors";
  import { onClipsChanged } from "$lib/ipc/events";
  import type { Snippet } from "$lib/ipc/types";
  import { t } from "$lib/i18n/index.svelte";
  import { scrollFade } from "$lib/scrollFade";
  import { createReorder } from "../reorder.svelte";
  import { dialogs, toasts } from "../ui.svelte";
  import SnippetEditor from "../snippets/SnippetEditor.svelte";

  let list = $state<Snippet[]>([]);
  let status = $state<"loading" | "ready" | "error">("loading");
  let query = $state("");
  let selectedId = $state<number | null>(null);
  let focusNew = $state<number | null>(null);
  let total = $state(0);
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

  // {clipboard} in the preview: the clipboard's current text (`clipboard_text`, what the shelf
  // pastes with — FIX-09), else the latest copied text from the history. Re-read on new copies
  // and when the window is shown — never while it is hidden (no clipboard access for nothing).
  let clipboard = $state<string | null>(null);
  async function readClipboard() {
    try {
      const now = await clipboardText();
      if (now !== null) {
        clipboard = now;
        return;
      }
    } catch {
      /* fall back to the history */
    }
    try {
      const p = await clipsList({ text: "", exclude: [], filter: { kinds: ["text", "link", "code", "color", "contact"] }, cursor: null, limit: 1 });
      clipboard = p.items[0]?.preview ?? null;
    } catch {
      /* the preview uses the sample text */
    }
  }
  $effect(() => {
    void load();
    void readClipboard();
    // The store signals folder/snippet edits (here or from the shelf's "Save as snippet")
    // as `clips:changed { change: 'updated' }` without ids: re-read the list quietly.
    let changedTimer: ReturnType<typeof setTimeout> | undefined;
    let clipTimer: ReturnType<typeof setTimeout> | undefined;
    const visible = () => document.visibilityState === "visible";
    const onVisible = () => visible() && void readClipboard();
    document.addEventListener("visibilitychange", onVisible);
    const off = onClipsChanged((e) => {
      if (e.change === "added" && visible()) {
        clearTimeout(clipTimer);
        clipTimer = setTimeout(() => void readClipboard(), 300);
      }
      if (e.change !== "updated" || e.ids) return;
      clearTimeout(changedTimer);
      changedTimer = setTimeout(() => void load(true), 300);
    });
    return () => {
      clearTimeout(searchTimer);
      clearTimeout(changedTimer);
      clearTimeout(clipTimer);
      document.removeEventListener("visibilitychange", onVisible);
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
    enabled: () => !query.trim(),
    onError: (e) => toasts.show({ message: errorMessage(e), icon: "warning" }),
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
{:else if status === "error"}
  <!-- Nothing to pick or edit: one error tile with the way out (never "select a snippet"). -->
  <div class="empty squircle">
    <EmptyState icon="warning" title={t("settings.snippets.error")}>
      <Button variant="secondary" icon="refresh" onclick={() => load()}>{t("common.retry")}</Button>
    </EmptyState>
  </div>
{:else}
  <div class="snip">
    <aside class="pane pane--list squircle">
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
      {:else if list.length === 0}
        <div class="pane__msg"><Ico name="search" size={16} /><span>{t("settings.snippets.noMatch", { q: query.trim() })}</span></div>
      {:else}
        <ul class="items" aria-label={t("settings.snippets.list")} use:scrollFade>
          {#each list as s (s.id)}
            <li
              class="item-wrap"
              class:item-wrap--drag={reorder.dragId === s.id}
              draggable={!query.trim()}
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
                title={firstLine(s.body) ? `${s.name}\n${firstLine(s.body)}` : s.name}
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
          {query.trim() ? "" : t("settings.list.reorderHint")}
        </p>
      {/if}
    </aside>
    <section class="pane pane--editor squircle">
      {#if selected}
        {#key selected.id}
          <SnippetEditor snippet={selected} {clipboard} focusName={focusNew === selected.id} {onSaved} onDelete={() => remove(selected)} />
        {/key}
      {:else if status === "loading"}
        <div class="editor-sk">
          <Skeleton width="30%" height="12px" />
          <Skeleton height="36px" circle />
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
    padding: 24px 0;
    border-radius: var(--r-tile);
    background: var(--color-tile);
  }
  /* Two tiles side by side (never a 2 px seam). List: padding 8 → rows r8 (16 − 8); its
     search row sits 16 px in, where the rule lets it keep its capsule. Editor: padding 16. */
  .snip {
    display: grid;
    grid-template-columns: 248px minmax(0, 1fr);
    gap: 12px;
    align-items: start;
  }
  .pane {
    min-width: 0;
    border-radius: var(--r-tile);
    background: var(--color-tile);
  }
  .pane--list {
    position: sticky;
    top: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 8px;
  }
  .pane__top {
    display: flex;
    gap: 6px;
    padding: 8px 8px 6px;
  }
  .pane__top :global(.field) {
    flex: 1 1 0;
    min-width: 0;
  }
  .items {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 520px;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    list-style: none;
  }
  .sk {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px;
  }
  .item-wrap--drag {
    opacity: 0.5;
  }
  .item {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 100%;
    padding: 8px 12px;
    border: 0;
    border-radius: max(4px, calc(var(--r-tile) - 8px));
    corner-shape: squircle;
    background: transparent;
    color: var(--color-ink);
    font: inherit;
    text-align: left;
    transition: background-color var(--dur-fast) var(--ease-out-soft);
  }
  .item:hover {
    background: var(--color-track);
  }
  .item:active {
    background: var(--color-hover);
  }
  .item--sel,
  .item--sel:hover {
    background: var(--color-selected);
  }
  .item:focus-visible {
    outline-offset: -2px;
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
  /* Content preview, not a service mark: 12 px; ink-2 on the chosen row's wash (ink-3 there
     falls under 4.5:1 in both themes). */
  .item__body {
    color: var(--color-ink-3);
    font-family: var(--font-mono);
    font-size: var(--text-secondary);
  }
  .item--sel .item__body {
    color: var(--color-ink-2);
  }
  .pane__msg {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: 8px;
    padding: 10px 12px;
    color: var(--color-ink-2);
    font-size: var(--text-secondary);
    line-height: var(--text-secondary--line-height);
  }
  .pane__msg span {
    flex: 1 1 140px;
  }
  .pane__hint {
    margin: 0;
    padding: 6px 12px 6px;
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
    line-height: var(--text-secondary--line-height);
  }
  .pane__hint:empty {
    display: none;
  }
  .pane__placeholder {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    min-height: 320px;
    padding: 24px;
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
    text-align: center;
  }
  .editor-sk {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 16px;
  }
  @media (max-width: 859px) {
    .snip {
      grid-template-columns: minmax(0, 1fr);
    }
    .pane--list {
      position: static;
    }
    .items {
      max-height: 220px;
    }
  }
</style>
