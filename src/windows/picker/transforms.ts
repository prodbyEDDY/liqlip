// Text transforms of the actions menu (SPEC §3.7 "Transform ▸"): case, lines, JSON, URL,
// Base64, HTML → plain text, slug. Pure functions — node-tested (tests/pickerTransforms).
// Every transform takes the record's full text and returns either the new text or a typed
// error the UI localizes (invalid JSON / Base64 / percent-encoding never yields "nothing").
//
// Line-based transforms keep the text's own line break (CRLF stays CRLF) and a trailing
// line break where it makes sense. JSON is re-printed token by token (no JSON.parse round
// trip): big numbers, duplicate keys and escapes survive exactly as written.

export type TransformId =
  | "upper"
  | "lower"
  | "title"
  | "sentence"
  | "trim"
  | "joinLines"
  | "dropEmpty"
  | "sortLines"
  | "dedupeLines"
  | "jsonFormat"
  | "jsonMinify"
  | "urlEncode"
  | "urlDecode"
  | "base64Encode"
  | "base64Decode"
  | "htmlToText"
  | "slug";

/** Menu order, in groups (a gap between groups). */
export const TRANSFORM_GROUPS: readonly (readonly TransformId[])[] = [
  ["upper", "lower", "title", "sentence"],
  ["trim", "joinLines", "dropEmpty", "sortLines", "dedupeLines"],
  ["jsonFormat", "jsonMinify"],
  ["urlEncode", "urlDecode"],
  ["base64Encode", "base64Decode"],
  ["htmlToText", "slug"],
];

/** Results of these are shown in a monospace preview. */
export const MONO_TRANSFORMS: ReadonlySet<TransformId> = new Set<TransformId>(["jsonFormat", "jsonMinify", "urlEncode", "base64Encode", "slug"]);

export type TransformError =
  /** Not JSON; `line`/`column` (1-based) point at the offending character. */
  | { code: "json"; line: number; column: number }
  /** Not Base64 (alphabet, padding, length). */
  | { code: "base64" }
  /** Valid Base64, but the bytes are not UTF-8 text. */
  | { code: "base64Binary" }
  /** Broken percent-encoding (decode) or a lone surrogate (encode). */
  | { code: "url" }
  /** No HTML parser in this environment. */
  | { code: "html" };

export type TransformResult = { ok: true; text: string } | { ok: false; error: TransformError };

/** The DOM subset `htmlToText` walks (a real `Node`, or a test double). */
export interface HtmlNode {
  nodeType: number;
  nodeName: string;
  nodeValue: string | null;
  childNodes: ArrayLike<HtmlNode>;
}

export interface TransformOptions {
  /** Locale for case mapping (default: the engine's). */
  locale?: string;
  /** HTML parser for `htmlToText`; default: the browser's `DOMParser` (no script runs). */
  parseHtml?: (html: string) => HtmlNode | null;
}

const ok = (text: string): TransformResult => ({ ok: true, text });
const err = (error: TransformError): TransformResult => ({ ok: false, error });

/** Apply one transform to the full text of a record. */
export function applyTransform(id: TransformId, input: string, opts: TransformOptions = {}): TransformResult {
  const locale = opts.locale;
  switch (id) {
    case "upper":
      return ok(input.toLocaleUpperCase(locale));
    case "lower":
      return ok(input.toLocaleLowerCase(locale));
    case "title":
      return ok(titleCase(input, locale));
    case "sentence":
      return ok(sentenceCase(input, locale));
    case "trim":
      return ok(trimLines(input));
    case "joinLines":
      return ok(joinLines(input));
    case "dropEmpty":
      return ok(dropEmptyLines(input));
    case "sortLines":
      return ok(sortLines(input));
    case "dedupeLines":
      return ok(dedupeLines(input));
    case "jsonFormat":
      return formatJson(input, 2);
    case "jsonMinify":
      return formatJson(input, 0);
    case "urlEncode":
      try {
        return ok(encodeURIComponent(input));
      } catch {
        return err({ code: "url" });
      }
    case "urlDecode":
      try {
        return ok(decodeURIComponent(input));
      } catch {
        return err({ code: "url" });
      }
    case "base64Encode":
      return ok(base64Encode(input));
    case "base64Decode":
      return base64Decode(input);
    case "htmlToText":
      return htmlToText(input, opts.parseHtml);
    case "slug":
      return ok(slugify(input));
  }
}

// ── lines ────────────────────────────────────────────────────────────────────

/** The text's line break: CRLF if it has one, else a lone CR (old Mac), else LF. */
export function eolOf(s: string): string {
  if (s.includes("\r\n")) return "\r\n";
  if (s.includes("\r") && !s.includes("\n")) return "\r";
  return "\n";
}

const splitLines = (s: string): string[] => s.split(/\r\n|\r|\n/);

