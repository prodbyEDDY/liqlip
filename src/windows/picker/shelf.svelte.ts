// The shelf session: open/close lifecycle (`picker:open` / `picker:close`), layout, settings,
// target, pills + folders, the search (chips + debounced text), counters, app icons, capture
// and stack state, live updates (`clips:changed`) and keyboard navigation. Records and
// selection live in `feed` / `selection`; actions on records in `actions.svelte.ts`.
import { tick, untrack } from "svelte";
import {
  appsList,
  captureResume,
  captureStatus,
  clipDetail,
  clipRename,
  clipsCounts,
  clipsUndo,
  folderCreate,
  folderDelete,
  folderMove,
  folderRename,
  foldersList,
  pickerHide,
  pickerMakeInteractive,
  pickerTarget,
  settingsGet,
  stackGet,
} from "$lib/ipc/commands";
import {
  onCaptureStatus,
  onClipsChanged,
  onPickerClose,
  onPickerLayout,
  onPickerOpen,
  onSettingsChanged,
  onStackChanged,
} from "$lib/ipc/events";
import { errorCode, errorMessage } from "$lib/ipc/errors";
import { isTauri } from "$lib/ipc/runtime";
import type {
  AppInfo,
  CaptureStatus,
  Clip,
  ClipDetail,
  ClipsChanged,
  Counts,
  Folder,
  PickerLayout,
  PickerOpen,
  Settings,
  Stack,
  TargetInfo,
} from "$lib/ipc/types";
import { t } from "$lib/i18n/index.svelte";
import { motionDuration } from "$lib/motion";
import { feed, PAGE, type FeedSource } from "./feed.svelte";
import { selection } from "./selection.svelte";
import { overlays } from "./overlays.svelte";
import { toasts } from "./toasts.svelte";
import { view } from "./view.svelte";
import { buildPills, cyclePill, folderIdOf, folderPillId, pillFilter, type Pill, type PillId } from "./pills";
import { extractChips, mergeFilters, parseQuery, type ParsedQuery, type QueryContext } from "./query";
import { searchTerms } from "./highlight";
import { navGrid, navigate, rangeBetween, type NavDir } from "./gridNav";

const DEFAULT_LAYOUT: PickerLayout = { position: "center", centerRow: false, width: 760, height: 640, bleed: 72 };
/** Rows stay painted through the exit animation, then are released (ARCHITECTURE §9.7). */
const RELEASE_MS = 260;
const SEARCH_DEBOUNCE_MS = 120;
/** A burst of live changes (copies, retention) becomes one refresh. */
const CHANGED_DEBOUNCE_MS = 250;
const REMEMBER_MS = 60_000;
/** Echo window for our own optimistic mutations (`clips:changed {origin:'user'}`). */
const OWN_ECHO_MS = 4_000;

export type FolderError = "exists" | "invalid" | "failed";

/** A `ClipDetail` trimmed back to the list shape. */
function toClip(d: ClipDetail): Clip {
  const { text: _t, wordCount: _w, lineCount: _l, files: _f, imageUrl: _i, ocrText: _o, ...clip } = d;
  return clip;
}

class Shelf {
  // ── session ──
  open = $state(false);
  interactive = $state(true);
  mode = $state<PickerOpen["mode"]>("summon");
  target = $state.raw<TargetInfo | null>(null);
  layout = $state.raw<PickerLayout>(DEFAULT_LAYOUT);
  settings = $state.raw<Settings | null>(null);

  // ── around the feed ──
  counts = $state.raw<Counts | null>(null);
  folders = $state.raw<Folder[]>([]);
  apps = $state.raw<ReadonlyMap<number, AppInfo>>(new Map());
  capture = $state.raw<CaptureStatus>({ pausedUntil: null, ignoreNext: false, collectToStack: false });
  stack = $state.raw<Stack>({ items: [] });

  // ── filters ──
  pill = $state<PillId>("all");
  /** Committed filter tokens shown as chips (raw text, e.g. `app:chrome`). */
  chips = $state.raw<string[]>([]);
  /** Text in the search input (as typed). */
  draft = $state("");
  /** `draft` after the 120 ms debounce — what the feed is queried with. */
  query = $state("");
  /** The clock `date:` tokens are relative to (set on open). */
  clock = $state(Date.now());

