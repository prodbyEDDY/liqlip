// Search string → feed query (SPEC §3.4). The backend receives free text WITHOUT tokens
// (ARCHITECTURE §9.3): this module parses the filter tokens
//   app:<app> · type:<kind>[,<kind>] (`type:shot` = `type:screenshot`) · in:<folder> · is:fav · is:snippet ·
//   date:today|yesterday|week|month · -word / -"phrase" (exclude) · "quoted phrase"
// into `FeedQuery.filter` / `exclude`, and powers the chips and the `:` autocomplete of the
// search field. Unknown keys and unknown values stay plain text (`https://x` is a word, not
// a token). Keys are Latin; values accept Russian aliases (`type:ссылки`, `date:вчера`).
// Pure — node-tested (tests/pickerQuery).
import type { Filter, Kind } from "$lib/ipc/types";

export type TokenKey = "app" | "type" | "in" | "is" | "date";
export const TOKEN_KEYS: readonly TokenKey[] = ["app", "type", "in", "is", "date"];

export type DateRange = "today" | "yesterday" | "week" | "month";
export type IsFlag = "fav" | "snippet";

export interface QueryToken {
  key: TokenKey;
  /** Canonical value: kind(s) joined by ",", `fav`/`snippet`, a date range, or the
   *  app/folder text as typed. */
  value: string;
  /** The token as typed (what a chip shows and serializes back to). */
  raw: string;
}

export interface QueryContext {
  apps: readonly { id: number; exe: string; name: string }[];
  folders: readonly { id: number; name: string }[];
  /** Unix ms — the clock `date:` is relative to. */
  now: number;
}

export interface ParsedQuery {
  /** Free text without tokens: terms joined by one space, phrases kept in quotes (the store
   *  ANDs the terms). */
  text: string;
  exclude: string[];
  filter: Filter;
  tokens: QueryToken[];
  /** `is:snippet` — list snippets instead of clips. */
  snippets: boolean;
  /** A token resolved to nothing (unknown app or folder): the result is known to be empty. */
  impossible: boolean;
}

// ── aliases ──────────────────────────────────────────────────────────────────

const KIND_ALIASES: Record<string, Kind> = {
  text: "text", texts: "text", текст: "text", тексты: "text",
  link: "link", links: "link", url: "link", ссылка: "link", ссылки: "link",
  code: "code", код: "code",
  color: "color", colors: "color", colour: "color", цвет: "color", цвета: "color",
  contact: "contact", contacts: "contact", контакт: "contact", контакты: "contact",
  image: "image", images: "image", img: "image", picture: "image", картинка: "image", картинки: "image", изображение: "image",
  file: "file", files: "file", файл: "file", файлы: "file",
  // SPEC §3.4 / §9.6: `shot` is the short synonym. `type:image` stays copied pictures only —
  // a token filters one kind; the Images pill is the one that shows both.
  screenshot: "screenshot", screenshots: "screenshot", shot: "screenshot", shots: "screenshot",
  скриншот: "screenshot", скриншоты: "screenshot", снимок: "screenshot", снимки: "screenshot",
};

const IS_ALIASES: Record<string, IsFlag> = {
  fav: "fav", favs: "fav", favorite: "fav", favorites: "fav", favourite: "fav", избранное: "fav",
  snippet: "snippet", snippets: "snippet", сниппет: "snippet", сниппеты: "snippet",
};

const DATE_ALIASES: Record<string, DateRange> = {
  today: "today", сегодня: "today",
  yesterday: "yesterday", вчера: "yesterday",
  week: "week", неделя: "week",
  month: "month", месяц: "month",
};

/** Canonical values offered by the autocomplete, per key (app/in come from the context). */
export const KIND_VALUES: readonly Kind[] = ["text", "link", "code", "color", "contact", "image", "screenshot", "file"];
export const IS_VALUES: readonly IsFlag[] = ["fav", "snippet"];
export const DATE_VALUES: readonly DateRange[] = ["today", "yesterday", "week", "month"];

// ── scanner ──────────────────────────────────────────────────────────────────

interface Word {
  raw: string;
  neg: boolean;
  /** Lower-cased key before ":" (any Latin word), or null. */
  key: string | null;
  value: string;
  quoted: boolean;
  /** Input continues with whitespace after this word (the word is finished). */
  closed: boolean;
}