/** Lines without the empty "line" after a trailing break, and whether there was one. */
function bodyLines(s: string): { lines: string[]; trailing: boolean } {
  const lines = splitLines(s);
  const trailing = lines.length > 1 && lines[lines.length - 1] === "";
  if (trailing) lines.pop();
  return { lines, trailing };
}

/** Trim both ends of every line, then drop blank lines at the start and the end. */
function trimLines(s: string): string {
  const lines = splitLines(s).map((l) => l.trim());
  let a = 0;
  let b = lines.length;
  while (a < b && lines[a] === "") a++;
  while (b > a && lines[b - 1] === "") b--;
  return lines.slice(a, b).join(eolOf(s));
}

/** One line: every line break (and the spaces around it, blank lines too) becomes one space. */
function joinLines(s: string): string {
  return splitLines(s)
    .map((l) => l.trim())
    .filter((l) => l !== "")
    .join(" ");
}

/** Drop lines that are empty or whitespace-only. */
function dropEmptyLines(s: string): string {
  return splitLines(s)
    .filter((l) => l.trim() !== "")
    .join(eolOf(s));
}

const collator = new Intl.Collator("ru", { numeric: true, sensitivity: "variant" });

/** Sort lines (Russian collation, numbers by value: 9 before 10); a trailing break stays. */
function sortLines(s: string): string {
  const { lines, trailing } = bodyLines(s);
  const eol = eolOf(s);
  return lines.sort((a, b) => collator.compare(a, b)).join(eol) + (trailing ? eol : "");
}

/** Keep the first occurrence of every line (exact match); a trailing break stays. */
function dedupeLines(s: string): string {
  const { lines, trailing } = bodyLines(s);
  const eol = eolOf(s);
  const seen = new Set<string>();
  const out = lines.filter((l) => (seen.has(l) ? false : (seen.add(l), true)));
  return out.join(eol) + (trailing ? eol : "");
}

// ── case ─────────────────────────────────────────────────────────────────────

/** "Каждое Слово": lower-case, then the first letter of every word up (a word starts after
 *  anything that is not a letter, digit or apostrophe: "o'neil" → "O'neil", "3d" stays). */