  // ── ui ──
  /** A capture arrived while scrolled down: "New items · Show". */
  pendingNew = $state(false);
  /** Card whose title is being edited inline (F2). */
  renamingId = $state<number | null>(null);
  /** Card under the pointer (its hover actions are rendered). */
  hoveredId = $state<number | null>(null);
  /** Folder pill being renamed inline (double click / context menu). */
  renamingFolder = $state<number | null>(null);
  /** Time from `picker:open` to the first painted cards (ms), for the report / diagnostics. */
  lastOpenMs = $state<number | null>(null);
  searchEl: HTMLInputElement | null = null;
  /** The panel (overlay layers position themselves relative to it: its transform makes it
   *  the containing block of fixed descendants). */
  panelEl: HTMLElement | null = null;

  #closedAt = 0;
  #lastKey = "";
  #releaseTimer: ReturnType<typeof setTimeout> | undefined;
  #searchTimer: ReturnType<typeof setTimeout> | undefined;
  #changedTimer: ReturnType<typeof setTimeout> | undefined;
  #changes: ClipsChanged[] = [];
  #own = new Map<number, number>();
  #appsAt = 0;

  // ── derived ──
  horizontal = $derived(
    this.layout.position === "top" || this.layout.position === "bottom" || (this.layout.position === "center" && this.layout.centerRow),
  );
  pills: Pill[] = $derived(buildPills(this.counts, this.folders));
  activePill: Pill | null = $derived(this.pills.find((p) => p.id === this.pill) ?? null);
  queryCtx: QueryContext = $derived({ apps: [...this.apps.values()], folders: this.folders, now: this.clock });
  parsed: ParsedQuery = $derived(parseQuery([...this.chips, this.query].join(" "), this.queryCtx));
  source: FeedSource = $derived.by(() => {
    const p = this.parsed;
    if (p.snippets || this.pill === "snippets") return { mode: "snippets", text: p.text };
    if (p.impossible) return { mode: "none" };
    const filter = mergeFilters(pillFilter(this.pill), p.filter);
    if (!filter) return { mode: "none" };
    return { mode: "clips", query: { text: p.text, exclude: p.exclude, filter, cursor: null, limit: PAGE } };
  });
  sourceKey = $derived(JSON.stringify(this.source));
  /** Terms highlighted in previews and titles. */
  terms: string[] = $derived(searchTerms(this.parsed.text));
  /** No text and no chips in the search. */
  searchEmpty = $derived(this.draft.trim() === "" && this.chips.length === 0);
  /** A search or a pill narrows the feed (for the empty-state wording). */
  filtered = $derived(!this.searchEmpty || this.parsed.text !== "");
  paused = $derived(this.capture.pausedUntil !== null);

  // ── lifecycle ──

