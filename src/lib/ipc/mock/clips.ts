// Mock of the store + paste + paste-stack commands (owner: W1-E; W0 baseline).
// Semantics follow ARCHITECTURE §4 / §6 / §9.6 closely enough to build the UI against:
// filters, substring search, keyset cursor, counts, favorites order, folders, soft delete +
// undo (2 min), clear, snippets, paste outcomes by target, the paste stack and a timer
// that simulates captures (`origin: 'capture'`).
import type {
  AppInfo,
  ClearResult,
  ClearScope,
  Clip,
  ClipDetail,
  ClipMeta,
  ClipsChanged,
  CopyRequest,
  Counts,
  DragOutcome,
  FeedQuery,
  Folder,
  Kind,
  Page,
  PasteOutcome,
  PasteRequest,
  Snippet,
  SnippetInput,
  Stack,
  StackItem,
  StorageStats,
  TextPasteRequest,
  UndoResult,
} from "../types";
import { KINDS } from "../types";
import { fail, flags, mockEmit, now, rng } from "./bus";
import { makeClip, metrics, previewOf, seed, type MockClip, type MockDb, type MockSnippet } from "./seed";
import { currentSettings } from "./settings";
import { captureFlags, mockHidePicker, mockTarget } from "./system";

export type Args = Record<string, unknown>;

// ── state ────────────────────────────────────────────────────────────────────

let instance: MockDb | null = null;
/** The mock database (seeded lazily on first use). */
export function db(): MockDb {
  instance ??= seed({ size: flags.size, empty: flags.empty });
  return instance;
}

const visible = (c: MockClip) => c.deletedAt === null;
const UNDO_TTL = 120_000;
const undo = new Map<string, { expires: number; apply: () => void }>();
let undoSeq = 0;

function undoToken(apply: () => void): string {
  const token = `u${++undoSeq}.${now().toString(36)}`;
  undo.set(token, { expires: now() + UNDO_TTL, apply });
  return token;
}

function changed(change: ClipsChanged["change"], ids?: number[], origin: ClipsChanged["origin"] = "user"): void {
  mockEmit<ClipsChanged>("clips:changed", ids ? { origin, change, ids } : { origin, change });
}

function find(id: number): MockClip {
  const c = db().clips.find((x) => x.id === id && visible(x));
  if (!c) throw fail("not_found", `clip ${id}`);
  return c;
}

export function toClip(m: MockClip): Clip {
  return {
    id: m.id,
    kind: m.kind,
    title: m.title,
    preview: m.preview,
    appId: m.appId,
    createdAt: m.createdAt,
    copiedAt: m.copiedAt,
    copyCount: m.copyCount,
    usedAt: m.usedAt,
    useCount: m.useCount,
    favorite: m.favorite,
    folderId: m.folderId,
    size: m.size,
    width: m.width,
    height: m.height,
    charCount: m.charCount,
    fileCount: m.fileCount,
    thumbUrl: m.thumbUrl,
    formats: [...m.formats],
    meta: structuredClone(m.meta),
    ocr: m.ocr,
  };
}

function toDetail(m: MockClip): ClipDetail {
  return {
    ...toClip(m),
    text: m.text,
    wordCount: m.wordCount,
    lineCount: m.lineCount,
    files: m.files ? m.files.map((f) => ({ ...f })) : null,
    imageUrl: m.imageUrl,
    ocrText: m.ocrText,
  };
}

const isTextKind = (k: Kind) => k !== "image" && k !== "file";

// ── feed ─────────────────────────────────────────────────────────────────────

type Key = number[];
/** Sort key + per-position direction (1 = asc, -1 = desc), mirroring §4.3. */
function ordering(q: FeedQuery): { key: (c: MockClip) => Key; dir: number[] } {
  if (q.filter.favorites) return { key: (c) => [c.favOrder ?? 0, c.id], dir: [1, 1] };
  if (currentSettings().picker.favoritesFirst) {
    return { key: (c) => [c.favorite ? 1 : 0, c.copiedAt, c.id], dir: [-1, -1, -1] };
  }
  return { key: (c) => [c.copiedAt, c.id], dir: [-1, -1] };
}

function compare(a: Key, b: Key, dir: number[]): number {
  for (let i = 0; i < a.length; i++) {
    if (a[i] !== b[i]) return (a[i] < b[i] ? -1 : 1) * dir[i];
  }
  return 0;
}

