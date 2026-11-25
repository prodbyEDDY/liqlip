// Snippet placeholder expansion (SPEC §3.10) — ONE shared pure implementation: the settings
// editor uses it for the live preview, the shelf (W2-C) when it pastes a snippet
// (`text_paste { text, caretBack }`). No Svelte, no IPC, no globals: the caller passes the
// time, the clipboard text, a uuid generator and the locale.
//
// Syntax
//   {clipboard}                         current clipboard text
//   {date} {time} {datetime} {weekday}  now, with optional attributes
//       format="yyyy-MM-dd"             custom format (tokens below; 'quoted' text is literal)
//       offset="+2d"                    shift: [+-]N with m h d w M y (minutes … years);
//                                       parts chain: "+1d-2h", "-1w 3d" (unsigned part
//                                       inherits the previous sign)
//   {uuid}                              random UUID (from the caller's generator)
//   {cursor}                            caret position after the paste (first one only;
//                                       extra ones are dropped and reported)
//   {name | uppercase | lowercase | trim}   modifiers, applied left to right
//   {{ and }}                           literal { and }
//
// A placeholder starts with `{` immediately followed by a letter and ends at the next `}`
// on the same line (quotes respected). Anything else stays literal text, so code with
// braces (`{ return x; }`, JSON) is untouched. A placeholder with a problem (unknown name,
// bad attribute/modifier/format/offset) is pasted as written and reported by `parseSnippet`
// so the editor can highlight it.
//
// Format tokens: yyyy yy · MMMM (month name; genitive in Russian when a day token is
// present: "26 ноября") MMM MM M · dddd (weekday) ddd dd d · HH H hh h · mm m · ss s · tt.

export const PLACEHOLDER_NAMES = ["clipboard", "date", "time", "datetime", "weekday", "uuid", "cursor"] as const;
export type PlaceholderName = (typeof PLACEHOLDER_NAMES)[number];

export const MODIFIERS = ["uppercase", "lowercase", "trim"] as const;
export type Modifier = (typeof MODIFIERS)[number];

/** Why a placeholder is not expanded. `cursor` = a second `{cursor}` (dropped). */
export type PlaceholderIssue = "unknown" | "syntax" | "attribute" | "modifier" | "format" | "offset" | "cursor";

export interface TextToken {
  kind: "text";
  text: string;
  start: number;
  end: number;
}

/** `{{` / `}}` — a literal brace. */
export interface EscapeToken {
  kind: "escape";
  text: "{" | "}";
  start: number;
  end: number;
}

export interface PlaceholderToken {
  kind: "placeholder";
  /** Source text including the braces. */
  raw: string;
  start: number;
  end: number;
  /** Lower-cased name as written (may be unknown). */
  name: string;
  attrs: Record<string, string>;
  /** Lower-cased modifier names as written (may be unknown). */
  modifiers: string[];
  issue: PlaceholderIssue | null;
}

export type SnippetToken = TextToken | EscapeToken | PlaceholderToken;

export interface ExpandContext {
  /** "Now" for {date}/{time}/{datetime}/{weekday}. */
  now: Date | number;
  /** Current clipboard text for {clipboard}. */
  clipboard: string;
  /** Called once per {uuid}. */
  uuid: () => string;
  /** BCP-47 locale for names and default formats (default "en"). */
  locale?: string;
}

export interface ExpandResult {
  text: string;
  /** ← presses after the paste to land on {cursor} (`caretSteps` of the text after it; 0 = none). */
  caretBack: number;
  /** UTF-16 offset of {cursor} in `text` (for previews), `null` when there is none. */
  caretIndex: number | null;
}

const NAME_RE = /^[A-Za-z][A-Za-z0-9_-]*/;
const DATE_NAMES = new Set(["date", "time", "datetime", "weekday"]);

// ── parsing ──────────────────────────────────────────────────────────────────

/** Tokenize a snippet body (for highlighting and diagnostics). Offsets are UTF-16. */
export function parseSnippet(body: string): SnippetToken[] {
  const out: SnippetToken[] = [];
  let text = "";
  let textStart = 0;
  const flush = (at: number) => {
    if (text) out.push({ kind: "text", text, start: textStart, end: at });
    text = "";
  };
  let cursorSeen = false;
  let i = 0;
  while (i < body.length) {
    const ch = body[i];
    if ((ch === "{" || ch === "}") && body[i + 1] === ch) {
      flush(i);
      out.push({ kind: "escape", text: ch, start: i, end: i + 2 });
      i += 2;
      textStart = i;
      continue;
    }
    if (ch === "{") {
      const ph = readPlaceholder(body, i);
      if (ph) {
        if (ph.name === "cursor" && ph.issue === null) {
          if (cursorSeen) ph.issue = "cursor";
          cursorSeen = true;
        }
        flush(i);
        out.push(ph);
        i = ph.end;
        textStart = i;
        continue;
      }
    }
    if (!text) textStart = i;
    text += ch;
    i++;
  }
  flush(i);
  return out;
}