  /** Subscribe to the backend and start the reactive wiring. Returns the disposer. */
  async start(): Promise<() => void> {
    const offs = await Promise.all([
      onPickerOpen((p) => this.#onOpen(p)),
      onPickerClose(() => this.#onClose()),
      onPickerLayout((l) => (this.layout = l)),
      onClipsChanged((c) => this.#onChanged(c)),
      onCaptureStatus((s) => (this.capture = s)),
      onStackChanged((s) => (this.stack = s)),
      onSettingsChanged((s) => (this.settings = s)),
    ]);
    const stopEffects = $effect.root(() => {
      $effect(() => {
        view.horizontal = this.horizontal;
        view.stripLead = this.layout.position === "center" ? 16 : 14;
      });
      // Typing is debounced; chips and pills apply at once.
      $effect(() => {
        const d = this.draft;
        if (d === untrack(() => this.query)) return;
        clearTimeout(this.#searchTimer);
        this.#searchTimer = setTimeout(() => (this.query = d), SEARCH_DEBOUNCE_MS);
      });
      // Any change of what the grid shows reloads it from the top.
      $effect(() => {
        if (!this.open) return;
        const key = this.sourceKey;
        untrack(() => {
          if (key === this.#lastKey) return;
          this.#lastKey = key;
          void this.#reload(false);
        });
      });
    });
    void settingsGet()
      .then((s) => (this.settings ??= s))
      .catch(() => {});
    // `picker_target` is a safe read in Rust. The browser mock answers it by "summoning" the
    // shelf (there is no hotkey in a browser tab); inside Tauri, a window that is already
    // visible at mount (summoned before the listeners were ready) opens itself here.
    void pickerTarget()
      .then((target) => {
        if (isTauri() && !this.open && document.visibilityState === "visible") {
          this.#onOpen({ mode: "summon", target, interactive: document.hasFocus() });
        }
      })
      .catch(() => {});
    return () => {
      for (const off of offs) off();
      stopEffects();
      clearTimeout(this.#releaseTimer);
      clearTimeout(this.#searchTimer);
      clearTimeout(this.#changedTimer);
    };
  }

  #onOpen(p: PickerOpen): void {
    clearTimeout(this.#releaseTimer);
    const remember = this.settings?.picker.rememberSearch ?? "60s";
    const closedFor = this.#closedAt ? Date.now() - this.#closedAt : 0;
    if (this.#closedAt && (remember === "never" || (remember === "60s" && closedFor > REMEMBER_MS))) {
      this.pill = "all";
      this.chips = [];
      this.draft = "";
    }
    clearTimeout(this.#searchTimer);
    this.query = this.draft;
    this.clock = Date.now();
    this.mode = p.mode;
    this.target = p.target;
    this.interactive = p.interactive;
    this.pendingNew = false;
    // Reopened within the exit animation with the same query: the rows are still there.
    const keep = feed.status === "ready" && this.sourceKey === this.#lastKey;
    selection.userMoved = false;
    this.open = true;
    this.#lastKey = this.sourceKey;
    const t0 = performance.now();
    void this.#reload(keep).then(async () => {
      await tick();
      requestAnimationFrame(() => {
        this.lastOpenMs = Math.round(performance.now() - t0);
        if (import.meta.env.DEV) console.info(`[picker] cards painted ${this.lastOpenMs} ms after picker:open`);
      });
    });
    void this.refreshMeta();
    void captureStatus()
      .then((s) => (this.capture = s))
      .catch(() => {});
    void stackGet()
      .then((s) => (this.stack = s))
      .catch(() => {});
    if (p.interactive) requestAnimationFrame(() => this.focusSearch(false));
  }

  #onClose(): void {
    if (!this.open) return;
    this.open = false;
    this.#closedAt = Date.now();
    overlays.closeAll();
    this.renamingId = null;
    clearTimeout(this.#releaseTimer);
    this.#releaseTimer = setTimeout(() => {
      feed.release();
      selection.reset();
      this.pendingNew = false;
      this.#lastKey = "";
      view.scrollToStart();
    }, motionDuration(RELEASE_MS));
  }

  /** Esc reached the end of the chain, or the user dismissed: close and give focus back. */
  async hide(): Promise<void> {
    try {
      await pickerHide(true);
    } catch (e) {
      toasts.error(errorMessage(e));
    }
  }

  // ── loading ──

  async #reload(keep: boolean): Promise<void> {
    const source = this.source;
    const prevId = selection.current;
    const prevIndex = feed.indexOf(prevId);
    if (!keep) {
      this.pendingNew = false;
      view.scrollToStart();
    }
    let ok: boolean;
    try {
      ok = await feed.load(source, keep);
    } catch (e) {
      toasts.error(errorMessage(e));
      return;
    }
    if (!ok) return;
    if (!keep || !selection.userMoved) {
      selection.focus(feed.idAt(0), false);
      selection.userMoved = false;
      if (!keep) selection.clearMulti();
    } else if (feed.indexOf(prevId) < 0) {
      selection.focus(feed.idAt(Math.max(0, Math.min(prevIndex, feed.count - 1))), false);
    }
    selection.prune((id) => feed.index.has(id));
    this.#ensureApps();
  }

  /** Error state → "Try again". */
  retry(): void {
    void this.#reload(false);
    void this.refreshMeta();
  }

  /** "New items · Show": back to the top with the fresh records. */
  showNew(): void {
    this.pendingNew = false;
    void this.#reload(false);
  }

  /** Counters, folders and (when stale) app icons. */
  async refreshMeta(): Promise<void> {
    try {
      const [counts, folders] = await Promise.all([clipsCounts(), foldersList()]);
      this.counts = counts;
      this.folders = folders;
    } catch {
      // The grid shows the error state for the feed itself; stale counters are harmless.
    }
    this.#ensureApps(true);
  }

  #ensureApps(force = false): void {
    const missing = feed.items.some((c) => c.appId !== null && !this.apps.has(c.appId));
    const stale = Date.now() - this.#appsAt > 5_000;
    if (!(force || missing) || !stale) return;
    this.#appsAt = Date.now();
    void appsList()
      .then((list) => (this.apps = new Map(list.map((a) => [a.id, a]))))
      .catch(() => {});
  }

  // ── live updates ──

  /** Our own mutation of these records — its `clips:changed` echo needs no refresh. */
  markOwn(ids: readonly number[]): void {
    const now = Date.now();
    for (const id of ids) this.#own.set(id, now);
    if (this.#own.size > 500) {
      for (const [id, at] of this.#own) if (now - at > OWN_ECHO_MS) this.#own.delete(id);
    }
  }

  #isOwn(c: ClipsChanged): boolean {
    const now = Date.now();
    return c.origin === "user" && !!c.ids?.length && c.ids.every((id) => now - (this.#own.get(id) ?? 0) < OWN_ECHO_MS);
  }

  #onChanged(c: ClipsChanged): void {
    this.#changes.push(c);
    clearTimeout(this.#changedTimer);
    this.#changedTimer = setTimeout(() => void this.#applyChanges(), CHANGED_DEBOUNCE_MS);
  }

  async #applyChanges(): Promise<void> {
    const batch = this.#changes;
    this.#changes = [];
    if (!this.open) return; // a closed shelf reloads on open anyway
    void this.refreshMeta();
    if (batch.some((c) => c.change === "reset")) {
      await this.#reload(false);
      return;
    }
    const foreign = batch.filter((c) => !this.#isOwn(c));
    if (foreign.length === 0 || feed.mode === "snippets") return;

    const deleted = foreign.filter((c) => c.change === "deleted").flatMap((c) => c.ids ?? []);
    if (deleted.length) this.#dropRows(deleted);

    // A capture (new copy, or a re-copy raising an old record) belongs at the top.
    const captured = foreign.some((c) => c.origin === "capture" && c.change !== "deleted");
    const added = foreign.some((c) => c.change === "added");
    if (captured || added) {
      const busy = view.scrolled || overlays.top !== null || this.renamingId !== null;
      if (!busy) await this.#reload(true);
      else if (captured) this.pendingNew = true; // own/other-window additions never show the plaque
      return;
    }
    const updated = [...new Set(foreign.filter((c) => c.change === "updated").flatMap((c) => c.ids ?? []))].filter((id) => feed.clip(id));
    if (updated.length > 20) {
      if (!view.scrolled) await this.#reload(true);
      return;
    }
    for (const id of updated) {
      clipDetail(id)
        .then((d) => feed.replace(toClip(d)))
        .catch((e) => {
          if (errorCode(e) === "not_found") this.#dropRows([id]);
        });
    }
  }

  /** Remove rows and keep the cursor on the card that took the removed one's place. */
  #dropRows(ids: readonly number[]): { clip: Clip; index: number }[] {
    const at = feed.indexOf(selection.current);
    const removed = feed.remove(ids);
    if (selection.current !== null && !feed.index.has(selection.current)) {
      selection.focus(feed.idAt(Math.max(0, Math.min(at, feed.count - 1))), false);
    }
    selection.prune((id) => feed.index.has(id));
    return removed;
  }

  /** Optimistic removal used by actions (delete, move out of the shown folder…); returns
   *  the removed rows for a rollback (`feed.restore`). */
  dropRows(ids: readonly number[]): { clip: Clip; index: number }[] {
    return this.#dropRows(ids);
  }

  // ── search ──

  /** Input text changed. Finished tokens become chips; returns the text the input should
   *  now hold (differs from `value` when a chip was taken out). */
  setDraft(value: string): string {
    const { chips, rest } = extractChips(value, this.queryCtx);
    if (chips.length) {
      this.chips = [...this.chips, ...chips];
      this.draft = rest;
      return rest;
    }
    this.draft = value;
    return value;
  }

  /** Accept an autocomplete suggestion: the word `[start, end)` becomes a chip. */
  acceptToken(insert: string, start: number, end: number): string {
    const rest = (this.draft.slice(0, start) + this.draft.slice(end)).replace(/\s{2,}/g, " ").trimStart();
    this.chips = [...this.chips, insert];
    this.draft = rest;
    return rest;
  }

  removeChip(index: number): void {
    this.chips = this.chips.filter((_, i) => i !== index);
  }

  clearSearch(): void {
    clearTimeout(this.#searchTimer);
    this.chips = [];
    this.draft = "";
    this.query = "";
  }

  focusSearch(select = true): void {
    const el = this.searchEl;
    if (!el) return;
    el.focus({ preventScroll: true });
    if (select) el.select();
  }

  /** Hover-opened shelf: the first click into the search makes it keyboard-interactive. */
  async makeInteractive(): Promise<void> {
    if (this.interactive) return;
    try {
      await pickerMakeInteractive();
      this.interactive = true;
    } catch (e) {
      toasts.error(errorMessage(e));
    }
  }

  // ── pills ──

  setPill(id: PillId): void {
    if (this.pill === id) return;
    this.pill = id;
    selection.clearMulti();
  }

  cyclePill(delta: number): void {
    this.setPill(cyclePill(this.pills, this.pill, delta));
  }

  pillAt(n: number): void {
    const p = this.pills[n - 1];
    if (p) this.setPill(p.id);
  }

  // ── navigation ──

  navigate(dir: NavDir, extend: boolean): void {
    if (feed.count === 0) return;
    const from = feed.indexOf(selection.current);
    const r = navigate(from, dir, navGrid(view.geometry, view.extent));
    this.#moveTo(r.index, extend);
    if (r.hitEnd && feed.hasMore) {
      void feed.loadMore().then((added) => {
        if (!added || dir === "end" || feed.indexOf(selection.current) !== r.index) return;
        this.#moveTo(navigate(from, dir, navGrid(view.geometry, view.extent)).index, extend);
      });
    }
  }

  #moveTo(index: number, extend: boolean): void {
    const id = feed.idAt(index);
    if (id === null) return;
    if (extend && feed.mode === "clips") {
      if (selection.anchor === null || !feed.index.has(selection.anchor)) selection.anchor = selection.current ?? id;
      const a = feed.indexOf(selection.anchor);
      selection.setMany(rangeBetween(a, index).map((i) => feed.idAt(i)!));
    } else {
      selection.clearMulti();
    }
    selection.focus(id);
    view.reveal(index);
  }

  /** Shift+click: range from the anchor (or the current card) to `id`. */
  selectRange(id: number): void {
    const anchor = selection.anchor ?? selection.current ?? id;
    const a = feed.indexOf(anchor);
    const b = feed.indexOf(id);
    if (a < 0 || b < 0) return;
    selection.anchor = anchor;
    selection.setMany(rangeBetween(a, b).map((i) => feed.idAt(i)!));
    selection.focus(id);
  }

  selectAll(): void {
    if (feed.mode !== "clips" || feed.count === 0) return;
    selection.setMany(feed.items.map((c) => c.id));
    selection.anchor = feed.idAt(0);
  }

  /** Records an action applies to: the multi-selection, else the current card. */
  targets(): number[] {
    return selection.targets((id) => feed.indexOf(id));
  }

  // ── Esc chain (SPEC §3.5): layers → selection → search → close ──

  escape(): void {
    if (overlays.top) {
      overlays.close();
      return;
    }
    if (this.renamingId !== null) {
      this.renamingId = null;
      return;
    }
    if (selection.multi) {
      selection.clearMulti();
      return;
    }
    if (!this.searchEmpty) {
      this.clearSearch();
      return;
    }
    void this.hide();
  }

  // ── rename (F2) ──

  startRename(id: number | null): void {
    if (id === null || feed.mode !== "clips" || !feed.clip(id)) return;
    selection.focus(id);
    view.reveal(feed.indexOf(id));
    this.renamingId = id;
  }

  async commitRename(id: number, title: string): Promise<void> {
    this.renamingId = null;
    const clip = feed.clip(id);
    const next = title.trim() || null;
    if (!clip || next === clip.title) return;
    this.markOwn([id]);
    feed.patch(id, { title: next });
    try {
      feed.replace(await clipRename(id, next));
    } catch (e) {
      feed.patch(id, { title: clip.title });
      toasts.error(errorMessage(e));
    }
    this.focusSearch(false);
  }

  // ── folders ──

  folderName(id: number | null): string {
    return this.folders.find((f) => f.id === id)?.name ?? "";
  }

  #folderError(e: unknown): FolderError {
    const raw = String(e);
    if (errorCode(e) === "invalid") return raw.includes("exists") ? "exists" : "invalid";
    return "failed";
  }

  /** Create a folder and switch to it; returns an error code to show inline, or null. */
  async createFolder(name: string): Promise<FolderError | null> {
    const n = name.trim();
    if (!n || [...n].length > 64) return "invalid";
    try {
      const f = await folderCreate(n);
      this.folders = [...this.folders, f];
      this.setPill(folderPillId(f.id));
      void this.refreshMeta();
      return null;
    } catch (e) {
      return this.#folderError(e);
    }
  }

  async renameFolder(id: number, name: string): Promise<FolderError | null> {
    const n = name.trim();
    if (!n || [...n].length > 64) return "invalid";
    if (n === this.folderName(id)) return null;
    try {
      const f = await folderRename(id, n);
      this.folders = this.folders.map((x) => (x.id === id ? { ...x, name: f.name } : x));
      return null;
    } catch (e) {
      return this.#folderError(e);
    }
  }

  async deleteFolder(id: number, withClips: boolean): Promise<void> {
    const name = this.folderName(id);
    const before = this.folders;
    this.folders = this.folders.filter((f) => f.id !== id);
    if (folderIdOf(this.pill) === id) this.setPill("all");
    try {
      const { undoToken } = await folderDelete(id, withClips);
      toasts.rememberUndo(undoToken);
      toasts.show(t("picker.toast.folderDeleted", { name }), {
        icon: "trash",
        action: { label: t("common.undo"), run: () => void this.undo() },
      });
    } catch (e) {
      this.folders = before;
      toasts.error(errorMessage(e));
    }
    void this.refreshMeta();
  }

  /** Reorder folders (drag on the pills); `beforeId: null` = to the end. */
  async moveFolder(id: number, beforeId: number | null): Promise<void> {
    const before = this.folders;
    const list = before.filter((f) => f.id !== id);
    const moved = before.find((f) => f.id === id);
    if (!moved) return;
    const at = beforeId === null ? list.length : list.findIndex((f) => f.id === beforeId);
    list.splice(at < 0 ? list.length : at, 0, moved);
    this.folders = list.map((f, i) => ({ ...f, position: i + 1 }));
    try {
      await folderMove(id, beforeId);
    } catch (e) {
      this.folders = before;
      toasts.error(errorMessage(e));
    }
  }

  // ── undo (Ctrl+Z / toast) ──

  async undo(): Promise<void> {
    const token = toasts.takeUndo();
    if (!token) return;
    toasts.dismiss();
    try {
      const ok = await clipsUndo(token);
      if (ok) {
        toasts.show(t("picker.toast.restored"), { icon: "undo" });
        await Promise.all([this.#reload(true), this.refreshMeta()]);
      } else {
        toasts.error(t("picker.toast.undoExpired"));
      }
    } catch (e) {
      toasts.error(errorMessage(e));
    }
  }

  // ── capture ──

  async resume(): Promise<void> {
    try {
      this.capture = await captureResume();
    } catch (e) {
      toasts.error(errorMessage(e));
    }
  }
}

export const shelf = new Shelf();
