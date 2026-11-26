<script lang="ts">
  // Quick Look (SPEC §3.8): a sheet over the grid (the shelf keeps its size) — the record big,
  // an info table from `clip_detail`, editing of text records.
  //   view   text (search matches, invisible characters), code (mono), link (URL + Open),
  //          image (zoom / pan, recognized text), files (list, "not found"), colour (swatch +
  //          HEX / RGB / HSL, click copies), contact; a snippet shows its body and what it
  //          would paste right now.
  //   keys   ←/→ (↑/↓) neighbour — within the selection when opened on several, else the feed
  //          (loading more at its end) · Enter paste (⇧ plain, Ctrl copy, Alt keep open) ·
  //          Space / Esc close · Ctrl+E edit · Ctrl+I invisibles · +/−/0 zoom · Ctrl+O open ·
  //          Ctrl+D favorite · Delete · Ctrl+K actions · Ctrl+C copies the record when no text
  //          is selected.
  //   edit   textarea, Ctrl+S saves (`clip_update_text`), Esc cancels — with a confirmation
  //          when there are unsaved changes; `invalid: duplicate` is explained at the field.
  // Layout: preview beside the table on a wide panel, above it on a narrow one (container
  // query on the sheet's body).
  import { onMount, tick, untrack } from "svelte";
  import Ico from "$lib/components/Ico.svelte";
  import Button from "$lib/components/Button.svelte";
  import Skeleton from "$lib/components/Skeleton.svelte";
  import { LayerFooter, LayerHeader, LayerSurface } from "./kit";
  import { clipDetail, clipOcr, clipUpdateText, settingsOpen } from "$lib/ipc/commands";
  import { errorCode, errorMessage } from "$lib/ipc/errors";
  import { getLocale, t, type TranslationKey } from "$lib/i18n/index.svelte";
  import type { IconName } from "$lib/icons";
  import { isImageKind, type ClipDetail, type Format, type Kind, type Snippet } from "$lib/ipc/types";
  import { parseSnippet } from "$lib/snippets/expand";
  import { overlays, type LayerProps } from "../overlays.svelte";
  import { feed } from "../feed.svelte";
  import { selection } from "../selection.svelte";
  import { shelf } from "../shelf.svelte";
  import { view } from "../view.svelte";
  import { toasts } from "../toasts.svelte";
  import { copySnippet, pasteSnippet, runAction } from "../actions.svelte";
  import { colorValue, formatSize, hostPath, linkHost, pluralCategory, relativeTime } from "../format";
  import { baseName, entryFormat, fileIcon, pathTail } from "../preview";
  import ViewText from "./ViewText.svelte";
  import ImageStage from "./ImageStage.svelte";
  import { focusLayer, focusOnce, trapTab } from "./focus";
  import { confirmDiscard, copyColorValue, copyImageText } from "./layerActions";
  import { previewClipboardText, previewExpansion } from "./snippetText";
  import { TEXT_KINDS } from "./menuModel";
  import { qlPrefs } from "./qlPrefs.svelte";

  let { layer, close }: LayerProps = $props();
  const uid = $props.id();

  const KIND_ICON: Record<Kind, IconName> = {
    text: "text",
    link: "link",
    code: "code",
    color: "palette",
    contact: "at",
    image: "image",
    file: "file",
    screenshot: "image", // W5: behaves like an image (W6-F may give it its own glyph)
  };

  // ── which record ──
  const opened = untrack(() => layer.ctx.ids ?? []);
  /** Opened on several selected records: ←/→ browse those; otherwise the whole feed. */
  const scope: number[] | null = opened.length > 1 ? opened : null;
  let id = $state<number | null>(opened.includes(untrack(() => selection.current) ?? -1) ? untrack(() => selection.current) : (opened[0] ?? null));

  const snippetMode = $derived(feed.mode === "snippets");
  const snippet: Snippet | undefined = $derived(snippetMode && id !== null ? feed.snippet(id) : undefined);

  // ── detail ──
  let detail = $state<ClipDetail | null>(null);
  let loadError = $state<unknown>(null);
  let seq = 0;

  async function load(target: number, quiet = false): Promise<void> {
    const mine = ++seq;
    if (!quiet) {
      detail = null;
      loadError = null;
    }
    try {
      const d = await clipDetail(target);
      if (mine === seq) {
        detail = d;
        loadError = null;
      }
    } catch (e) {
      if (mine === seq && !quiet) loadError = e;
    }
  }

  $effect(() => {
    const target = id;
    if (target === null || snippetMode) return;
    untrack(() => void load(target));
  });

  // The feed's copy changed (OCR finished, renamed, favorite, used…): refresh quietly.
  let seen: unknown = null;
  $effect(() => {
    const c = id !== null && !snippetMode ? feed.clip(id) : undefined;
    if (!c || c === seen) return;
    const first = seen === null;
    seen = c;
    if (first) return;
    untrack(() => {
      if (!editing && detail && detail.id === c.id) void load(c.id, true);
    });
  });

  // ── editing ──
  let editing = $state(false);
  let draft = $state("");
  let editError = $state<string | null>(null);
  let saving = $state(false);
  let area = $state<HTMLTextAreaElement | null>(null);
  const wantsEdit = untrack(() => layer.ctx.edit === true);

  const canEdit = $derived(!!detail && TEXT_KINDS.includes(detail.kind));
  const dirty = $derived(editing && !!detail && draft !== (detail.text ?? detail.preview));

  function startEdit() {
    if (snippet) {
      overlays.open("snippet", { mode: "edit", snippet });
      return;
    }
    if (!detail || !canEdit) return;
    draft = detail.text ?? detail.preview;
    editError = null;
    editing = true;
    void tick().then(() => {
      if (!area) return;
      area.focus({ preventScroll: true });
      // Short texts: the caret at the end (appending is the common edit); long ones stay at
      // the top where reading starts.
      const end = draft.length <= 2_000 ? draft.length : 0;
      area.setSelectionRange(end, end);
      if (end === 0) area.scrollTop = 0;
    });
  }

  // Opened with Ctrl+E (or "Edit" in the menu): straight into the editor once loaded.
  let editedOnce = false;
  $effect(() => {
    if (wantsEdit && detail && !editing && !editedOnce) {
      editedOnce = true;
      untrack(startEdit);
    }
  });

  function stopEdit() {
    editing = false;
    editError = null;
    void tick().then(() => focusLayer(sheet));
  }

  function cancelEdit() {
    if (dirty) confirmDiscard(stopEdit, () => void tick().then(() => area?.focus({ preventScroll: true })));
    else stopEdit();
  }

  async function save() {
    if (!detail || saving) return;
    if (draft === "") {
      editError = t("picker.ql.editEmpty");
      return;
    }
    if (!dirty) {
      stopEdit();
      return;
    }
    saving = true;
    editError = null;
    const target = detail.id;
    shelf.markOwn([target]);
    try {
      const clip = await clipUpdateText(target, draft);
      feed.replace(clip);
      await load(target, true);
      stopEdit();
      toasts.show(t("picker.ql.saved"));
    } catch (e) {
      const raw = String(e);
      editError = errorCode(e) === "invalid" && raw.includes("duplicate") ? t("picker.ql.editDuplicate") : errorMessage(e);
    } finally {
      saving = false;
    }
  }

  // ── browsing ──
  function neighbour(delta: number): number | null {
    if (id === null) return null;
    if (scope) {
      const live = scope.filter((x) => feed.index.has(x));
      const i = live.indexOf(id);
      return live[i + delta] ?? null;
    }
    const i = feed.indexOf(id);
    return i < 0 ? null : feed.idAt(i + delta);
  }

  let stepping = false;
  async function step(delta: number) {
    if (editing || stepping) return;
    let next = neighbour(delta);
    if (next === null && delta > 0 && !scope && feed.hasMore) {
      stepping = true;
      await feed.loadMore();
      stepping = false;
      next = neighbour(delta);
    }
    if (next === null) return;
    go(next);
  }

  function go(next: number) {
    id = next;
    selection.focus(next);
    if (!scope) selection.clearMulti();
    view.reveal(feed.indexOf(next));
  }

  // The record left the feed behind the sheet (deleted from the actions menu opened here, a
  // "delete recent"): show the one that took its place — or close — instead of a record that
  // no longer exists (Enter on it would only say "not found").
  let lastIndex = -1;
  $effect(() => {
    if (snippetMode || id === null || editing) return;
    const i = feed.indexOf(id);
    if (i >= 0) {
      lastIndex = i;
      return;
    }
    if (feed.status !== "ready" || lastIndex < 0) return;
    const gone = id;
    untrack(() => {
      let next: number | null;
      if (scope) {
        const at = scope.indexOf(gone);
        const live = (x: number) => feed.index.has(x);
        next = scope.slice(at + 1).find(live) ?? scope.slice(0, Math.max(0, at)).reverse().find(live) ?? null;
      } else {
        next = feed.count ? feed.idAt(Math.min(lastIndex, feed.count - 1)) : null;
      }
      if (next !== null) go(next);
      else close();
    });
  });

  const position = $derived.by(() => {
    if (id === null) return null;
    if (scope) {
      const live = scope.filter((x) => feed.index.has(x));
      return t("picker.ql.position", { i: live.indexOf(id) + 1, n: live.length });
    }
    const i = feed.indexOf(id);
    if (i < 0) return null;
    return t(feed.hasMore ? "picker.ql.positionMore" : "picker.ql.position", { i: i + 1, n: feed.count });
  });

  // ── actions ──
  function paste(plain: boolean, keepOpen = false) {
    if (id === null) return;
    if (!keepOpen) close();
    if (snippet) void pasteSnippet(snippet, plain, keepOpen);
    else void runAction(keepOpen ? "pasteKeepOpen" : plain ? "pastePlain" : "paste", [id]);
  }

  function copy() {
    if (id === null) return;
    if (snippet) void copySnippet(snippet);
    else void runAction("copy", [id]);
  }

  async function remove() {
    if (id === null || snippetMode) return;
    const target = id;
    const next = neighbour(1) ?? neighbour(-1);
    await runAction("delete", [target]);
    if (next !== null && feed.index.has(next)) go(next);
    else close();
  }

  function toggleFavorite() {
    if (id === null || snippetMode || !detail) return;
    detail = { ...detail, favorite: !detail.favorite };
    void runAction("toggleFavorite", [id]);
  }

  function openActions() {
    if (id === null) return;
    const r = sheet?.getBoundingClientRect();
    overlays.open("actions", {
      ids: [id],
      snippet: snippet ?? null,
      // Above the footer's buttons, at the sheet's right edge (the surface clamps it).
      anchor: r ? { x: r.right - 312, y: r.bottom - 52 } : undefined,
    });
  }

  // ── image ──
  let stage = $state<ReturnType<typeof ImageStage> | null>(null);
  let ocrBusy = $state(false);

  async function recognize() {
    if (!detail || ocrBusy) return;
    ocrBusy = true;
    try {
      await clipOcr(detail.id);
      await load(detail.id, true);
    } catch (e) {
      const code = errorCode(e);
      toasts.error(code === "busy" ? t("picker.ocr.busy") : code === "invalid" ? t("picker.ocr.off") : t("picker.ocr.failed"));
    } finally {
      ocrBusy = false;
    }
  }

  // ── snippet preview ──
  const sampleUuid = "3f2b8c1e-5d4a-4e7b-9c0d-1a2b3c4d5e6f";
  let clipSample = $state<string | null>(null);
  onMount(() => {
    if (untrack(() => feed.mode) === "snippets") {
      previewClipboardText()
        .then((s) => (clipSample = s))
        .catch(() => {});
    }
  });
  const expansion = $derived(snippet ? previewExpansion(snippet.body, clipSample ?? t("settings.snippets.preview.sample"), Date.now(), sampleUuid) : null);

  // ── keyboard ──
  let sheet = $state<HTMLDivElement | null>(null);

  const focused = { v: false };
  $effect(() => focusOnce(sheet, focused));

  $effect(() => {
    layer.onKey = (e: KeyboardEvent) => {
      const ctrl = e.ctrlKey && !e.altKey && !e.metaKey;
      const plain = !e.ctrlKey && !e.altKey && !e.metaKey;
      if (editing) {
        if (ctrl && !e.shiftKey && e.code === "KeyS") {
          void save();
          return true;
        }
        if (e.key === "Escape") {
          cancelEdit();
          return true;
        }
        return trapTab(e, sheet);
      }
      // Opened with Ctrl+E and the editor isn't there yet (the record is loading): keys typed
      // right away belong to the editor, not to Quick Look — a leading Space would close the
      // sheet and the rest would land in the search. Swallow them; Esc still leaves.
      if (wantsEdit && !editedOnce && !loadError) {
        if (e.key === "Escape") close();
        return true;
      }
      const onControl = document.activeElement instanceof HTMLButtonElement || document.activeElement instanceof HTMLInputElement;
      if (trapTab(e, sheet)) return true;
      if (plain && !e.shiftKey && (e.key === "ArrowLeft" || e.key === "ArrowUp")) {
        void step(-1);
        return true;
      }
      if (plain && !e.shiftKey && (e.key === "ArrowRight" || e.key === "ArrowDown")) {
        void step(1);
        return true;
      }
      if (e.code === "Enter" || e.code === "NumpadEnter") {
        if (onControl && plain && !e.shiftKey) return false; // the focused button owns it
        if (e.repeat) return true;
        if (ctrl && !e.shiftKey) copy();
        else if (e.altKey && !e.ctrlKey && !e.shiftKey) paste(false, true);
        else if (plain) paste(e.shiftKey);
        return true;
      }
      if (e.code === "Space" && plain && !e.shiftKey) {
        if (onControl) return false;
        close();
        return true;
      }
      if (ctrl && !e.shiftKey) {
        switch (e.code) {
          case "KeyE":
            startEdit();
            return true;
          case "KeyI":
            if (detail && (detail.kind === "text" || detail.kind === "code")) qlPrefs.invisibles = !qlPrefs.invisibles;
            return true;
          case "KeyO":
            if (id !== null && !snippetMode) void runAction("open", [id]);
            return true;
          case "KeyD":
            toggleFavorite();
            return true;
          case "KeyK":
            openActions();
            return true;
          case "KeyZ":
            // Delete here shows "Deleted · Undo" over the sheet; Ctrl+Z works as on the grid.
            if (toasts.undoToken) void shelf.undo();
            return true;
          case "KeyC":
            if (window.getSelection()?.toString()) return false; // copy the selected text natively
            copy();
            return true;
          case "Equal":
          case "NumpadAdd":
            stage?.zoomIn();
            return true;
          case "Minus":
          case "NumpadSubtract":
            stage?.zoomOut();
            return true;
          case "Digit0":
          case "Numpad0":
            stage?.fitToView();
            return true;
        }
      }
      if (ctrl && e.shiftKey && e.code === "KeyO") {
        if (id !== null && !snippetMode) void runAction("reveal", [id]);
        return true;
      }
      if (plain && detail && isImageKind(detail.kind)) {
        if (e.code === "Equal" || e.code === "NumpadAdd" || e.key === "+") {
          stage?.zoomIn();
          return true;
        }
        if (e.code === "Minus" || e.code === "NumpadSubtract") {
          stage?.zoomOut();
          return true;
        }
        if (e.code === "Digit0" || e.code === "Numpad0") {
          stage?.fitToView();
          return true;
        }
      }
      if (plain && !e.shiftKey && e.code === "Delete") {
        if (!e.repeat) void remove();
        return true;
      }
      return false;
    };
  });

  // ── info table ──
  const locale = $derived(getLocale());
  const dateTime = (at: number) => new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeStyle: "short" }).format(at);
  const plural = (key: string, n: number) => t(`${key}.${pluralCategory(n, locale)}` as TranslationKey, { n: n.toLocaleString(locale) });
  const sizeText = (bytes: number | null | undefined) => {
    const s = formatSize(bytes);
    return s ? t(`common.size.${s.unit}` as TranslationKey, { n: s.n }) : null;
  };

  interface InfoRow {
    label: string;
    value: string;
    /** Tooltip of the value (a shortened path → the full one). */
    title?: string;
    sub?: string;
    icon?: string | null;
    appIcon?: boolean;
  }

  const app = $derived(detail?.appId != null ? shelf.apps.get(detail.appId) : undefined);

  const info: InfoRow[] = $derived.by(() => {
    if (snippet) {
      const rows: InfoRow[] = [{ label: t("picker.ql.type"), value: t("picker.ql.snippet") }];
      rows.push({ label: t("picker.ql.text"), value: plural("picker.ql.chars", [...snippet.body].length) });
      rows.push({
        label: t("picker.ql.used"),
        value: snippet.useCount > 0 ? plural("picker.ql.times", snippet.useCount) : t("picker.ql.usedNever"),
      });
      rows.push({ label: t("picker.ql.updated"), value: dateTime(snippet.updatedAt) });
      return rows;
    }
    const d = detail;
    if (!d) return [];
    const now = Date.now();
    const rows: InfoRow[] = [
      { label: t("picker.ql.source"), value: app?.name ?? t("picker.ql.unknownApp"), icon: app?.iconUrl ?? null, appIcon: true },
      { label: t("picker.ql.type"), value: `${t(`common.kind.${d.kind}` as TranslationKey)} · ${entryFormat(d)}` },
    ];
    if (d.formats.length > 1) {
      rows.push({ label: t("picker.ql.formats"), value: d.formats.map((f: Format) => t(`picker.menu.format.${f}` as TranslationKey)).join(", ") });
    }
    if (isImageKind(d.kind)) {
      const px = d.width && d.height ? t("picker.ql.pixels", { w: d.width, h: d.height }) : null;
      rows.push({ label: t("picker.ql.size"), value: [px, sizeText(d.size)].filter(Boolean).join(" · ") || "—" });
    } else if (d.kind === "file") {
      const n = d.files?.length ?? d.fileCount ?? 0;
      const total = (d.files ?? []).reduce((a, f) => a + (f.size ?? 0), 0);
      rows.push({ label: t("picker.ql.size"), value: [plural("picker.ql.files", n), sizeText(total)].filter(Boolean).join(" · ") });
      // FIX-08: the folder of the first file — its tail, the full path on hover.
      const tail = pathTail(d.meta.dir);
      if (tail) rows.push({ label: t("picker.ql.location"), value: tail, title: d.meta.dir });
    } else {
      const s = sizeText(d.size);
      if (s) rows.push({ label: t("picker.ql.size"), value: s });
    }
    if (TEXT_KINDS.includes(d.kind) && d.charCount !== null) {
      const parts = [plural("picker.ql.chars", d.charCount)];
      if (d.wordCount !== null) parts.push(plural("picker.ql.words", d.wordCount));
      if (d.lineCount !== null) parts.push(plural("picker.ql.lines", d.lineCount));
      rows.push({ label: t("picker.ql.text"), value: parts.join(" · ") });
    }
    rows.push({
      label: t("picker.ql.copied"),
      value: dateTime(d.createdAt),
      sub: d.copyCount > 1 || d.copiedAt - d.createdAt > 60_000 ? t("picker.ql.copiedAgain", { when: relativeTime(d.copiedAt, now, locale), times: plural("picker.ql.times", d.copyCount) }) : undefined,
    });
    rows.push({
      label: t("picker.ql.used"),
      value:
        d.useCount > 0 && d.usedAt !== null
          ? t("picker.ql.usedLast", { times: plural("picker.ql.times", d.useCount), when: relativeTime(d.usedAt, now, locale) })
          : t("picker.ql.usedNever"),
    });
    if (d.folderId !== null) rows.push({ label: t("picker.ql.folder"), value: shelf.folderName(d.folderId) || "—" });
    if (d.favorite) rows.push({ label: t("picker.ql.favorite"), value: t("picker.ql.favoriteYes") });
    return rows;
  });

  // ── header ──
  const title = $derived.by(() => {
    if (snippet) return snippet.name;
    const d = detail ?? (id !== null ? feed.clip(id) : undefined);
    if (!d) return t("picker.preview.label");
    if (d.title) return d.title;
    switch (d.kind) {
      case "link":
        return hostPath(d.meta.url ?? d.preview);
      case "file":
        return baseName(d.preview.split("\n")[0] ?? d.preview);
      case "color":
      case "contact":
        return d.preview;
      default:
        return t(`common.kind.${d.kind}` as TranslationKey);
    }
  });
  const kind = $derived(detail?.kind ?? (id !== null ? feed.clip(id)?.kind : undefined));
  const textual = $derived(detail?.kind === "text" || detail?.kind === "code");

  const hints = $derived.by((): [string, string][] => {
    if (editing) return [["Ctrl S", t("common.save")], ["Esc", t("common.cancel")]];
    const h: [string, string][] = [["↵", t("picker.preview.paste")], ["← →", t("picker.preview.browse")]];
    if (canEdit || snippet) h.push(["Ctrl E", t("picker.ql.hint.edit")]);
    h.push(["Ctrl K", t("picker.ql.hint.actions")]);
    return h;
  });

  const ocrEnabled = $derived(shelf.settings?.ocr.enabled ?? true);