const encodeCursor = (k: Key) => btoa(JSON.stringify(k));
function decodeCursor(s: string): Key {
  try {
    const k = JSON.parse(atob(s));
    if (Array.isArray(k) && k.every((x) => typeof x === "number")) return k;
  } catch {
    /* fall through */
  }
  throw fail("invalid", "cursor");
}

/** Free text → terms like the store (ARCHITECTURE §4.4): whitespace splits, `"…"` keeps a
 *  phrase (an unclosed quote runs to the end), empty and repeated terms are dropped, ≤ 32. */
export function searchTermsOf(text: string): string[] {
  const out: string[] = [];
  for (const m of text.matchAll(/"([^"]*)"?|(\S+)/g)) {
    const t = (m[1] ?? m[2] ?? "").trim().toLowerCase();
    if (t && !out.includes(t)) out.push(t);
  }
  return out.slice(0, 32);
}

/** Terms of ≥ 3 characters search the full text, title and OCR (FTS trigram); shorter ones
 *  only the preview and title. */
function matchesTerm(c: MockClip, term: string): boolean {
  const short = [...term].length < 3;
  const fields = short
    ? [c.preview, c.title]
    : [c.title, c.text, c.preview, c.meta.url, c.ocrText, ...(c.files ?? []).map((f) => f.path)];
  return fields.some((f) => !!f && f.toLowerCase().includes(term));
}

function clipsList({ q }: Args): Page {
  if (flags.error) throw fail("busy", "mock-error");
  const query = q as FeedQuery;
  const f = query.filter ?? {};
  const terms = searchTermsOf(query.text ?? "");
  const exclude = (query.exclude ?? []).map((w) => w.trim().toLowerCase()).filter(Boolean);
  let items = db().clips.filter((c) => {
    if (!visible(c)) return false;
    if (f.kinds?.length && !f.kinds.includes(c.kind)) return false;
    if (f.favorites && !c.favorite) return false;
    if (f.folderId !== undefined && c.folderId !== f.folderId) return false;
    if (f.appIds?.length && (c.appId === null || !f.appIds.includes(c.appId))) return false;
    if (f.since !== undefined && c.copiedAt < f.since) return false;
    if (f.until !== undefined && c.copiedAt > f.until) return false;
    if (!terms.every((t) => matchesTerm(c, t))) return false;
    if (exclude.some((t) => matchesTerm(c, t))) return false;
    return true;
  });
  const { key, dir } = ordering(query);
  items.sort((a, b) => compare(key(a), key(b), dir));
  if (query.cursor) {
    const after = decodeCursor(query.cursor);
    items = items.filter((c) => compare(key(c), after, dir) > 0);
  }
  const limit = Math.max(1, Math.min(200, Math.floor(query.limit || 40)));
  const page = items.slice(0, limit);
  const nextCursor = items.length > limit ? encodeCursor(key(page[page.length - 1])) : null;
  return { items: page.map(toClip), nextCursor };
}

function clipsCounts(): Counts {
  if (flags.error) throw fail("busy", "mock-error");
  const d = db();
  const live = d.clips.filter(visible);
  const kinds = Object.fromEntries(KINDS.map((k) => [k, 0])) as Record<Kind, number>;
  const folders: Record<number, number> = {};
  for (const fo of d.folders) folders[fo.id] = 0;
  for (const c of live) {
    kinds[c.kind]++;
    if (c.folderId !== null && c.folderId in folders) folders[c.folderId]++;
  }
  return {
    all: live.length,
    kinds,
    favorites: live.filter((c) => c.favorite).length,
    snippets: d.snippets.length,
    folders,
  };
}

// ── mutations ────────────────────────────────────────────────────────────────

function setFavorite({ ids, value }: Args): void {
  const d = db();
  const list = (ids as number[]).map(find);
  let max = Math.max(0, ...d.clips.map((c) => c.favOrder ?? 0));
  for (const c of list) {
    if (value && !c.favorite) {
      c.favorite = true;
      c.favOrder = ++max;
    } else if (!value) {
      c.favorite = false;
      c.favOrder = null;
    }
  }
  changed("updated", list.map((c) => c.id));
}