const isSpace = (c: string) => /\s/.test(c);

function scan(input: string): Word[] {
  const s = input;
  const out: Word[] = [];
  let i = 0;
  while (i < s.length) {
    while (i < s.length && isSpace(s[i])) i++;
    if (i >= s.length) break;
    const start = i;
    let neg = false;
    if (s[i] === "-" && i + 1 < s.length && !isSpace(s[i + 1])) {
      neg = true;
      i++;
    }
    let key: string | null = null;
    const m = /^([A-Za-z]+):/.exec(s.slice(i));
    if (m) {
      key = m[1].toLowerCase();
      i += m[0].length;
    }
    let value = "";
    let quoted = false;
    if (s[i] === '"') {
      quoted = true;
      const close = s.indexOf('"', i + 1);
      value = s.slice(i + 1, close < 0 ? s.length : close);
      i = close < 0 ? s.length : close + 1;
    }
    while (i < s.length && !isSpace(s[i])) value += s[i++];
    out.push({ raw: s.slice(start, i), neg, key, value, quoted, closed: i < s.length });
  }
  return out;
}

// ── resolution ───────────────────────────────────────────────────────────────

const lower = (s: string) => s.toLocaleLowerCase();
const exeBase = (exe: string) => lower(exe.split(/[\\/]/).pop() ?? exe).replace(/\.exe$/, "");

/** Apps matching an `app:` value: exact exe basename / name first, else substring. */
export function resolveApps(value: string, apps: QueryContext["apps"]): number[] {
  const v = lower(value.trim());
  if (!v) return [];
  const exact = apps.filter((a) => exeBase(a.exe) === v || lower(a.name) === v);
  const hits = exact.length ? exact : apps.filter((a) => exeBase(a.exe).includes(v) || lower(a.name).includes(v));
  return [...new Set(hits.map((a) => a.id))].sort((a, b) => a - b);
}

/** Folder matching an `in:` value: exact name (case-insensitive), else the first prefix match. */
export function resolveFolder(value: string, folders: QueryContext["folders"]): number | null {
  const v = lower(value.trim());
  if (!v) return null;
  const exact = folders.find((f) => lower(f.name) === v);
  return (exact ?? folders.find((f) => lower(f.name).startsWith(v)))?.id ?? null;
}

function kindsOf(value: string): Kind[] | null {
  const parts = value.split(/[,|]/).map((p) => lower(p.trim())).filter(Boolean);
  if (parts.length === 0) return null;
  const kinds = parts.map((p) => KIND_ALIASES[p]);
  return kinds.every(Boolean) ? [...new Set(kinds)] : null;
}

/** `[since, until]` (Unix ms, inclusive) of a date range in local time. */
export function dateBounds(range: DateRange, now: number): { since: number; until?: number } {
  const day = new Date(now);
  day.setHours(0, 0, 0, 0);
  const startOfToday = day.getTime();
  switch (range) {
    case "today":
      return { since: startOfToday };
    case "yesterday": {
      const y = new Date(startOfToday);
      y.setDate(y.getDate() - 1);
      return { since: y.getTime(), until: startOfToday - 1 };
    }
    case "week":
      return { since: now - 7 * 86_400_000 };
    case "month":
      return { since: now - 30 * 86_400_000 };
  }
}

type Interpreted =
  | { type: "token"; token: QueryToken; apply: (f: Filter, state: { snippets: boolean; impossible: boolean }) => void }
  | { type: "text"; term: string }
  | { type: "exclude"; term: string }
  | { type: "pending" };

/** A word that starts like a filter token (`app:`, `type:li`) — while it is still being typed
 *  it is neither a filter nor text. */
function looksLikeToken(w: Word): boolean {
  return !w.neg && w.key !== null && (TOKEN_KEYS as readonly string[]).includes(w.key);
}