/** Read `{name …}` at `start`, or `null` when it is not placeholder-shaped (literal text). */
function readPlaceholder(body: string, start: number): PlaceholderToken | null {
  const nameMatch = NAME_RE.exec(body.slice(start + 1, start + 65));
  if (!nameMatch) return null;
  let i = start + 1 + nameMatch[0].length;
  const after = body[i];
  if (after !== "}" && after !== "|" && after !== " " && after !== "\t") return null;
  // Find the closing brace on this line, skipping quoted values.
  let quote: string | null = null;
  let end = -1;
  for (let j = i; j < body.length; j++) {
    const c = body[j];
    if (c === "\n" || c === "\r") break;
    if (quote) {
      if (c === quote) quote = null;
    } else if (c === '"' || c === "'") {
      quote = c;
    } else if (c === "}") {
      end = j;
      break;
    }
  }
  if (end < 0) return null;

  const token: PlaceholderToken = {
    kind: "placeholder",
    raw: body.slice(start, end + 1),
    start,
    end: end + 1,
    name: nameMatch[0].toLowerCase(),
    attrs: {},
    modifiers: [],
    issue: null,
  };

  // Attributes: (ws name = "value")*
  const inner = body.slice(i, end);
  let k = 0;
  const ws = () => {
    while (k < inner.length && (inner[k] === " " || inner[k] === "\t")) k++;
  };
  let syntaxOk = true;
  for (;;) {
    const before = k;
    ws();
    if (k >= inner.length || inner[k] === "|") break;
    if (k === before) {
      syntaxOk = false; // attribute not separated by whitespace
      break;
    }
    const m = /^([A-Za-z][A-Za-z0-9_-]*)\s*=\s*(["'])/.exec(inner.slice(k));
    if (!m) {
      syntaxOk = false;
      break;
    }
    const q = m[2];
    const valueStart = k + m[0].length;
    const close = inner.indexOf(q, valueStart);
    if (close < 0) {
      syntaxOk = false;
      break;
    }
    const key = m[1].toLowerCase();
    if (key in token.attrs) syntaxOk = false;
    token.attrs[key] = inner.slice(valueStart, close);
    k = close + 1;
  }
  // Modifiers: (| name)*
  while (syntaxOk && k < inner.length) {
    ws();
    if (k >= inner.length) break;
    if (inner[k] !== "|") {
      syntaxOk = false;
      break;
    }
    k++;
    ws();
    const m = NAME_RE.exec(inner.slice(k));
    if (!m) {
      syntaxOk = false;
      break;
    }
    token.modifiers.push(m[0].toLowerCase());
    k += m[0].length;
  }

  token.issue = syntaxOk ? validate(token) : "syntax";
  return token;
}

function validate(t: PlaceholderToken): PlaceholderIssue | null {
  if (!(PLACEHOLDER_NAMES as readonly string[]).includes(t.name)) return "unknown";
  const keys = Object.keys(t.attrs);
  if (DATE_NAMES.has(t.name)) {
    if (keys.some((k) => k !== "format" && k !== "offset")) return "attribute";
    if (t.attrs.format !== undefined && !formatIsValid(t.attrs.format)) return "format";
    if (t.attrs.offset !== undefined && parseOffset(t.attrs.offset) === null) return "offset";
  } else if (keys.length > 0) {
    return "attribute";
  }
  if (t.name === "cursor" && t.modifiers.length > 0) return "modifier";
  if (t.modifiers.some((m) => !(MODIFIERS as readonly string[]).includes(m))) return "modifier";
  return null;
}

// ── expansion ────────────────────────────────────────────────────────────────

/** Expand a snippet body. Pure: same inputs → same output (the uuid generator aside). */
export function expandSnippet(body: string, ctx: ExpandContext): ExpandResult {
  const locale = ctx.locale ?? "en";
  const now = new Date(typeof ctx.now === "number" ? ctx.now : ctx.now.getTime());
  let text = "";
  let caretIndex: number | null = null;
  for (const tok of parseSnippet(body)) {
    if (tok.kind !== "placeholder") {
      text += tok.text;
      continue;
    }
    if (tok.issue === "cursor") continue; // extra {cursor}: dropped
    if (tok.issue) {
      text += tok.raw;
      continue;
    }
    if (tok.name === "cursor") {
      caretIndex = text.length;
      continue;
    }
    let value = valueOf(tok, now, ctx, locale);
    for (const m of tok.modifiers) value = applyModifier(value, m as Modifier, locale);
    text += value;
  }
  const caretBack = caretIndex === null ? 0 : caretSteps(text.slice(caretIndex));
  return { text, caretBack, caretIndex };
}

function valueOf(tok: PlaceholderToken, now: Date, ctx: ExpandContext, locale: string): string {
  switch (tok.name as PlaceholderName) {
    case "clipboard":
      return ctx.clipboard;
    case "uuid":
      return ctx.uuid();
    case "date":
    case "time":
    case "datetime":
    case "weekday": {
      const at = tok.attrs.offset !== undefined ? (applyOffset(now, tok.attrs.offset) ?? now) : now;
      return tok.attrs.format !== undefined
        ? formatDate(at, tok.attrs.format, locale)
        : defaultFormat(tok.name as "date" | "time" | "datetime" | "weekday", at, locale);
    }
    default:
      return tok.raw;
  }
}

function applyModifier(value: string, m: Modifier, locale: string): string {
  switch (m) {
    case "uppercase":
      return value.toLocaleUpperCase(locale);
    case "lowercase":
      return value.toLocaleLowerCase(locale);
    case "trim":
      return value.trim();
  }
}

function defaultFormat(name: "date" | "time" | "datetime" | "weekday", d: Date, locale: string): string {
  const date: Intl.DateTimeFormatOptions = { year: "numeric", month: "2-digit", day: "2-digit" };
  const time: Intl.DateTimeFormatOptions = { hour: "2-digit", minute: "2-digit" };
  const opts =
    name === "date" ? date : name === "time" ? time : name === "datetime" ? { ...date, ...time } : { weekday: "long" as const };
  return new Intl.DateTimeFormat(locale, opts).format(d);
}

// ── offsets ──────────────────────────────────────────────────────────────────

export type OffsetUnit = "m" | "h" | "d" | "w" | "M" | "y";
export interface OffsetPart {
  amount: number;
  unit: OffsetUnit;
}

/** Parse "+2d", "-1w", "+1d-2h", "1M 2d" → signed parts, or `null` when invalid. */
export function parseOffset(raw: string): OffsetPart[] | null {
  const s = raw.trim();
  if (!s) return null;
  const re = /\s*([+-])?\s*(\d{1,6})\s*([mhdwMy])\s*/y;
  const parts: OffsetPart[] = [];
  let sign = 1;
  let at = 0;
  while (at < s.length) {
    re.lastIndex = at;
    const m = re.exec(s);
    if (!m) return null;
    if (m[1]) sign = m[1] === "-" ? -1 : 1;
    parts.push({ amount: sign * Number(m[2]), unit: m[3] as OffsetUnit });
    at = re.lastIndex;
  }
  return parts;
}

/** Shift a date by an offset string (local calendar arithmetic; month ends clamp:
 *  Jan 31 + 1M = Feb 28/29). `null` when the offset is invalid. */
export function applyOffset(date: Date, raw: string): Date | null {
  const parts = parseOffset(raw);
  if (!parts) return null;
  let d = new Date(date.getTime());
  for (const { amount, unit } of parts) {
    switch (unit) {
      case "m":
        d.setMinutes(d.getMinutes() + amount);
        break;
      case "h":
        d.setHours(d.getHours() + amount);
        break;
      case "d":
        d.setDate(d.getDate() + amount);
        break;
      case "w":
        d.setDate(d.getDate() + amount * 7);
        break;
      case "M":
        d = addMonths(d, amount);
        break;
      case "y":
        d = addMonths(d, amount * 12);
        break;
    }
  }
  return d;
}

function addMonths(d: Date, n: number): Date {
  const day = d.getDate();
  const r = new Date(d.getTime());
  r.setDate(1);
  r.setMonth(r.getMonth() + n);
  const last = new Date(r.getFullYear(), r.getMonth() + 1, 0).getDate();
  r.setDate(Math.min(day, last));
  return r;
}

// ── custom date formats ──────────────────────────────────────────────────────

const FORMAT_TOKENS = ["yyyy", "yy", "MMMM", "MMM", "MM", "M", "dddd", "ddd", "dd", "d", "HH", "H", "hh", "h", "mm", "m", "ss", "s", "tt"];

type FormatPiece = { lit: string } | { tok: string };

/** Split a format into tokens and literals; `null` on an unclosed 'quote'. */
function lexFormat(format: string): FormatPiece[] | null {
  const out: FormatPiece[] = [];
  let i = 0;
  while (i < format.length) {
    const c = format[i];
    if (c === "'") {
      if (format[i + 1] === "'") {
        out.push({ lit: "'" });
        i += 2;
        continue;
      }
      const close = format.indexOf("'", i + 1);
      if (close < 0) return null;
      out.push({ lit: format.slice(i + 1, close) });
      i = close + 1;
      continue;
    }
    const tok = FORMAT_TOKENS.find((t) => format.startsWith(t, i));
    if (tok) {
      out.push({ tok });
      i += tok.length;
    } else {
      out.push({ lit: c });
      i++;
    }
  }
  return out;
}

function formatIsValid(format: string): boolean {
  return lexFormat(format) !== null;
}

const pad = (n: number, w = 2) => String(n).padStart(w, "0");

function part(d: Date, locale: string, opts: Intl.DateTimeFormatOptions, type: Intl.DateTimeFormatPartTypes): string {
  return new Intl.DateTimeFormat(locale, opts).formatToParts(d).find((p) => p.type === type)?.value ?? "";
}

/** Format a date with the custom tokens (see the header). Invalid formats render as written. */
export function formatDate(d: Date, format: string, locale = "en"): string {
  const pieces = lexFormat(format);
  if (!pieces) return format;
  // Month names next to a day number take the genitive form where the language has one.
  const withDay = pieces.some((p) => "tok" in p && (p.tok === "d" || p.tok === "dd"));
  let out = "";
  for (const p of pieces) {
    if ("lit" in p) {
      out += p.lit;
      continue;
    }
    const h = d.getHours();
    switch (p.tok) {
      case "yyyy":
        out += pad(d.getFullYear(), 4);
        break;
      case "yy":
        out += pad(d.getFullYear() % 100);
        break;
      case "MMMM":
        out += withDay
          ? part(d, locale, { day: "numeric", month: "long" }, "month")
          : part(d, locale, { month: "long" }, "month");
        break;
      case "MMM":
        out += withDay
          ? part(d, locale, { day: "numeric", month: "short" }, "month")
          : part(d, locale, { month: "short" }, "month");
        break;
      case "MM":
        out += pad(d.getMonth() + 1);
        break;
      case "M":
        out += String(d.getMonth() + 1);
        break;
      case "dddd":
        out += part(d, locale, { weekday: "long" }, "weekday");
        break;
      case "ddd":
        out += part(d, locale, { weekday: "short" }, "weekday");
        break;
      case "dd":
        out += pad(d.getDate());
        break;
      case "d":
        out += String(d.getDate());
        break;
      case "HH":
        out += pad(h);
        break;
      case "H":
        out += String(h);
        break;
      case "hh":
        out += pad(h % 12 || 12);
        break;
      case "h":
        out += String(h % 12 || 12);
        break;
      case "mm":
        out += pad(d.getMinutes());
        break;
      case "m":
        out += String(d.getMinutes());
        break;
      case "ss":
        out += pad(d.getSeconds());
        break;
      case "s":
        out += String(d.getSeconds());
        break;
      case "tt":
        out += part(d, locale, { hour: "numeric", hour12: true }, "dayPeriod") || (h < 12 ? "AM" : "PM");
        break;
    }
  }
  return out;
}

// ── caret steps ──────────────────────────────────────────────────────────────

/** ← presses needed to walk back over `s` — the paste contract (W1-C, `text_paste.caretBack`):
 *  one per Unicode code point (a surrogate pair is one press), and a CRLF line break is one
 *  press, not two. */
export function caretSteps(s: string): number {
  let n = 0;
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    if (c === 0x0d && s.charCodeAt(i + 1) === 0x0a) i++; // CRLF
    else if (c >= 0xd800 && c <= 0xdbff && (s.charCodeAt(i + 1) & 0xfc00) === 0xdc00) i++; // surrogate pair
    n++;
  }
  return n;
}