/** Fractional position before `before` (or at the end), like the store's REAL columns. */
function between(sorted: number[], before: number | null): number {
  if (before === null) return (sorted.length ? sorted[sorted.length - 1] : 0) + 1;
  const prev = sorted.filter((p) => p < before).pop();
  return prev === undefined ? before - 1 : (prev + before) / 2;
}

function moveFavorite({ id, beforeId }: Args): void {
  const c = find(id as number);
  if (!c.favorite) throw fail("invalid", "not a favorite");
  const others = db().clips.filter((x) => x.favorite && visible(x) && x.id !== c.id);
  const before = beforeId === null ? null : (others.find((x) => x.id === beforeId)?.favOrder ?? null);
  if (beforeId !== null && before === null) throw fail("not_found", `clip ${String(beforeId)}`);
  c.favOrder = between(others.map((x) => x.favOrder ?? 0).sort((a, b) => a - b), before);
  changed("updated", [c.id]);
}

function setFolder({ ids, folderId }: Args): UndoResult {
  const target = folderId as number | null;
  if (target !== null && !db().folders.some((f) => f.id === target)) throw fail("not_found", `folder ${target}`);
  const list = (ids as number[]).map(find);
  const prev = list.map((c) => [c, c.folderId] as const);
  for (const c of list) c.folderId = target;
  changed("updated", list.map((c) => c.id));
  return {
    undoToken: undoToken(() => {
      for (const [c, f] of prev) c.folderId = f;
      changed("updated", list.map((c) => c.id));
    }),
  };
}

function rename({ id, title }: Args): Clip {
  const c = find(id as number);
  const t = typeof title === "string" ? title.trim() : "";
  if ([...t].length > 200) throw fail("invalid", "title too long");
  c.title = t || null;
  changed("updated", [c.id]);
  return toClip(c);
}

/** Tiny stand-in for capture's classifier (W1-B owns the real one). */
function classify(text: string): { kind: Kind; meta: ClipMeta } {
  const s = text.trim();
  if (/^(https?:\/\/|www\.)\S+$/i.test(s)) {
    const url = s.startsWith("www.") ? `https://${s}` : s;
    let host = "";
    try {
      host = new URL(url).host;
    } catch {
      /* keep empty */
    }
    return { kind: "link", meta: { url, host } };
  }
  if (/^#([0-9a-f]{3}|[0-9a-f]{6}|[0-9a-f]{8})$/i.test(s) || /^(rgb|hsl)a?\(.+\)$/i.test(s)) {
    return { kind: "color", meta: { color: { hex: s.startsWith("#") ? s.toUpperCase() : s, rgb: s, hsl: s } } };
  }
  if (/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(s)) return { kind: "contact", meta: { contact: "email" } };
  if (/^\+?[\d\s()-]{7,20}$/.test(s) && (s.match(/\d/g) ?? []).length >= 7) return { kind: "contact", meta: { contact: "phone" } };
  const codeSignals = [/[{};]\s*$/m, /^\s{2,}\S/m, /\b(function|const|let|fn|def|class|SELECT|import|return)\b/, /=>|::|->/];
  if (codeSignals.filter((r) => r.test(text)).length >= 2) return { kind: "code", meta: {} };
  return { kind: "text", meta: {} };
}

function applyText(c: MockClip, text: string): void {
  c.text = text;
  c.preview = previewOf(text);
  Object.assign(c, metrics(text));
  c.size = new TextEncoder().encode(text).length;
  c.formats = ["text"];
}

function duplicateOf(text: string, exceptId?: number): MockClip | undefined {
  return db().clips.find((x) => x.id !== exceptId && isTextKind(x.kind) && x.text === text);
}

function updateText({ id, text }: Args): Clip {
  const c = find(id as number);
  if (!isTextKind(c.kind)) throw fail("invalid", "not a text record");
  const t = String(text ?? "");
  if (!t) throw fail("invalid", "empty text");
  const dupe = duplicateOf(t, c.id);
  if (dupe && visible(dupe)) throw fail("invalid", "duplicate");
  applyText(c, t);
  changed("updated", [c.id]);
  return toClip(c);
}

