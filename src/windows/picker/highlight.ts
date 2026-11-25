// Search-match highlighting in card previews and titles (SPEC §3.4). Matching mirrors the
// backend's: case-insensitive substrings, Cyrillic and Latin alike. Each whitespace-separated
// term (a quoted phrase is one term) is highlighted wherever it occurs. Pure — node-tested.

export interface Segment {
  text: string;
  hit: boolean;
}

const escapeRe = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/** Terms to highlight for the free text of a query (`text` from `parseQuery`). */
export function searchTerms(text: string): string[] {
  const terms: string[] = [];
  const re = /"([^"]*)"?|(\S+)/g;
  for (const m of text.matchAll(re)) {
    const term = (m[1] ?? m[2] ?? "").trim();
    if (term) terms.push(term);
  }
  // The whole text as typed is also a term (the backend matches it as one substring).
  const whole = text.trim();
  if (whole && !terms.includes(whole)) terms.unshift(whole);
  return [...new Set(terms)].sort((a, b) => b.length - a.length);
}

/** Split `text` into plain / highlighted segments for `terms` (longest match first). */
export function segments(text: string, terms: readonly string[]): Segment[] {
  if (!text || terms.length === 0) return [{ text, hit: false }];
  const re = new RegExp(terms.map(escapeRe).join("|"), "giu");
  const out: Segment[] = [];
  let last = 0;
  for (const m of text.matchAll(re)) {
    const at = m.index ?? 0;
    if (m[0].length === 0) continue;
    if (at > last) out.push({ text: text.slice(last, at), hit: false });
    out.push({ text: m[0], hit: true });
    last = at + m[0].length;
  }
  if (last < text.length) out.push({ text: text.slice(last), hit: false });
  return out.length ? out : [{ text, hit: false }];
}
