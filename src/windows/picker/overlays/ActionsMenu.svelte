<script lang="ts">
  // The actions menu (SPEC §3.7) — Ctrl+K, right click, the ⋯ button. A searchable list:
  // typing filters (across submenus too, with the parent shown as the second line; English
  // labels match in the Russian UI), ↑/↓ + Enter run, → / ← open and leave a submenu (drill-in,
  // so it fits a 400 px edge panel), every item's shortcut is shown and also works while the
  // menu is open. Transform items preview their result live; a transform opens a level with
  // the full result and Paste / Copy / Save as a new item.
  //
  // The items come from `menuModel.ts` (pure, tested); this component renders them and runs
  // their commands through `runAction` and `layerActions`.
  import { onMount, tick, untrack } from "svelte";
  import Ico from "$lib/components/Ico.svelte";
  import Skeleton from "$lib/components/Skeleton.svelte";
  import { LayerItem, LayerSurface } from "./kit";
  import { clipDetail, settingsOpen } from "$lib/ipc/commands";
  import { errorMessage } from "$lib/ipc/errors";
  import { en } from "$lib/i18n/en";
  import { getLocale, t, type TranslationKey } from "$lib/i18n/index.svelte";
  import type { Clip, Format, MergeSeparator, Snippet } from "$lib/ipc/types";
  import { overlays, type LayerProps } from "../overlays.svelte";
  import { feed } from "../feed.svelte";
  import { shelf } from "../shelf.svelte";
  import { toasts } from "../toasts.svelte";
  import { copySnippet, copyText, pasteSnippet, pasteText, runAction } from "../actions.svelte";
  import { applyTransform, MONO_TRANSFORMS, type TransformId, type TransformResult } from "../transforms";
  import { buildMenu, chordNode, searchMenu, type MenuNode } from "./menuModel";
  import { SEPARATOR_TEXT, separatorPref } from "./separator.svelte";
  import { focusOnce } from "./focus";
  import {
    confirmDeleteRecent,
    confirmSnippetDelete,
    copyColorValue,
    copyImageText,
    copyPaths,
    createFolderAndMove,
    mergeIntoNew,
    moreFromApp,
    saveAsNew,
  } from "./layerActions";

  let { layer, close }: LayerProps = $props();

  const uid = $props.id();
  /** Rows of the transform preview (characters shown; the rest is summarised). */
  const PREVIEW_CHARS = 1600;

  // ── context ──
  const ids = $derived(layer.ctx.ids ?? []);
  const snippet = $derived((layer.ctx.snippet as Snippet | null | undefined) ?? null);
  /** The records as they were when the menu opened — a live reload of the feed (a capture, a
   *  search typed behind the menu) must not empty the menu; live copies win when present. */
  const opened = new Map(untrack(() => (layer.ctx.ids ?? []).flatMap((id) => (feed.clip(id) ? [[id, feed.clip(id)!] as const] : []))));
  const clips = $derived(ids.map((id) => feed.clip(id) ?? opened.get(id)).filter((c): c is Clip => c !== undefined));
  const single = $derived(clips.length === 1 ? clips[0] : null);
  const root = $derived(
    buildMenu({
      clips,
      snippet: !!snippet,
      folders: shelf.folders,
      appName: single?.appId != null ? (shelf.apps.get(single.appId)?.name ?? null) : null,
      ocrEnabled: shelf.settings?.ocr.enabled ?? true,
      separator: separatorPref.value,
    }),
  );

  // ── labels ──
  function labelOf(n: MenuNode): string {
    return n.text ?? t(n.label, n.vars);
  }
  function englishOf(n: MenuNode): string {
    if (n.text) return n.text;
    let s: string = en[n.label] ?? "";
    for (const [k, v] of Object.entries(n.vars ?? {})) s = s.replaceAll(`{${k}}`, String(v));
    return s;
  }
  const searchLabels = (n: MenuNode): string[] => (getLocale() === "en" ? [labelOf(n)] : [labelOf(n), englishOf(n)]);

  // ── navigation state ──
  /** Open submenus by id (resolved against the live tree, so live updates don't strand us). */
  let pathIds = $state<string[]>([]);
  let query = $state("");
  let active = $state(0);
  /** "New folder…": the field names the folder. */
  let naming = $state(false);
  let folderError = $state<TranslationKey | null>(null);
  let creating = $state(false);
  /** Row to land on after a level change (back to the submenu we came from). */
  let landOn: string | null = null;

  const path = $derived.by(() => {
    const out: MenuNode[] = [];
    let level = root;
    for (const id of pathIds) {
      const n = level.find((x) => x.id === id);
      if (!n?.children) break;
      out.push(n);
      level = n.children;
    }
    return out;
  });
  const parent = $derived(path.length ? path[path.length - 1] : null);
  const level = $derived(parent?.children ?? root);

  interface Row {
    key: string;
    node: MenuNode;
    /** Submenus between the current level and the node (search results). */
    via: MenuNode[];
    hint?: string;
    gap: boolean;
  }

  const rows: Row[] = $derived.by(() => {
    if (naming) return [];
    if (!query.trim()) {
      return level.map((node) => ({ key: node.id, node, via: [], hint: node.hint ? t(node.hint) : undefined, gap: !!node.gapBefore }));
    }
    return searchMenu(level, query, searchLabels).map((h) => ({
      key: [...h.path.map((p) => p.id), h.node.id].join("/"),
      node: h.node,
      via: h.path,
      hint: h.path.length ? h.path.map(labelOf).join(" › ") : h.node.hint ? t(h.node.hint) : undefined,
      gap: false,
    }));
  });

  // ── transform preview ──
  /** Full text of the record (the preview only has the start); loaded on first need. */
  let fullText = $state<string | null>(null);
  let textState = $state<"idle" | "loading" | "error">("idle");

  const previewId: TransformId | null = $derived.by(() => {
    if (parent?.cmd === "transform") return parent.arg as TransformId;
    const n = rows[active]?.node;
    return n?.cmd === "transform" ? (n.arg as TransformId) : null;
  });
  const previewBig = $derived(parent?.cmd === "transform");

  $effect(() => {
    if (!previewId || !single || fullText !== null || textState !== "idle") return;
    untrack(() => void loadText(single.id));
  });

  async function loadText(id: number): Promise<void> {
    textState = "loading";
    try {
      const d = await clipDetail(id);
      fullText = d.text ?? d.preview;
      textState = "idle";
    } catch {
      textState = "error";
    }
  }

  const result: TransformResult | null = $derived(
    previewId && fullText !== null ? applyTransform(previewId, fullText, { locale: getLocale() }) : null,
  );
  const resultError = $derived.by(() => {
    if (!result || result.ok) return null;
    const e = result.error;
    return e.code === "json" ? t("picker.transform.error.json", { line: e.line, column: e.column }) : t(`picker.transform.error.${e.code}` as TranslationKey);
  });
  const resultText = $derived(result?.ok ? result.text : null);

  function rowDisabled(row: Row): boolean {
    const n = row.node;
    if (n.disabled) return true;
    if (n.cmd === "transformPaste" || n.cmd === "transformCopy" || n.cmd === "transformSave") return resultText === null || resultText === "";
    return false;
  }

  // ── active row ──
  const enabledIdx = $derived(rows.flatMap((r, i) => (rowDisabled(r) ? [] : [i])));

  function firstEnabled(): number {
    if (landOn) {
      const at = rows.findIndex((r) => r.node.id === landOn);
      landOn = null;
      if (at >= 0) return at;
    }
    return enabledIdx[0] ?? 0;
  }

  // A new level or a new query starts at its first item (or where we came back to).
  $effect(() => {
    void query;
    void pathIds.length;
    void naming;
    untrack(() => {
      active = firstEnabled();
      if (listEl) listEl.scrollTop = 0;
      void tick().then(reveal);
    });
  });

  function move(delta: number) {
    const list = enabledIdx;
    if (list.length === 0) return;
    const at = list.indexOf(active);
    const next = at < 0 ? (delta > 0 ? 0 : list.length - 1) : Math.max(0, Math.min(list.length - 1, at + delta));
    active = Math.abs(delta) === 1 && at >= 0 ? list[(at + delta + list.length) % list.length] : list[next];
    void tick().then(reveal);
  }

  function reveal() {
    listEl?.querySelector<HTMLElement>(`#${CSS.escape(`${uid}-r${active}`)}`)?.scrollIntoView({ block: "nearest" });
  }

  // ── levels ──
  function openSubmenu(row: Row) {
    pathIds = [...pathIds.slice(0, path.length), ...row.via.map((v) => v.id), row.node.id];
    query = "";
  }

  function back(): boolean {
    if (naming) {
      naming = false;
      folderError = null;
      query = "";
      landOn = "folderNew";
      return true;
    }
    if (path.length === 0) return false;
    landOn = path[path.length - 1].id;
    pathIds = pathIds.slice(0, path.length - 1);
    query = "";
    return true;
  }

  function activate(row: Row | undefined) {
    if (!row || rowDisabled(row)) return;
    if (row.node.children) openSubmenu(row);
    else void exec(row.node);
  }

  // ── commands ──
  const one = () => ids.slice(0, 1);

  async function exec(n: MenuNode): Promise<void> {
    const s = snippet;
    switch (n.cmd) {
      case "folderNew":
        naming = true;
        folderError = null;
        query = "";
        return;
      case "transformPaste":
      case "transformCopy":
      case "transformSave": {
        const text = resultText;
        if (text === null) return;
        close();
        if (n.cmd === "transformPaste") await pasteText(text, { plain: true });
        else if (n.cmd === "transformCopy") await copyText(text);
        else await saveAsNew(text);
        return;
      }
    }
    close();
    switch (n.cmd) {
      case "paste":
        return s ? pasteSnippet(s, false) : runAction("paste", ids);
      case "pastePlain":
        return s ? pasteSnippet(s, true) : runAction("pastePlain", ids);
      case "pasteKeepOpen":
        return s ? pasteSnippet(s, false, true) : runAction("pasteKeepOpen", ids);
      case "pasteAs":
        return runAction("pasteAs", ids, { mode: { format: n.arg as Format } });
      case "copy":
        return s ? copySnippet(s) : runAction("copy", ids);
      case "copyAs":
        return runAction("copy", ids, { mode: { format: n.arg as Format } });
      case "copyPath":
        return copyPaths(ids[0]);
      case "copyColor":
        return void copyColorValue(String(n.arg));
      case "open":
      case "openWith":
      case "reveal":
        return runAction(n.cmd, one());
      case "preview":
        overlays.open("preview", { ids: s ? [s.id] : ids });
        return;
      case "ocr":
        if (single) await copyImageText(single);
        return;
      case "saveSnippet":
        overlays.open("snippet", { mode: "fromClip", clipId: ids[0] });
        return;
      case "favorite":
        return runAction("toggleFavorite", ids);
      case "folder":
        return runAction("moveToFolder", ids, { folderId: n.arg as number });
      case "folderRemove":
        return runAction("removeFromFolder", ids);
      case "rename":
        return runAction("rename", one());
      case "edit":
        overlays.open("preview", { ids: one(), edit: true });
        return;
      case "stack":
        return runAction("stack", ids);
      case "showStack":
        overlays.open("stack");
        return;
      case "moreFromApp":
        if (single?.appId != null) moreFromApp(single.appId);
        return;
      case "delete":
        return runAction("delete", ids);
      case "deleteRecent":
        confirmDeleteRecent(n.arg as number);
        return;
      case "pasteAll": {
        const sep = n.arg as MergeSeparator;
        separatorPref.set(sep);
        return runAction("paste", ids, { separator: SEPARATOR_TEXT[sep] });
      }
      case "copyAll":
        return runAction("copy", ids, { separator: SEPARATOR_TEXT[n.arg as MergeSeparator] });
      case "merge":
        return mergeIntoNew(ids);
      case "snippetEdit":
        if (s) overlays.open("snippet", { mode: "edit", snippet: s });
        return;
      case "snippetNew":
        overlays.open("snippet", { mode: "new" });
        return;
      case "snippetSettings":
        try {
          await settingsOpen("snippets");
        } catch (e) {
          toasts.error(errorMessage(e));
        }
        return;
      case "snippetDelete":
        if (s) confirmSnippetDelete(s);
        return;
    }
  }

  async function createFolder() {
    if (creating) return;
    const name = query.trim();
    if (!name) return;
    creating = true;
    folderError = await createFolderAndMove(name, ids);
    creating = false;
    if (!folderError) close();
  }

  // ── keyboard (the root routes keys here first while the menu is the top layer) ──
  $effect(() => {
    layer.onKey = (e: KeyboardEvent) => {
      const caret = inputEl?.selectionStart ?? 0;
      const caretEnd = inputEl ? caret === inputEl.value.length && inputEl.selectionEnd === caret : true;
      const plain = !e.ctrlKey && !e.altKey && !e.metaKey;
      if (e.key === "Escape") {
        if (query) query = "";
        else if (!back()) close();
        return true;
      }
      if (naming) {
        if (e.key === "Enter" && plain && !e.shiftKey) {
          void createFolder();
          return true;
        }
        if ((e.key === "ArrowLeft" && caret === 0 && caretEnd) || (e.key === "Backspace" && !query)) return back();
        return e.key === "Tab";
      }
      if (plain && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
        move(e.key === "ArrowDown" ? 1 : -1);
        return true;
      }
      if (plain && (e.key === "PageDown" || e.key === "PageUp")) {
        move(e.key === "PageDown" ? 8 : -8);
        return true;
      }
      if (plain && !query && (e.key === "Home" || e.key === "End")) {
        active = e.key === "Home" ? (enabledIdx[0] ?? 0) : (enabledIdx[enabledIdx.length - 1] ?? 0);
        void tick().then(reveal);
        return true;
      }
      if (plain && !e.shiftKey && e.key === "ArrowRight" && caretEnd && rows[active]?.node.children) {
        openSubmenu(rows[active]);
        return true;
      }
      if (plain && !e.shiftKey && e.key === "ArrowLeft" && caret === 0 && caretEnd && path.length) return back();
      if (e.key === "Backspace" && plain && !query && path.length) return back();
      if (e.key === "Tab") return true;
      // Shortcuts of this level's items work while the menu is open (Delete only while the
      // filter is empty — otherwise it edits the text).
      const activeNode = rows[active]?.node;
      const hit =
        (activeNode?.cmd === "transform" && activeNode.children ? chordNode(activeNode.children, e) : null) ??
        chordNode(query ? level.filter((n) => n.cmd !== "delete") : level, e);
      if (hit) {
        if (hit.cmd.startsWith("transform") && activeNode?.cmd === "transform") {
          // Ctrl+Enter / Ctrl+S on a transform in the list act on its result directly.
          if (resultText) void exec(hit);
        } else {
          activate({ key: hit.id, node: hit, via: [], gap: false });
        }
        return true;
      }
      if (e.key === "Enter" && plain && !e.shiftKey) {
        activate(rows[active]);
        return true;
      }
      return false;
    };
  });

  // ── layout ──
  let inputEl = $state<HTMLInputElement | null>(null);
  let listEl = $state<HTMLDivElement | null>(null);
  let areaEl = $state<HTMLDivElement | null>(null);
  /** The menu keeps the height it opened with: levels and filtering scroll inside it instead
   *  of resizing an anchored surface. */
  let fixedHeight = $state<number | null>(null);
  let fadeTop = $state(false);
  let fadeBottom = $state(false);

  function updateFades() {
    const el = listEl;
    if (!el) return;
    fadeTop = el.scrollTop > 2;
    fadeBottom = el.scrollTop + el.clientHeight < el.scrollHeight - 2;
  }

  const focused = { v: false };
  $effect(() => focusOnce(inputEl, focused, (el) => el.focus({ preventScroll: true })));

  onMount(() => {
    void tick().then(() => {
      if (areaEl) fixedHeight = Math.round(areaEl.getBoundingClientRect().height);
      updateFades();
    });
  });

  $effect(() => {
    void rows.length;
    void previewId;
    void tick().then(updateFades);
  });

  const crumb = $derived(naming ? t("picker.menu.folderNew").replace(/…$/, "") : parent ? labelOf(parent) : "");
  const placeholder = $derived(naming ? t("picker.menu.folderName") : path.length ? t("picker.menu.filter") : t("picker.menu.search"));
  const menuLabel = $derived(clips.length > 1 ? t("picker.menu.labelMany", { n: clips.length }) : t("picker.action.menu"));
  const previewMono = $derived(previewId !== null && MONO_TRANSFORMS.has(previewId));
  const previewUnchanged = $derived(resultText !== null && fullText !== null && resultText === fullText);