function titleCase(s: string, locale?: string): string {
  return s
    .toLocaleLowerCase(locale)
    .replace(/(^|[^\p{L}\p{N}'’])(\p{L})/gu, (_m, pre: string, ch: string) => pre + ch.toLocaleUpperCase(locale));
}

/** "Как в предложении": lower-case, then up the first letter of the text, of each line and
 *  after a sentence end (. ! ? … with optional closing quotes/brackets and a space). */
function sentenceCase(s: string, locale?: string): string {
  return s
    .toLocaleLowerCase(locale)
    .replace(/(^[^\p{L}\n]*|[.!?…]+["»”’')\]]*\s+[^\p{L}\n]*|\n[^\p{L}\n]*)(\p{L})/gu, (_m, pre: string, ch: string) => pre + ch.toLocaleUpperCase(locale));
}

// ── JSON ─────────────────────────────────────────────────────────────────────

class JsonSyntax extends Error {
  constructor(readonly at: number) {
    super("json");
  }
}

/** 1-based line / column of a UTF-16 offset (CRLF counts as one break). */
export function lineColumn(s: string, at: number): { line: number; column: number } {
  let line = 1;
  let start = 0;
  for (let i = 0; i < at && i < s.length; i++) {
    const c = s.charCodeAt(i);
    if (c === 10 || (c === 13 && s.charCodeAt(i + 1) !== 10)) {
      line++;
      start = i + 1;
    }
  }
  return { line, column: Math.min(at, s.length) - start + 1 };
}

const NUMBER_RE = /-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?/y;

/**
 * Validate and re-print JSON. `indent` 0 = minified, otherwise that many spaces per level.
 * Literals are copied verbatim (numbers keep their digits, strings their escapes, duplicate
 * keys stay), so formatting never changes the data. A leading BOM is ignored.
 */
function formatJson(input: string, indent: number): TransformResult {
  const s = input.charCodeAt(0) === 0xfeff ? input.slice(1) : input;
  let i = 0;
  const out: string[] = [];
  const nl = (depth: number) => (indent ? "\n" + " ".repeat(indent * depth) : "");

  const ws = () => {
    while (i < s.length) {
      const c = s.charCodeAt(i);
      if (c === 0x20 || c === 0x09 || c === 0x0a || c === 0x0d) i++;
      else break;
    }
  };

  const string = (): string => {
    const start = i;
    i++; // opening quote
    while (i < s.length) {
      const c = s.charCodeAt(i);
      if (c === 0x22) {
        i++;
        return s.slice(start, i);
      }
      if (c === 0x5c) {
        const e = s[i + 1];
        if (e === "u") {
          if (!/^[0-9a-fA-F]{4}$/.test(s.slice(i + 2, i + 6))) throw new JsonSyntax(i);
          i += 6;
        } else if (e !== undefined && '"\\/bfnrt'.includes(e)) {
          i += 2;
        } else {
          throw new JsonSyntax(i);
        }
        continue;
      }
      if (c < 0x20) throw new JsonSyntax(i); // raw control characters are not allowed
      i++;
    }
    throw new JsonSyntax(s.length);
  };

  const value = (depth: number): void => {
    ws();
    if (i >= s.length) throw new JsonSyntax(s.length);
    const c = s[i];
    if (c === "{") {
      i++;
      ws();
      if (s[i] === "}") {
        i++;
        out.push("{}");
        return;
      }
      out.push("{");
      for (;;) {
        ws();
        if (s[i] !== '"') throw new JsonSyntax(i);
        out.push(nl(depth + 1), string());
        ws();
        if (s[i] !== ":") throw new JsonSyntax(i);
        i++;
        out.push(indent ? ": " : ":");
        value(depth + 1);
        ws();
        if (s[i] === ",") {
          i++;
          out.push(",");
          continue;
        }
        if (s[i] === "}") {
          i++;
          out.push(nl(depth), "}");
          return;
        }
        throw new JsonSyntax(i);
      }
    }
    if (c === "[") {
      i++;
      ws();
      if (s[i] === "]") {
        i++;
        out.push("[]");
        return;
      }
      out.push("[");
      for (;;) {
        out.push(nl(depth + 1));
        value(depth + 1);
        ws();
        if (s[i] === ",") {
          i++;
          out.push(",");
          continue;
        }
        if (s[i] === "]") {
          i++;
          out.push(nl(depth), "]");
          return;
        }
        throw new JsonSyntax(i);
      }
    }
    if (c === '"') {
      out.push(string());
      return;
    }
    for (const word of ["true", "false", "null"]) {
      if (s.startsWith(word, i)) {
        i += word.length;
        out.push(word);
        return;
      }
    }
    NUMBER_RE.lastIndex = i;
    const m = NUMBER_RE.exec(s);
    if (m && m[0].length > 0 && m[0] !== "-") {
      i += m[0].length;
      out.push(m[0]);
      return;
    }
    throw new JsonSyntax(i);
  };

  try {
    value(0);
    ws();
    if (i < s.length) throw new JsonSyntax(i);
    return ok(out.join(""));
  } catch (e) {
    if (!(e instanceof JsonSyntax)) throw e;
    const at = lineColumn(s, e.at);
    return err({ code: "json", line: at.line, column: at.column });
  }
}

// ── Base64 (UTF-8) ───────────────────────────────────────────────────────────

function base64Encode(s: string): string {
  const bytes = new TextEncoder().encode(s);
  let bin = "";
  for (let i = 0; i < bytes.length; i += 0x8000) bin += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(bin);
}

/** Decode Base64 (standard or URL-safe, whitespace and missing padding tolerated, an optional
 *  `data:…;base64,` prefix) to UTF-8 text; binary data is an error, not mojibake. */
function base64Decode(input: string): TransformResult {
  let s = input.trim().replace(/^data:[^,]*;base64,/i, "");
  s = s.replace(/\s+/g, "").replace(/-/g, "+").replace(/_/g, "/");
  if (!/^[A-Za-z0-9+/]*={0,2}$/.test(s)) return err({ code: "base64" });
  const body = s.replace(/=+$/, "");
  if (body.length % 4 === 1) return err({ code: "base64" });
  if (s.length !== body.length && s.length % 4 !== 0) return err({ code: "base64" });
  let bin: string;
  try {
    bin = atob(body + "=".repeat((4 - (body.length % 4)) % 4));
  } catch {
    return err({ code: "base64" });
  }
  const bytes = Uint8Array.from(bin, (ch) => ch.charCodeAt(0));
  try {
    return ok(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
  } catch {
    return err({ code: "base64Binary" });
  }
}

// ── HTML → plain text ────────────────────────────────────────────────────────

const SKIP = new Set(["SCRIPT", "STYLE", "NOSCRIPT", "TEMPLATE", "HEAD", "TITLE", "SVG", "IFRAME", "OBJECT", "CANVAS"]);
const PARAGRAPH = new Set(["P", "H1", "H2", "H3", "H4", "H5", "H6", "BLOCKQUOTE", "PRE", "TABLE", "UL", "OL", "DL", "FIGURE", "HR"]);
const BLOCK = new Set([
  "DIV",
  "SECTION",
  "ARTICLE",
  "HEADER",
  "FOOTER",
  "NAV",
  "ASIDE",
  "MAIN",
  "ADDRESS",
  "FORM",
  "FIELDSET",
  "DETAILS",
  "SUMMARY",
  "DT",
  "DD",
  "FIGCAPTION",
  "CAPTION",
  "TR",
  "LI",
]);

function defaultParser(html: string): HtmlNode | null {
  if (typeof DOMParser === "undefined") return null;
  // A DOMParser document has no browsing context: scripts don't run, images don't load.
  const doc = new DOMParser().parseFromString(html, "text/html");
  return doc.body as unknown as HtmlNode;
}

/**
 * HTML source → readable text: tags dropped, entities decoded (by the parser), whitespace
 * collapsed like a browser renders it (except in <pre>), blocks and <br> become line breaks,
 * paragraphs and headings a blank line, list items get "• " / "1. ", table cells a tab.
 */
export function htmlToText(html: string, parse: ((html: string) => HtmlNode | null) | undefined = defaultParser): TransformResult {
  const root = parse(html);
  if (!root) return err({ code: "html" });

  // Output as chunks; a pending break is emitted lazily (no breaks at the very start/end,
  // the larger of two adjacent breaks wins).
  let out = "";
  let pending = 0; // 0 none · 1 line break · 2 blank line
  let atLineStart = true;

  const brk = (n: number) => {
    if (out.length > 0) pending = Math.max(pending, n);
  };
  const flush = () => {
    if (!pending) return;
    out = out.replace(/[ \t]+$/, "") + "\n".repeat(pending);
    pending = 0;
    atLineStart = true;
  };
  const write = (text: string, pre = false) => {
    if (!text) return;
    flush();
    // Collapsed whitespace never doubles across inline boundaries or starts a line.
    if (!pre && (atLineStart || /[ \t]$/.test(out))) text = text.replace(/^ +/, "");
    if (!text) return;
    out += text;
    atLineStart = text.endsWith("\n");
  };

  const walk = (node: HtmlNode, pre: boolean, list: { ordered: boolean; n: number } | null) => {
    if (node.nodeType === 3) {
      const v = node.nodeValue ?? "";
      if (pre) write(v, true);
      else write(v.replace(/[\t\n\f\r ]+/g, " "));
      return;
    }
    if (node.nodeType !== 1 && node.nodeType !== 9 && node.nodeType !== 11) return;
    const name = node.nodeName.toUpperCase();
    if (SKIP.has(name)) return;
    if (name === "BR") {
      flush();
      out = out.replace(/[ \t]+$/, "") + "\n";
      atLineStart = true;
      return;
    }
    const paragraph = PARAGRAPH.has(name);
    const block = paragraph || BLOCK.has(name);
    if (block) brk(paragraph ? 2 : 1);
    if (name === "LI" && list) {
      list.n++;
      write(list.ordered ? `${list.n}. ` : "• ");
    }
    if ((name === "TD" || name === "TH") && !atLineStart && !pending) write("\t");
    const childList = name === "UL" || name === "OL" ? { ordered: name === "OL", n: 0 } : list;
    const kids = node.childNodes;
    for (let i = 0; i < kids.length; i++) walk(kids[i], pre || name === "PRE" || name === "TEXTAREA", childList);
    if (block) brk(paragraph ? 2 : 1);
  };

  walk(root, false, null);
  const text = out
    .split("\n")
    .map((l) => l.replace(/[ \t]+$/, ""))
    .join("\n")
    .replace(/\n{3,}/g, "\n\n")
    .replace(/^\n+|\n+$/g, "");
  return ok(text);
}

// ── slug ─────────────────────────────────────────────────────────────────────

/** Cyrillic → Latin for slugs (BGN-style, readable: щ → shch, х → kh, ю → yu). */
const TRANSLIT: Record<string, string> = {
  а: "a",
  б: "b",
  в: "v",
  г: "g",
  д: "d",
  е: "e",
  ё: "yo",
  ж: "zh",
  з: "z",
  и: "i",
  й: "y",
  к: "k",
  л: "l",
  м: "m",
  н: "n",
  о: "o",
  п: "p",
  р: "r",
  с: "s",
  т: "t",
  у: "u",
  ф: "f",
  х: "kh",
  ц: "ts",
  ч: "ch",
  ш: "sh",
  щ: "shch",
  ъ: "",
  ы: "y",
  ь: "",
  э: "e",
  ю: "yu",
  я: "ya",
  // Ukrainian / Belarusian letters
  і: "i",
  ї: "yi",
  є: "ye",
  ґ: "g",
  ў: "u",
};

/** "Привет, Мир! Café 2024" → "privet-mir-cafe-2024": lower-case, Cyrillic transliterated,
 *  accents dropped, every other run of characters becomes one "-". */
export function slugify(s: string): string {
  const latin = [...s.toLowerCase()].map((ch) => TRANSLIT[ch] ?? ch).join("");
  return latin
    .normalize("NFKD")
    .replace(/\p{M}+/gu, "")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}
