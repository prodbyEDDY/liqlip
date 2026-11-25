// Quick Look text: search matches (the store's matching, `highlight.ts`) plus optional
// "invisible characters" — every space, tab, line break, no-break space and zero-width
// character becomes its own run so the view can mark it (the marks are CSS, the characters
// stay: copying from the view copies the real text). Pure — node-tested (tests/pickerOverlays).
import { segments } from "../highlight";

export type WsKind = "space" | "tab" | "nl" | "nbsp" | "zw";

export interface TextRun {
  text: string;
  hit: boolean;
  ws?: WsKind;
}

/** Longest text the view marks invisibles in (a run per whitespace character). */
export const INVISIBLES_LIMIT = 100_000;
/** Longest text the view renders at all (the rest is reachable through copy / paste). */
export const VIEW_LIMIT = 400_000;

const WS_RE = /\r\n|[ \t\n\r  ​-‍⁠﻿]/g;

function wsKind(ch: string): WsKind {
  if (ch === " ") return "space";
  if (ch === "\t") return "tab";
  if (ch === "\n" || ch === "\r" || ch === "\r\n") return "nl";
  if (ch === " " || ch === " ") return "nbsp";
  return "zw";
}

/** Runs of `text` with search hits and (when `invisibles`) one run per invisible character. */
export function textRuns(text: string, terms: readonly string[], invisibles: boolean): TextRun[] {
  const segs = terms.length ? segments(text, terms) : [{ text, hit: false }];
  if (!invisibles) return segs;
  const out: TextRun[] = [];
  for (const seg of segs) {
    let last = 0;
    for (const m of seg.text.matchAll(WS_RE)) {
      const at = m.index ?? 0;
      if (at > last) out.push({ text: seg.text.slice(last, at), hit: seg.hit });
      out.push({ text: m[0], hit: seg.hit, ws: wsKind(m[0]) });
      last = at + m[0].length;
    }
    if (last < seg.text.length) out.push({ text: seg.text.slice(last), hit: seg.hit });
  }
  return out;
}