</script>

<LayerSurface variant="menu" role="dialog" anchor={layer.ctx.anchor ?? null} label={menuLabel} width={300} onclose={close}>
  {#snippet header()}
    <div class="search" class:search--nested={path.length > 0 || naming}>
      {#if path.length > 0 || naming}
        <button type="button" class="search__back" aria-label={t("picker.menu.back")} title={t("picker.menu.back")} onmousedown={(e) => e.preventDefault()} onclick={() => back()}>
          <Ico name="chevronLeft" size={14} />
        </button>
        <span class="search__crumb" title={crumb}>{crumb}</span>
      {:else}
        <span class="search__icon"><Ico name="search" size={14} /></span>
      {/if}
      <input
        bind:this={inputEl}
        bind:value={query}
        class="search__input selectable"
        type="text"
        spellcheck="false"
        autocomplete="off"
        maxlength={naming ? 64 : 80}
        {placeholder}
        aria-label={placeholder}
        aria-controls="{uid}-list"
        aria-activedescendant={!naming && rows.length ? `${uid}-r${active}` : undefined}
        onpointerdown={() => void shelf.makeInteractive()}
        oninput={() => (folderError = null)}
      />
      {#if clips.length > 1 && !path.length && !naming}
        <span class="search__count" title={menuLabel}>{clips.length}</span>
      {/if}
    </div>
  {/snippet}

  <div class="area" bind:this={areaEl} style:height={fixedHeight !== null ? `${fixedHeight}px` : undefined} style:flex={fixedHeight !== null ? "none" : undefined}>
    {#if naming}
      <div class="list">
        <LayerItem
          label={query.trim() ? t("picker.menu.folderCreate", { name: query.trim() }) : t("picker.menu.folderType")}
          icon="folderPlus"
          keys={query.trim() ? "↵" : undefined}
          disabled={!query.trim() || creating}
          active={!!query.trim()}
          onclick={() => void createFolder()}
        />
        {#if folderError}
          <p class="note note--error" role="alert"><Ico name="warning" size={14} />{t(folderError)}</p>
        {/if}
      </div>
    {:else}
      <div
        class="list"
        class:list--compact={previewBig}
        class:list--fade-top={fadeTop}
        class:list--fade-bottom={fadeBottom}
        bind:this={listEl}
        id="{uid}-list"
        role="menu"
        aria-label={menuLabel}
        tabindex="-1"
        onscroll={updateFades}
      >
        {#each rows as row, i (row.key)}
          {#if row.gap}<span class="gap" aria-hidden="true"></span>{/if}
          <LayerItem
            id="{uid}-r{i}"
            label={labelOf(row.node)}
            icon={row.node.icon}
            keys={row.node.keys}
            hint={row.hint}
            danger={row.node.danger}
            disabled={rowDisabled(row)}
            checked={row.node.checked}
            role={row.node.checked !== undefined ? "menuitemradio" : "menuitem"}
            submenu={!!row.node.children}
            active={i === active && !rowDisabled(row)}
            onpointermove={() => {
              if (!rowDisabled(row) && active !== i) active = i;
            }}
            onclick={() => activate(row)}
          >
            {#snippet lead()}
              {#if row.node.glyph}<span class="glyph">{row.node.glyph}</span>{:else}<Ico name={row.node.icon} size={16} />{/if}
            {/snippet}
            {#snippet trail()}
              {#if row.node.value}<span class="value" title={row.node.value}>{row.node.value}</span>{/if}
            {/snippet}
          </LayerItem>
        {:else}
          <p class="note">{t("picker.menu.empty")}</p>
        {/each}
      </div>
    {/if}

    {#if previewId && !naming}
      <section class="preview" class:preview--big={previewBig} aria-live="polite" aria-label={t("picker.transform.result")}>
        <header class="preview__head">
          <span>{t("picker.transform.result")}</span>
          {#if resultText !== null}<span class="preview__meta">{resultText.length.toLocaleString(getLocale())}</span>{/if}
        </header>
        {#if textState === "loading" || (fullText === null && textState === "idle")}
          <div class="preview__wait" aria-busy="true">
            <Skeleton width="86%" height="10px" radius={5} />
            <Skeleton width="64%" height="10px" radius={5} />
            <Skeleton width="72%" height="10px" radius={5} />
          </div>
        {:else if textState === "error"}
          <p class="preview__note preview__note--error"><Ico name="warning" size={14} />{t("picker.transform.loadError")}</p>
        {:else if resultError}
          <p class="preview__note preview__note--error"><Ico name="warning" size={14} />{resultError}</p>
        {:else if resultText === ""}
          <p class="preview__note">{t("picker.transform.empty")}</p>
        {:else if resultText !== null}
          <pre class="preview__text selectable" class:preview__text--mono={previewMono}>{resultText.slice(0, PREVIEW_CHARS)}{#if resultText.length > PREVIEW_CHARS}<span
                class="preview__more">… {t("picker.transform.more", { n: (resultText.length - PREVIEW_CHARS).toLocaleString(getLocale()) })}</span
              >{/if}</pre>
          {#if previewUnchanged}<p class="preview__note">{t("picker.transform.unchanged")}</p>{/if}
        {/if}
      </section>
    {/if}
  </div>
</LayerSurface>

<style>
  /* Filter field: a 32 px capsule in the menu's top corners (22 − 6 = 16 → concentric). The
     back button and the count are 24 px circles inset 4 px (16 − 4 = 12). */
  .search {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    gap: 8px;
    height: var(--h-md);
    padding: 0 4px 0 11px;
    border-radius: 999px;
    background: var(--color-track);
  }
  .search--nested {
    gap: 6px;
    padding-left: 4px;
  }
  .search__icon {
    display: inline-flex;
    flex: 0 0 auto;
    color: var(--color-ink-3);
  }
  .search__back {
    display: inline-flex;
    flex: 0 0 auto;
    align-items: center;
    justify-content: center;
    width: var(--h-sm);
    height: var(--h-sm);
    border: 0;
    border-radius: 999px;
    background: var(--color-raised);
    box-shadow: var(--shadow-on-lift);
    color: var(--color-ink);
    transition:
      background-color var(--dur-fast) var(--ease-out-soft),
      transform var(--dur-fast) var(--ease-out-soft);
  }
  .search__back:hover {
    background: var(--color-card-hover);
  }
  .search__back:active {
    transform: scale(0.92);
  }
  .search__crumb {
    flex: 0 1 auto;
    max-width: 55%;
    overflow: hidden;
    color: var(--color-ink);
    font-size: var(--text-secondary);
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .search__input {
    flex: 1 1 auto;
    min-width: 0;
    height: 100%;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--color-ink);
    font: inherit;
    font-size: var(--text-row);
    outline: none;
  }
  .search__input::placeholder {
    color: var(--color-ink-3);
  }
  .search__count {
    display: inline-flex;
    flex: 0 0 auto;
    align-items: center;
    justify-content: center;
    min-width: var(--h-sm);
    height: var(--h-sm);
    padding: 0 7px;
    border-radius: 999px;
    background: var(--color-ink);
    color: var(--color-paper);
    font-family: var(--font-mono);
    font-size: var(--text-meta);
    font-weight: 600;
  }

  .area {
    display: flex;
    flex: 1 1 auto;
    flex-direction: column;
    min-height: 0;
    max-height: 392px;
    margin-top: 6px;
  }
  .list {
    display: flex;
    flex: 1 1 auto;
    flex-direction: column;
    min-height: 0;
    overflow-y: auto;
    outline: none;
    scroll-padding-block: 4px;
  }
  /* A transform's level: its three rows, then the result takes the rest. */
  .list--compact {
    flex: 0 1 auto;
  }
  .list--fade-top {
    mask-image: linear-gradient(to bottom, transparent 0, #000 16px);
  }
  .list--fade-bottom {
    mask-image: linear-gradient(to top, transparent 0, #000 16px);
  }
  .list--fade-top.list--fade-bottom {
    mask-image: linear-gradient(to bottom, transparent 0, #000 16px, #000 calc(100% - 16px), transparent 100%);
  }
  .gap {
    display: block;
    flex: none;
    height: 6px;
  }
  .glyph {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 18px;
    color: var(--color-ink-2);
    font-family: var(--font-mono);
    font-size: var(--text-meta);
    font-weight: 650;
    letter-spacing: -0.02em;
  }
  .value {
    flex: 0 1 auto;
    max-width: 128px;
    overflow: hidden;
    color: var(--color-ink-3);
    font-family: var(--font-mono);
    font-size: var(--text-meta);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .note {
    display: flex;
    align-items: flex-start;
    gap: 7px;
    margin: 0;
    padding: 10px 10px 12px;
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
    line-height: var(--text-secondary--line-height);
  }
  .note--error {
    padding-top: 6px;
    color: var(--color-ink);
    font-weight: 600;
  }
  .note :global(.ico) {
    margin-top: 2px;
  }

  /* Transform result: a well in the menu's bottom corners (22 − 6 = 16 → --layer-r-in). */
  .preview {
    display: flex;
    flex: 0 0 auto;
    flex-direction: column;
    gap: 4px;
    min-height: 0;
    max-height: 118px;
    margin-top: 6px;
    padding: 8px 12px 10px;
    border-radius: var(--layer-r-in);
    background: var(--color-track);
  }
  .preview--big {
    flex: 1 1 auto;
    max-height: none;
    min-height: 96px;
  }
  .preview__head {
    display: flex;
    flex: 0 0 auto;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
    color: var(--color-ink-3);
    font-size: var(--text-meta);
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  .preview__meta {
    font-family: var(--font-mono);
    font-weight: 500;
    letter-spacing: 0;
    text-transform: none;
  }
  .preview__text {
    flex: 1 1 auto;
    min-height: 0;
    margin: 0;
    overflow: hidden;
    color: var(--color-ink);
    font-family: var(--font-sans);
    font-size: var(--text-secondary);
    line-height: 1.5;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    mask-image: linear-gradient(to top, transparent 0, #000 18px);
  }
  .preview--big .preview__text {
    overflow-y: auto;
    mask-image: none;
  }
  .preview__text--mono {
    font-family: var(--font-mono);
    font-size: 11.5px;
  }
  .preview__more {
    color: var(--color-ink-3);
  }
  .preview__wait {
    display: flex;
    flex-direction: column;
    gap: 7px;
    padding: 4px 0 2px;
  }
  .preview__note {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    margin: 0;
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
    line-height: var(--text-secondary--line-height);
  }
  .preview__note--error {
    color: var(--color-ink);
    font-weight: 600;
  }
  .preview__note :global(.ico) {
    flex: 0 0 auto;
    margin-top: 2px;
  }
</style>