function createText({ text, folderId }: Args): Clip {
  const t = String(text ?? "");
  if (!t) throw fail("invalid", "empty text");
  const folder = (folderId as number | null | undefined) ?? null;
  if (folder !== null && !db().folders.some((f) => f.id === folder)) throw fail("not_found", `folder ${folder}`);
  const existing = duplicateOf(t);
  if (existing) {
    existing.deletedAt = null;
    existing.copiedAt = now();
    existing.copyCount++;
    if (folder !== null) existing.folderId = folder;
    changed("updated", [existing.id]);
    return toClip(existing);
  }
  const d = db();
  const { kind, meta } = classify(t);
  const c = makeClip(d.nextId++, "text", rng(d.nextId), now(), null);
  applyText(c, t);
  c.kind = kind;
  c.meta = meta;
  c.folderId = folder;
  d.clips.unshift(c);
  changed("added", [c.id]);
  return toClip(c);
}

function deleteClips({ ids }: Args): UndoResult {
  const list = (ids as number[]).map(find);
  const at = now();
  for (const c of list) c.deletedAt = at;
  pruneStack();
  changed("deleted", list.map((c) => c.id));
  return {
    undoToken: undoToken(() => {
      for (const c of list) c.deletedAt = null;
      changed("added", list.map((c) => c.id));
    }),
  };
}

function undoOp({ token }: Args): boolean {
  const entry = undo.get(String(token));
  undo.delete(String(token));
  if (!entry || entry.expires < now()) return false;
  entry.apply();
  return true;
}

function deleteRecent({ minutes }: Args): number {
  const m = Number(minutes);
  if (!Number.isFinite(m) || m <= 0) throw fail("invalid", "minutes");
  const d = db();
  const since = now() - m * 60_000;
  const gone = d.clips.filter((c) => c.copiedAt >= since).map((c) => c.id);
  d.clips = d.clips.filter((c) => c.copiedAt < since);
  pruneStack();
  changed("deleted", gone);
  return gone.length;
}

function clear({ scope }: Args): ClearResult {
  const d = db();
  if ((scope as ClearScope) === "all") {
    d.clips = [];
    d.folders = [];
    pruneStack();
    changed("reset");
    return { undoToken: null };
  }
  const at = now();
  const list = d.clips.filter((c) => visible(c) && !c.favorite && c.folderId === null);
  for (const c of list) c.deletedAt = at;
  pruneStack();
  changed("reset");
  return {
    undoToken: undoToken(() => {
      for (const c of list) c.deletedAt = null;
      changed("reset");
    }),
  };
}

// ── folders ──────────────────────────────────────────────────────────────────

function folderOut(id: number): Folder {
  const f = db().folders.find((x) => x.id === id);
  if (!f) throw fail("not_found", `folder ${id}`);
  const count = db().clips.filter((c) => visible(c) && c.folderId === id).length;
  return { id: f.id, name: f.name, position: f.position, count };
}

function checkFolderName(name: unknown, exceptId?: number): string {
  const n = String(name ?? "").trim();
  if (!n || [...n].length > 64) throw fail("invalid", "name");
  if (db().folders.some((f) => f.id !== exceptId && f.name.toLowerCase() === n.toLowerCase())) {
    throw fail("invalid", "exists");
  }
  return n;
}

function foldersList(): Folder[] {
  if (flags.error) throw fail("busy", "mock-error");
  return [...db().folders].sort((a, b) => a.position - b.position).map((f) => folderOut(f.id));
}

function folderCreate({ name }: Args): Folder {
  const d = db();
  const n = checkFolderName(name);
  const id = Math.max(0, ...d.folders.map((f) => f.id)) + 1;
  d.folders.push({ id, name: n, position: Math.max(0, ...d.folders.map((f) => f.position)) + 1, createdAt: now() });
  changed("updated");
  return folderOut(id);
}

function folderRename({ id, name }: Args): Folder {
  const f = db().folders.find((x) => x.id === id);
  if (!f) throw fail("not_found", `folder ${String(id)}`);
  f.name = checkFolderName(name, f.id);
  changed("updated");
  return folderOut(f.id);
}

function folderMove({ id, beforeId }: Args): void {
  const d = db();
  const f = d.folders.find((x) => x.id === id);
  if (!f) throw fail("not_found", `folder ${String(id)}`);
  const others = d.folders.filter((x) => x.id !== f.id);
  const before = beforeId === null ? null : (others.find((x) => x.id === beforeId)?.position ?? null);
  if (beforeId !== null && before === null) throw fail("not_found", `folder ${String(beforeId)}`);
  f.position = between(others.map((x) => x.position).sort((a, b) => a - b), before);
  changed("updated");
}