</script>

<LayerSurface variant="sheet" labelledby="{uid}-title" onclose={editing ? cancelEdit : close} bind:el={sheet}>
  {#snippet header()}
    <LayerHeader title={title} id="{uid}-title" icon={snippet ? "document" : kind ? KIND_ICON[kind] : undefined} count={position} onclose={editing ? cancelEdit : close}>
      {#snippet tools()}
        {#if !editing && textual}
          <Button
            variant="ghost"
            size="sm"
            icon={qlPrefs.invisibles ? "eye" : "eyeOff"}
            title={t("picker.ql.invisibles")}
            aria-label={t("picker.ql.invisibles")}
            aria-pressed={qlPrefs.invisibles}
            onmousedown={(e: MouseEvent) => e.preventDefault()}
            onclick={() => (qlPrefs.invisibles = !qlPrefs.invisibles)}
          />
        {/if}
        {#if !editing && (canEdit || snippet)}
          <Button variant="ghost" size="sm" icon="edit" title={t("picker.ql.edit")} aria-label={t("picker.ql.edit")} onmousedown={(e: MouseEvent) => e.preventDefault()} onclick={startEdit} />
        {/if}
        {#if !editing && detail && (detail.kind === "link" || detail.kind === "file" || isImageKind(detail.kind))}
          <Button variant="ghost" size="sm" icon="external" title={t("picker.ql.open")} aria-label={t("picker.ql.open")} onmousedown={(e: MouseEvent) => e.preventDefault()} onclick={() => id !== null && void runAction("open", [id])} />
        {/if}
      {/snippet}
    </LayerHeader>
  {/snippet}

  <div class="wrap">
    <div class="ql" class:ql--single={editing}>
      <div class="ql__view">
        {#if snippet}
          <div class="well well--text scroll">
            <pre class="body selectable">{#each parseSnippet(snippet.body) as tok (tok.start)}{#if tok.kind === "placeholder"}<span class="ph" class:ph--bad={tok.issue !== null}>{tok.raw}</span>{:else}{tok.text}{/if}{/each}</pre>
          </div>
          {#if expansion}
            <div class="label">{t("picker.ql.snippetResult")}</div>
            <div class="well well--result scroll selectable">
              {#if expansion.caretIndex === null}{expansion.text}{:else}{expansion.text.slice(0, expansion.caretIndex)}<span class="caret" aria-hidden="true"></span>{expansion.text.slice(expansion.caretIndex)}{/if}
            </div>
          {/if}
        {:else if loadError}
          <div class="well well--state">
            <Ico name="warning" size={20} />
            <p>{t("picker.ql.loadError")} · {errorMessage(loadError)}</p>
            <Button variant="secondary" size="sm" icon="refresh" onclick={() => id !== null && void load(id)}>{t("common.retry")}</Button>
          </div>
        {:else if !detail}
          <div class="well well--text" aria-busy="true">
            <div class="skeleton">
              <Skeleton width="78%" height="12px" radius={6} />
              <Skeleton width="92%" height="12px" radius={6} />
              <Skeleton width="64%" height="12px" radius={6} />
              <Skeleton width="84%" height="12px" radius={6} />
              <Skeleton width="40%" height="12px" radius={6} />
            </div>
          </div>
        {:else if editing}
          <textarea
            bind:this={area}
            bind:value={draft}
            class="well edit selectable"
            class:edit--mono={detail.kind === "code"}
            spellcheck="false"
            aria-label={t("picker.ql.editLabel")}
            aria-invalid={!!editError || undefined}
            aria-describedby={editError ? `${uid}-err` : detail.formats.some((f) => f === "html" || f === "rtf") ? `${uid}-fmt` : undefined}
            oninput={() => (editError = null)}
          ></textarea>
          {#if editError}
            <p class="note note--error" id="{uid}-err" role="alert"><Ico name="warning" size={14} />{editError}</p>
          {:else if detail.formats.some((f) => f === "html" || f === "rtf")}
            <p class="note" id="{uid}-fmt"><Ico name="info" size={14} />{t("picker.ql.editFormats")}</p>
          {/if}
        {:else if isImageKind(detail.kind)}
          <ImageStage bind:this={stage} src={detail.imageUrl ?? detail.thumbUrl ?? ""} alt={title} width={detail.width} height={detail.height} />
          <div class="ocr" class:ocr--open={qlPrefs.ocrOpen}>
            <button type="button" class="ocr__toggle" aria-expanded={qlPrefs.ocrOpen} aria-controls="{uid}-ocr" onmousedown={(e) => e.preventDefault()} onclick={() => (qlPrefs.ocrOpen = !qlPrefs.ocrOpen)}>
              <Ico name="ocr" size={15} />
              <span class="ocr__title">{t("picker.ocr.title")}</span>
              <span class="ocr__state">
                {#if detail.ocrText}{plural("picker.ql.chars", detail.ocrText.length)}{:else if detail.ocrText === ""}{t("picker.ocr.empty")}{:else if detail.ocr === "pending"}{t("picker.ocr.pending")}{:else}{t("picker.ocr.none")}{/if}
              </span>
              <span class="ocr__chev"><Ico name={qlPrefs.ocrOpen ? "chevronDown" : "chevronRight"} size={14} /></span>
            </button>
            {#if qlPrefs.ocrOpen}
              <div class="ocr__body" id="{uid}-ocr">
                {#if detail.ocrText}
                  <pre class="ocr__text selectable">{detail.ocrText}</pre>
                  <div class="ocr__actions"><Button variant="secondary" size="sm" icon="copy" onclick={() => detail && void copyImageText(detail)}>{t("picker.ocr.copy")}</Button></div>
                {:else if detail.ocrText === ""}
                  <p class="ocr__note">{t("picker.ocr.empty")}</p>
                {:else if detail.ocr === "pending" || ocrBusy}
                  <div class="skeleton skeleton--tight" aria-busy="true"><Skeleton width="70%" height="10px" radius={5} /><Skeleton width="46%" height="10px" radius={5} /></div>
                {:else if !ocrEnabled}
                  <p class="ocr__note">{t("picker.ocr.off")}</p>
                  <div class="ocr__actions"><Button variant="secondary" size="sm" icon="settings" onclick={() => void settingsOpen("ocr").catch(() => {})}>{t("picker.ocr.settings")}</Button></div>
                {:else}
                  <p class="ocr__note">{detail.ocr === "error" ? t("picker.ocr.failed") : t("picker.ocr.none")}</p>
                  <div class="ocr__actions"><Button variant="secondary" size="sm" icon="ocr" loading={ocrBusy} onclick={() => void recognize()}>{detail.ocr === "error" ? t("common.retry") : t("picker.ocr.run")}</Button></div>
                {/if}
              </div>
            {/if}
          </div>
        {:else if detail.kind === "color"}
          {@const css = colorValue(detail)}
          <div class="well well--swatch checker"><div class="swatch" style:background={css ?? "var(--color-track)"}></div></div>
          <div class="values">
            {#each [["HEX", detail.meta.color?.hex], ["RGB", detail.meta.color?.rgb], ["HSL", detail.meta.color?.hsl]] as [label, value] (label)}
              {#if value}
                <button type="button" class="chip" title={t("picker.ql.copyValue", { value })} onmousedown={(e) => e.preventDefault()} onclick={() => void copyColorValue(value ?? "")}>
                  <span class="chip__label">{label}</span><span class="chip__value">{value}</span><Ico name="copy" size={13} />
                </button>
              {/if}
            {/each}
          </div>
        {:else if detail.kind === "link"}
          {@const url = detail.meta.url ?? detail.text ?? detail.preview}
          <div class="well well--center">
            <span class="mono-badge" aria-hidden="true">{(linkHost(detail).replace(/^www\./, "")[0] ?? "?").toUpperCase()}</span>
            <span class="link__host">{linkHost(detail)}</span>
            <div class="link__url selectable"><ViewText text={url} terms={shelf.terms} /></div>
            <Button variant="secondary" size="sm" icon="external" onclick={() => id !== null && void runAction("open", [id])}>{t("picker.menu.open")}</Button>
          </div>
        {:else if detail.kind === "file"}
          <ul class="well files scroll">
            {#each detail.files ?? [] as f (f.path)}
              {@const name = baseName(f.path)}
              {@const ext = name.includes(".") ? (name.split(".").pop() ?? "").toUpperCase() : "FILE"}
              <li class="file" class:file--missing={!f.exists} title={f.exists ? f.path : t("picker.ql.missing")}>
                <span class="file__icon"><Ico name={f.exists ? fileIcon(ext) : "warning"} size={18} /></span>
                <span class="file__name">{name}</span>
                <span class="file__path selectable"><bdi>{f.path}</bdi></span>
                <span class="file__meta">
                  {#if !f.exists}<span class="missing">{t("picker.ql.missing")}</span>{:else}{sizeText(f.size) ?? ""}{/if}
                </span>
              </li>
            {/each}
          </ul>
        {:else if detail.kind === "contact"}
          <div class="well well--center">
            <span class="mono-badge mono-badge--icon" aria-hidden="true"><Ico name={detail.meta.contact === "phone" ? "phone" : "mail"} size={20} /></span>
            <span class="contact selectable"><ViewText text={detail.text ?? detail.preview} terms={shelf.terms} /></span>
          </div>
        {:else}
          {@const full = detail.text ?? detail.preview}
          <div class="well well--text scroll">
            <ViewText text={full} terms={shelf.terms} invisibles={qlPrefs.invisibles} mono={detail.kind === "code"} wrap={detail.kind !== "code"} />
          </div>
          {#if detail.meta.truncated}
            <p class="note"><Ico name="info" size={14} />{t("picker.ql.truncated")}</p>
          {:else if full.length > 400_000}
            <p class="note"><Ico name="info" size={14} />{t("picker.ql.viewCapped", { n: (400_000).toLocaleString(locale) })}</p>
          {:else if qlPrefs.invisibles && full.length > 100_000}
            <p class="note"><Ico name="info" size={14} />{t("picker.ql.invisiblesCapped", { n: (100_000).toLocaleString(locale) })}</p>
          {/if}
        {/if}
      </div>

      {#if !editing}
        <aside class="ql__info" aria-label={t("picker.ql.info")}>
          {#if !snippet && !detail && !loadError}
            {#each [0, 1, 2, 3, 4] as i (i)}
              <div class="info__row"><Skeleton width="56px" height="10px" radius={5} /><Skeleton width={`${[70, 54, 80, 62, 48][i]}%`} height="10px" radius={5} /></div>
            {/each}
          {:else}
            <dl class="info">
              {#each info as row (row.label)}
                <div class="info__row">
                  <dt>{row.label}</dt>
                  <dd>
                    <span class="info__value" title={row.title}>
                      {#if row.appIcon}
                        {#if row.icon}<img class="info__app" src={row.icon} alt="" />{:else}<Ico name="appWindow" size={14} />{/if}
                      {/if}
                      <span>{row.value}</span>
                    </span>
                    {#if row.sub}<span class="info__sub">{row.sub}</span>{/if}
                  </dd>
                </div>
              {/each}
            </dl>
          {/if}
        </aside>
      {/if}
    </div>
  </div>

  {#snippet footer()}
    <LayerFooter {hints}>
        {#snippet actions()}
          {#if editing}
            <Button variant="ghost" size="sm" onclick={cancelEdit}>{t("common.cancel")}</Button>
            <Button variant="primary" size="sm" icon="check" loading={saving} onclick={() => void save()}>{t("common.save")}</Button>
          {:else}
            <Button variant="secondary" size="sm" icon="copy" disabled={id === null} onclick={copy}>{t("common.copy")}</Button>
            <Button variant="primary" size="sm" icon="paste" disabled={id === null} onclick={() => paste(false)}>{t("common.paste")}</Button>
          {/if}
        {/snippet}
    </LayerFooter>
  {/snippet}
</LayerSurface>

<style>
  /* The body: preview + table side by side when the sheet is wide, stacked when narrow. */
  .wrap {
    container-type: inline-size;
    display: flex;
    flex: 1 1 auto;
    min-height: 0;
    margin-top: 6px;
  }
  .ql {
    display: grid;
    flex: 1 1 auto;
    grid-template-columns: minmax(0, 1fr) 216px;
    grid-template-rows: minmax(0, 1fr);
    gap: 8px 16px;
    min-height: 0;
  }
  .ql--single {
    grid-template-columns: minmax(0, 1fr);
  }
  .ql__view {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
    min-height: 0;
  }
  .ql__info {
    min-width: 0;
    min-height: 0;
    padding: 6px 4px 0 0;
    overflow-y: auto;
  }

  /* Content wells: concentric with the sheet (--layer-r-in = 24 − 8 = 16). */
  .well {
    flex: 1 1 auto;
    min-height: 0;
    border-radius: var(--layer-r-in);
    background: var(--color-track);
  }
  .scroll {
    overflow: auto;
  }
  .well--text {
    padding: 12px 14px;
  }
  .well--state,
  .well--center {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 16px;
    text-align: center;
  }
  .well--state {
    color: var(--color-ink-2);
    font-size: var(--text-secondary);
  }
  .well--state p {
    margin: 0;
  }
  .skeleton {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding-top: 2px;
  }
  .skeleton--tight {
    gap: 7px;
    padding: 4px 2px;
  }
  .label {
    flex: 0 0 auto;
    padding: 0 4px;
    color: var(--color-ink-3);
    font-size: var(--text-meta);
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  .note {
    display: flex;
    flex: 0 0 auto;
    align-items: flex-start;
    gap: 7px;
    margin: 0;
    padding: 0 4px;
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
    line-height: var(--text-secondary--line-height);
  }
  .note--error {
    color: var(--color-ink);
    font-weight: 600;
  }
  .note :global(.ico) {
    flex: 0 0 auto;
    margin-top: 2px;
  }

  /* Snippet body: placeholders as quiet washes; the result well marks the caret. */
  .body {
    margin: 0;
    color: var(--color-ink);
    font-family: var(--font-mono);
    font-size: var(--text-secondary);
    line-height: 1.65;
    white-space: pre-wrap;
    overflow-wrap: break-word;
  }
  .ph {
    border-radius: 4px;
    background: color-mix(in srgb, var(--color-ink) 14%, transparent);
    font-weight: 600;
  }
  .ph--bad {
    text-decoration: underline wavy var(--color-ink-3);
    text-underline-offset: 3px;
  }
  .well--result {
    flex: 0 1 auto;
    max-height: 40%;
    padding: 10px 14px;
    color: var(--color-ink);
    font-size: var(--text-row);
    line-height: 1.6;
    white-space: pre-wrap;
    overflow-wrap: break-word;
  }
  .caret {
    display: inline-block;
    width: 2px;
    height: 1.15em;
    margin: 0 1px;
    border-radius: 999px;
    background: var(--color-ink);
    vertical-align: text-bottom;
  }

  /* Editor: the textarea is the well; the ring shows while typing. */
  .edit {
    width: 100%;
    padding: 12px 14px;
    border: 0;
    resize: none;
    color: var(--color-ink);
    font: inherit;
    font-size: var(--text-row);
    line-height: 1.6;
    outline: var(--focus-ring);
    outline-offset: -2px;
  }
  .edit--mono {
    font-family: var(--font-mono);
    font-size: var(--text-secondary);
    white-space: pre;
  }

  /* Recognized text (images): a disclosure under the image. */
  .ocr {
    display: flex;
    flex: 0 0 auto;
    flex-direction: column;
    max-height: 46%;
    border-radius: var(--layer-r-in);
    background: var(--color-track);
  }
  .ocr__toggle {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: var(--h-md);
    padding: 0 12px;
    border: 0;
    border-radius: var(--layer-r-in);
    background: transparent;
    color: var(--color-ink-2);
    font: inherit;
    font-size: var(--text-secondary);
    text-align: left;
  }
  .ocr__toggle:hover {
    background: var(--color-hover);
  }
  .ocr__toggle:focus-visible {
    outline-offset: -2px;
  }
  .ocr__title {
    color: var(--color-ink);
    font-weight: 600;
  }
  .ocr__state {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    color: var(--color-ink-3);
    text-align: right;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ocr__chev {
    display: inline-flex;
    color: var(--color-ink-3);
  }
  .ocr__body {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-height: 0;
    padding: 0 12px 12px;
  }
  .ocr__text {
    min-height: 0;
    margin: 0;
    overflow: auto;
    color: var(--color-ink);
    font-family: var(--font-sans);
    font-size: var(--text-secondary);
    line-height: 1.55;
    white-space: pre-wrap;
    overflow-wrap: break-word;
  }
  .ocr__note {
    margin: 0;
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
  }
  .ocr__actions {
    display: flex;
  }

  /* Colour: the swatch fills its well over the checker (alpha shows). */
  .checker {
    background-color: var(--color-paper);
    background-image: conic-gradient(var(--color-checker) 25%, transparent 0 50%, var(--color-checker) 0 75%, transparent 0);
    background-size: 16px 16px;
  }
  .well--swatch {
    display: flex;
    overflow: hidden;
  }
  .swatch {
    flex: 1 1 auto;
  }
  .values {
    display: flex;
    flex: 0 0 auto;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: var(--h-md);
    padding: 0 12px;
    border: 0;
    border-radius: 999px;
    background: var(--color-track);
    color: var(--color-ink-3);
    font: inherit;
    transition: background-color var(--dur-fast) var(--ease-out-soft);
  }
  .chip:hover {
    background: var(--color-hover);
    color: var(--color-ink);
  }
  .chip:active {
    background: var(--color-selected);
  }
  .chip__label {
    font-size: var(--text-meta);
    font-weight: 650;
    letter-spacing: 0.04em;
  }
  .chip__value {
    color: var(--color-ink);
    font-family: var(--font-mono);
    font-size: var(--text-secondary);
  }

  /* Link / contact: a centred card in the well. */
  .mono-badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 48px;
    height: 48px;
    border-radius: 999px;
    background: var(--color-hover);
    color: var(--color-ink);
    font-size: 20px;
    font-weight: 650;
  }
  .link__host {
    color: var(--color-ink);
    font-size: var(--text-panel);
    font-weight: 600;
  }
  .link__url {
    max-width: 100%;
    max-height: 40%;
    overflow: auto;
    color: var(--color-ink-2);
    word-break: break-all;
  }
  .link__url :global(.text) {
    font-size: var(--text-secondary);
  }
  .contact {
    max-width: 100%;
    color: var(--color-ink);
    overflow-wrap: anywhere;
  }
  .contact :global(.text) {
    font-size: 18px;
    font-weight: 600;
    line-height: 1.4;
  }

  /* Files: rows r12 inside the r16 well with 4 px padding (concentric). */
  .files {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 4px;
    list-style: none;
  }
  .file {
    display: grid;
    grid-template-columns: 24px minmax(0, 1fr) auto;
    column-gap: 10px;
    align-items: center;
    padding: 8px 12px 8px 8px;
    border-radius: calc(var(--layer-r-in) - 4px);
    color: var(--color-ink);
  }
  .file:hover {
    background: var(--color-track);
  }
  .file__icon {
    display: inline-flex;
    grid-row: 1 / span 2;
    grid-column: 1;
    justify-content: center;
    color: var(--color-ink-2);
  }
  .file--missing .file__icon,
  .file--missing .file__name {
    color: var(--color-ink-3);
  }
  .file__name {
    overflow: hidden;
    font-size: var(--text-row);
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .file__meta {
    grid-row: 1;
    grid-column: 3;
    color: var(--color-ink-3);
    font-family: var(--font-mono);
    font-size: var(--text-meta);
    white-space: nowrap;
  }
  .file__path {
    grid-row: 2;
    grid-column: 2 / span 2;
    overflow: hidden;
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;
    text-align: left;
  }
  .missing {
    display: inline-flex;
    align-items: center;
    height: 20px;
    padding: 0 8px;
    border-radius: 999px;
    background: var(--color-hover);
    color: var(--color-ink);
    font-family: var(--font-sans);
    font-weight: 600;
  }

  /* Info table: label over value, quiet labels. */
  .info {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    row-gap: 10px;
    margin: 0;
  }
  .info__row {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }
  .info__row dt {
    color: var(--color-ink-3);
    font-size: var(--text-meta);
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  .info__row dd {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
    margin: 0;
  }
  .info__value {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    color: var(--color-ink);
    font-size: var(--text-secondary);
    overflow-wrap: anywhere;
  }
  .info__app {
    flex: 0 0 auto;
    width: 16px;
    height: 16px;
    object-fit: contain;
  }
  .info__sub {
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
  }

  /* Narrow sheet (edge panels): preview above, details below in two columns. Last in the
     sheet so it wins over the base rules. */
  @container (max-width: 540px) {
    .ql {
      grid-template-columns: minmax(0, 1fr);
      grid-template-rows: minmax(0, 1fr) fit-content(42%);
    }
    .ql--single {
      grid-template-rows: minmax(0, 1fr);
    }
    .ql__info {
      padding: 2px 4px 0;
    }
    .info {
      grid-template-columns: repeat(2, minmax(0, 1fr));
      column-gap: 12px;
    }
  }
</style>