function interpret(w: Word, ctx: QueryContext): Interpreted {
  if (w.neg) return { type: "exclude", term: w.key !== null ? w.raw.slice(1) : w.value };
  if (!looksLikeToken(w)) {
    // A quoted phrase stays quoted: the store splits the text into AND-ed terms and keeps
    // `"…"` together (ARCHITECTURE §4.4).
    const term = w.key !== null ? w.raw : w.quoted ? `"${w.value}"` : w.value;
    return { type: "text", term: w.quoted && !w.value.trim() ? "" : term };
  }
  const key = w.key as TokenKey;
  const value = w.value.trim();
  const token = (canonical: string): QueryToken => ({ key, value: canonical, raw: w.raw });
  switch (key) {
    case "app":
      if (!value) break;
      return {
        type: "token",
        token: token(value),
        apply: (f, st) => {
          const ids = resolveApps(value, ctx.apps);
          if (ids.length === 0) st.impossible = true;
          f.appIds = [...new Set([...(f.appIds ?? []), ...ids])].sort((a, b) => a - b);
        },
      };
    case "type": {
      const kinds = kindsOf(value);
      if (!kinds) break;
      return {
        type: "token",
        token: token(kinds.join(",")),
        apply: (f) => {
          f.kinds = [...new Set([...(f.kinds ?? []), ...kinds])];
        },
      };
    }
    case "in":
      if (!value) break;
      return {
        type: "token",
        token: token(value),
        apply: (f, st) => {
          const id = resolveFolder(value, ctx.folders);
          if (id === null) st.impossible = true;
          else f.folderId = id;
        },
      };
    case "is": {
      const flag = IS_ALIASES[lower(value)];
      if (!flag) break;
      return {
        type: "token",
        token: token(flag),
        apply: (f, st) => {
          if (flag === "fav") f.favorites = true;
          else st.snippets = true;
        },
      };
    }
    case "date": {
      const range = DATE_ALIASES[lower(value)];
      if (!range) break;
      return {
        type: "token",
        token: token(range),
        apply: (f) => {
          const b = dateBounds(range, ctx.now);
          f.since = b.since;
          if (b.until !== undefined) f.until = b.until;
          else delete f.until;
        },
      };
    }
  }
  // Known key, unusable value: still being typed → pending; finished → plain text.
  return w.closed ? { type: "text", term: w.raw } : { type: "pending" };
}

// ── public API ───────────────────────────────────────────────────────────────

/** Parse a search string. The last, unfinished word (no trailing space) that only *looks*
 *  like a token (`type:li`) is ignored instead of being searched as text. */
export function parseQuery(input: string, ctx: QueryContext): ParsedQuery {
  const filter: Filter = {};
  const state = { snippets: false, impossible: false };
  const text: string[] = [];
  const exclude: string[] = [];
  const tokens: QueryToken[] = [];
  for (const w of scan(input)) {
    const r = interpret(w, ctx);
    if (r.type === "token") {
      tokens.push(r.token);
      r.apply(filter, state);
    } else if (r.type === "text") {
      if (r.term) text.push(r.term);
    } else if (r.type === "exclude") {
      if (r.term) exclude.push(r.term);
    }
  }
  return { text: text.join(" "), exclude, filter, tokens, snippets: state.snippets, impossible: state.impossible };
}

/** Combine the active pill's filter with the tokens' filter; `null` = no record can match
 *  (e.g. pill "Links" + `type:image`), so the shelf shows "Nothing found" without a call. */
export function mergeFilters(pill: Filter, tokens: Filter): Filter | null {
  const out: Filter = { ...pill, ...tokens };
  if (pill.kinds && tokens.kinds) {
    const kinds = pill.kinds.filter((k) => tokens.kinds!.includes(k));
    if (kinds.length === 0) return null;
    out.kinds = kinds;
  }
  if (pill.folderId !== undefined && tokens.folderId !== undefined && pill.folderId !== tokens.folderId) return null;
  if (pill.favorites || tokens.favorites) out.favorites = true;
  return out;
}