function folderDelete({ id, withClips }: Args): UndoResult {
  const d = db();
  const f = d.folders.find((x) => x.id === id);
  if (!f) throw fail("not_found", `folder ${String(id)}`);
  const members = d.clips.filter((c) => c.folderId === f.id && visible(c));
  const at = now();
  d.folders = d.folders.filter((x) => x.id !== f.id);
  for (const c of members) {
    c.folderId = null;
    if (withClips) c.deletedAt = at;
  }
  pruneStack();
  changed("updated");
  return {
    undoToken: undoToken(() => {
      d.folders.push(f);
      for (const c of members) {
        c.folderId = f.id;
        c.deletedAt = null;
      }
      changed("updated");
    }),
  };
}

// ── snippets / apps / stats ──────────────────────────────────────────────────

const snippetOut = (s: MockSnippet): Snippet => ({ ...s });

function snippetsList({ text }: Args): Snippet[] {
  if (flags.error) throw fail("busy", "mock-error");
  const terms = searchTermsOf(String(text ?? ""));
  return db()
    .snippets.filter((s) => terms.every((t) => s.name.toLowerCase().includes(t) || s.body.toLowerCase().includes(t)))
    .sort((a, b) => a.position - b.position)
    .map(snippetOut);
}

function snippetSave({ input }: Args): Snippet {
  const d = db();
  const i = input as SnippetInput;
  const name = String(i.name ?? "").trim();
  if (!name || [...name].length > 100) throw fail("invalid", "name");
  const keyword = i.keyword?.trim() || null;
  if (keyword && d.snippets.some((s) => s.keyword === keyword && s.id !== i.id)) throw fail("invalid", "keyword exists");
  const t = now();
  if (i.id !== undefined) {
    const s = d.snippets.find((x) => x.id === i.id);
    if (!s) throw fail("not_found", `snippet ${i.id}`);
    Object.assign(s, { name, body: i.body, keyword, updatedAt: t });
    changed("updated");
    return snippetOut(s);
  }
  const s: MockSnippet = {
    id: Math.max(0, ...d.snippets.map((x) => x.id)) + 1,
    name,
    body: i.body,
    keyword,
    position: Math.max(0, ...d.snippets.map((x) => x.position)) + 1,
    createdAt: t,
    updatedAt: t,
    useCount: 0,
  };
  d.snippets.push(s);
  changed("updated");
  return snippetOut(s);
}

/** `snippet_move` (W2-B): same fractional placement as `folder_move`. */
function snippetMove({ id, beforeId }: Args): void {
  const d = db();
  const s = snippetById(id);
  if (beforeId === id) return;
  const others = d.snippets.filter((x) => x.id !== s.id);
  const before = beforeId === null ? null : (others.find((x) => x.id === beforeId)?.position ?? null);
  if (beforeId !== null && before === null) throw fail("not_found", `snippet ${String(beforeId)}`);
  s.position = between(others.map((x) => x.position).sort((a, b) => a - b), before);
  changed("updated");
}

function snippetById(id: unknown): MockSnippet {
  const s = db().snippets.find((x) => x.id === id);
  if (!s) throw fail("not_found", `snippet ${String(id)}`);
  return s;
}

function appsList(): AppInfo[] {
  if (flags.error) throw fail("busy", "mock-error");
  const d = db();
  return d.apps
    .map((a) => ({ ...a, clipCount: d.clips.filter((c) => visible(c) && c.appId === a.id).length }))
    .filter((a) => a.clipCount > 0);
}

function storageStats(): StorageStats {
  if (flags.error) throw fail("busy", "mock-error");
  const live = db().clips;
  return {
    clips: live.filter(visible).length,
    dbBytes: 512 * 1024 + live.filter((c) => c.kind !== "image").reduce((s, c) => s + c.size, 0),
    mediaBytes: live.filter((c) => c.kind === "image").reduce((s, c) => s + c.size, 0),
  };
}

// ── paste ────────────────────────────────────────────────────────────────────

function writeClipboard(text: string | null): void {
  if (!text || typeof navigator === "undefined" || !navigator.clipboard) return;
  void navigator.clipboard.writeText(text).catch(() => {});
}

function outcome(): PasteOutcome {
  const target = mockTarget();
  if (!target) return { result: "copied", reason: "no_target" };
  if (target.elevated) return { result: "copied", reason: "elevated" };
  return { result: "pasted" };
}