/** Serialize a token back to text (`in:"My folder"`). */
export function serializeToken(key: TokenKey, value: string): string {
  const v = value.replace(/"/g, "").trim();
  return /\s/.test(v) || v === "" ? `${key}:"${v}"` : `${key}:${v}`;
}

/** Split the text being typed into finished, valid tokens (they become chips) and the rest
 *  (stays in the input). Only words followed by whitespace are chipped, so a token is never
 *  taken away while the user is still typing it. */
export function extractChips(draft: string, ctx: QueryContext): { chips: string[]; rest: string } {
  const chips: string[] = [];
  const rest: string[] = [];
  for (const w of scan(draft)) {
    const r = interpret(w, ctx);
    if (r.type === "token" && w.closed) chips.push(w.raw);
    else rest.push(w.raw);
  }
  const trailing = draft.length > 0 && isSpace(draft[draft.length - 1]) && rest.length > 0 ? " " : "";
  return { chips, rest: rest.join(" ") + trailing };
}

/** Parse one chip (always a valid token by construction). */
export function chipToken(raw: string, ctx: QueryContext): QueryToken | null {
  return parseQuery(raw, ctx).tokens[0] ?? null;
}

export interface Suggestion {
  key: TokenKey;
  /** Canonical value (kind, flag, range) or the app/folder text to insert. */
  value: string;
  /** Text that replaces the word being typed (+ a trailing space by the caller). */
  insert: string;
  /** Display label for app/folder suggestions (others are localized by the UI). */
  label?: string;
  /** App icon URL (app suggestions). */
  icon?: string | null;
  /** The typed value is a prefix of this suggestion (safe to complete on Space). */
  prefix?: boolean;
}

export interface SuggestContext extends QueryContext {
  apps: readonly { id: number; exe: string; name: string; iconUrl?: string | null }[];
}

/** Autocomplete for the word being typed once it reads `key:` (SPEC §3.4): apps, kinds,
 *  folders, flags, dates — filtered by the typed value (prefix matches first), ≤ 8. */
export function suggest(word: string, ctx: SuggestContext, limit = 8): Suggestion[] {
  const m = /^([A-Za-z]+):(.*)$/.exec(word);
  if (!m) return [];
  const key = m[1].toLowerCase() as TokenKey;
  if (!(TOKEN_KEYS as readonly string[]).includes(key)) return [];
  const typed = lower(m[2].replace(/"/g, "").trim());
  const rank = (labels: string[]) => {
    const ls = labels.map(lower);
    if (!typed) return 1;
    if (ls.some((l) => l.startsWith(typed))) return 0;
    if (ls.some((l) => l.includes(typed))) return 1;
    return -1;
  };
  const items: { s: Suggestion; r: number }[] = [];
  const push = (s: Suggestion, labels: string[]) => {
    const r = rank(labels);
    if (r >= 0) items.push({ s, r });
  };
  switch (key) {
    case "app":
      for (const a of ctx.apps) {
        const value = exeBase(a.exe) || lower(a.name);
        push({ key, value, insert: serializeToken(key, value), label: a.name, icon: a.iconUrl ?? null }, [a.name, value]);
      }
      break;
    case "type":
      for (const k of KIND_VALUES) {
        const aliases = Object.keys(KIND_ALIASES).filter((a) => KIND_ALIASES[a] === k);
        push({ key, value: k, insert: serializeToken(key, k) }, aliases);
      }
      break;
    case "in":
      for (const f of ctx.folders) push({ key, value: f.name, insert: serializeToken(key, f.name), label: f.name }, [f.name]);
      break;
    case "is":
      for (const v of IS_VALUES) {
        const aliases = Object.keys(IS_ALIASES).filter((a) => IS_ALIASES[a] === v);
        push({ key, value: v, insert: serializeToken(key, v) }, aliases);
      }
      break;
    case "date":
      for (const v of DATE_VALUES) {
        const aliases = Object.keys(DATE_ALIASES).filter((a) => DATE_ALIASES[a] === v);
        push({ key, value: v, insert: serializeToken(key, v) }, aliases);
      }
      break;
  }
  return items
    .map((x, i) => ({ ...x, i }))
    .sort((a, b) => a.r - b.r || a.i - b.i)
    .slice(0, limit)
    .map((x) => (typed && x.r === 0 ? { ...x.s, prefix: true } : x.s));
}

/** Space after an unfinished token (`type:te `): the first suggestion when the typed value
 *  is a prefix of it — the token completes instead of turning into search text. */
export function completeToken(word: string, ctx: SuggestContext): string | null {
  if (parseQuery(word, ctx).tokens.length) return null;
  const first = suggest(word, ctx, 1)[0];
  return first?.prefix ? first.insert : null;
}

/** The word under the caret (from the previous whitespace to the caret) and where it starts. */
export function wordAt(text: string, caret: number): { word: string; start: number } {
  let start = caret;
  while (start > 0 && !isSpace(text[start - 1])) start--;
  return { word: text.slice(start, caret), start };
}