/** Rust's plan (ARCHITECTURE §6.6): the shelf hides first — except with `keepOpen`, and when
 *  there is no target and `closeOnCopy` is off (the shelf stays and toasts `no_target`). */
function hideForPaste(keepOpen: boolean | undefined): void {
  if (keepOpen) return;
  if (!mockTarget() && !currentSettings().picker.closeOnCopy) return;
  mockHidePicker();
}

function markUsed(list: MockClip[]): void {
  const bump = currentSettings().picker.bumpOnPaste;
  const t = now();
  for (const c of list) {
    c.usedAt = t;
    c.useCount++;
    if (bump) c.copiedAt = t;
  }
  changed("updated", list.map((c) => c.id));
}

const SEPARATORS: Record<string, string> = { newline: "\n", space: " ", tab: "\t", none: "" };

function joinedText(list: MockClip[], separator?: string): string {
  const sep = separator ?? SEPARATORS[currentSettings().picker.mergeSeparator] ?? "\n";
  return list.map((c) => c.text ?? "").join(sep);
}

function lookup(ids: number[]): MockClip[] | null {
  try {
    return ids.map(find);
  } catch {
    return null;
  }
}

function clipPaste({ req }: Args): PasteOutcome {
  const r = req as PasteRequest;
  const list = lookup(r.ids);
  if (!list || list.length === 0) return { result: "failed", reason: "not_found" };
  writeClipboard(joinedText(list, r.separator));
  markUsed(list);
  hideForPaste(r.keepOpen);
  const o = outcome();
  const images = list.filter((c) => c.kind === "image").length;
  return images > 0 && images < list.length ? { ...o, reason: o.reason ?? "partial" } : o;
}

function clipCopy({ req }: Args): PasteOutcome {
  const r = req as CopyRequest;
  const list = lookup(r.ids);
  if (!list || list.length === 0) return { result: "failed", reason: "not_found" };
  writeClipboard(joinedText(list, r.separator));
  markUsed(list);
  return { result: "copied" };
}

function textPaste({ req }: Args): PasteOutcome {
  const r = req as TextPasteRequest;
  writeClipboard(r.text);
  // W2-C: lets a browser check see the snippet caret contract (lengths only, no content).
  console.info("[mock] text_paste", { chars: r.text.length, plain: r.plain, caretBack: r.caretBack ?? 0 });
  hideForPaste(r.keepOpen);
  return outcome();
}

function openLike(kinds: Kind[]) {
  return ({ id }: Args): void => {
    const c = find(id as number);
    if (!kinds.includes(c.kind)) throw fail("invalid", `cannot open a ${c.kind} record`);
    console.info("[mock] open", c.kind, c.id);
  };
}

// ── paste stack ──────────────────────────────────────────────────────────────

let stack: StackItem[] = [];
let stackSeq = 0;

function emitStack(): Stack {
  const s = { items: stack.map((i) => ({ ...i })) };
  mockEmit<Stack>("stack:changed", s);
  return s;
}

function pruneStack(): void {
  const before = stack.length;
  stack = stack.filter((i) => db().clips.some((c) => c.id === i.clipId && visible(c)));
  if (stack.length !== before) emitStack();
}

/** Append a record (also used by the capture simulator's "collect" mode). */
export function stackPush(c: MockClip): Stack {
  stack.push({ key: `s${++stackSeq}`, clipId: c.id, kind: c.kind, preview: c.title ?? c.preview });
  return emitStack();
}

function stackKey(key: unknown): number {
  const i = stack.findIndex((s) => s.key === key);
  if (i < 0) throw fail("not_found", `stack item ${String(key)}`);
  return i;
}

function stackMove({ key, beforeKey }: Args): Stack {
  const [item] = stack.splice(stackKey(key), 1);
  if (beforeKey === null) stack.push(item);
  else stack.splice(stackKey(beforeKey), 0, item);
  return emitStack();
}

function stackPasteAll({ separator }: Args): PasteOutcome {
  pruneStack();
  if (stack.length === 0) return { result: "failed", reason: "stack_empty" };
  const list = stack.map((i) => find(i.clipId));
  writeClipboard(joinedText(list, String(separator ?? "\n")));
  markUsed(list);
  return outcome();
}

function stackPasteNext(): PasteOutcome {
  pruneStack();
  const item = stack.shift();
  if (!item) return { result: "failed", reason: "stack_empty" };
  const c = find(item.clipId);
  writeClipboard(c.text);
  markUsed([c]);
  emitStack();
  return outcome();
}

// ── capture simulator ────────────────────────────────────────────────────────

let liveStarted = false;
const liveRng = rng(7);

/** Start the simulated captures (`?mock-live=0` disables it). */
export function startLive(): void {
  if (liveStarted || !flags.live) return;
  liveStarted = true;
  const tick = () => {
    setTimeout(tick, 7_000 + liveRng() * 5_000);
    const cap = captureFlags();
    if (cap.paused) return;
    if (cap.consumeIgnoreNext()) return;
    const d = db();
    // Every fourth capture re-copies an existing record: dedup raises it to the top
    // (`change: 'updated'`), like the store does (ARCHITECTURE §4.2).
    const older = d.clips.filter(visible).slice(4, 60);
    if (older.length && liveRng() < 0.25) {
      const c = older[Math.floor(liveRng() * older.length)];
      c.copiedAt = now();
      c.copyCount++;
      d.clips = [c, ...d.clips.filter((x) => x !== c)];
      changed("updated", [c.id], "capture");
      if (cap.collectToStack) stackPush(c);
      return;
    }
    const kinds: Kind[] = ["text", "text", "link", "code", "image", "color", "contact", "file"];
    const kind = kinds[Math.floor(liveRng() * kinds.length)];
    if (!currentSettings().storage.types[kind]) return;
    const appId = d.apps.length ? d.apps[Math.floor(liveRng() * d.apps.length)].id : null;
    const c = makeClip(d.nextId++, kind, liveRng, now(), appId);
    d.clips.unshift(c);
    changed("added", [c.id], "capture");
    if (cap.collectToStack) stackPush(c);
  };
  setTimeout(tick, 9_000);
}

// ── router table ─────────────────────────────────────────────────────────────

export const clipsHandlers: Record<string, (args: Args) => unknown> = {
  clips_list: clipsList,
  clips_counts: clipsCounts,
  clip_detail: ({ id }) => toDetail(find(id as number)),
  clip_set_favorite: setFavorite,
  clip_move_favorite: moveFavorite,
  clip_set_folder: setFolder,
  clip_rename: rename,
  clip_update_text: updateText,
  clip_create_text: createText,
  clips_delete: deleteClips,
  clips_undo: undoOp,
  clips_delete_recent: deleteRecent,
  clips_clear: clear,
  folders_list: foldersList,
  folder_create: folderCreate,
  folder_rename: folderRename,
  folder_move: folderMove,
  folder_delete: folderDelete,
  snippets_list: snippetsList,
  snippet_save: snippetSave,
  snippet_delete: ({ id }) => {
    const s = snippetById(id);
    db().snippets = db().snippets.filter((x) => x !== s);
    changed("updated");
  },
  snippet_mark_used: ({ id }) => {
    snippetById(id).useCount++;
  },
  snippet_move: snippetMove,
  apps_list: appsList,
  storage_stats: storageStats,

  clip_paste: clipPaste,
  clip_copy: clipCopy,
  text_paste: textPaste,
  text_copy: ({ text }) => writeClipboard(String(text ?? "")),
  // Browsers gate clipboard reads behind a permission prompt — the mock answers "no text",
  // so the shelf falls back to the newest history record, as it would in Rust on failure.
  clipboard_text: () => null,
  clip_open: openLike(["link", "file", "image"]),
  clip_open_with: openLike(["file", "image"]),
  clip_reveal: openLike(["file", "image"]),
  clip_drag_out: ({ ids }): DragOutcome => {
    (ids as number[]).forEach(find);
    return "cancelled";
  },

  stack_get: () => {
    pruneStack();
    return { items: stack.map((i) => ({ ...i })) };
  },
  stack_add: ({ ids }) => {
    for (const c of (ids as number[]).map(find)) stackPush(c);
    return { items: stack.map((i) => ({ ...i })) };
  },
  stack_remove: ({ key }) => {
    stack.splice(stackKey(key), 1);
    return emitStack();
  },
  stack_move: stackMove,
  stack_reverse: () => {
    stack.reverse();
    return emitStack();
  },
  stack_clear: () => {
    stack = [];
    return emitStack();
  },
  stack_paste_all: stackPasteAll,
  stack_paste_next: stackPasteNext,
};
