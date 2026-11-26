// Text layout shared by the live editor, hit-testing and the exported canvas. Port of Liquid
// Tools `src/windows/capture/annotationText.ts` (word wrap, then grapheme wrap; side handles
// reflow, corners scale the font) with its tests (tests/editorText.test.mjs). Liqlip changes:
// the LT "bubble" is the "badge" style (text on a coloured plate), padding scales with the font
// (LT's fixed 2/14 px at 20 px stay exact), a layout cache keyed by (text, width, size) so a
// redraw never re-measures unchanged text (LT defect П-20), and adapters to the v1 `TextObj`.
import { TEXT_FONT, type TextObj } from "./doc";

// Shared by the live editor, selection geometry and the exported canvas.
export interface AnnotationText {
  /** "text" (plain) or "badge" */
  type: string;
  text: string;
  /** px */
  fontSize: number;
  font: string;
  x0: number;
  y0: number;
  x1: number;
  y1: number;
  textWidth?: number;
}

export const TEXT_MIN = 10;
export const TEXT_MAX = 192;
export const TEXT_LINE_HEIGHT = 1.25;
export type MeasureText = (text: string, size: number, font: string) => number;
const graphemes = new Intl.Segmenter(undefined, { granularity: "grapheme" });

/** Inner padding: plain text 0.1 em, the badge plate 0.7 × 0.45 em (2 / 14×9 px at 20 px). */
export function textPadding(type: string, fontSize = 20) {
  return type === "badge" ? { x: fontSize * 0.7, y: fontSize * 0.45 } : { x: fontSize * 0.1, y: fontSize * 0.1 };
}

export interface TextLayout {
  lines: string[];
  w: number;
  h: number;
  pad: { x: number; y: number };
}

const caches = new WeakMap<MeasureText, Map<string, TextLayout>>();
const CACHE_MAX = 256;

export function layoutText(s: AnnotationText, measure: MeasureText): TextLayout {
  let cache = caches.get(measure);
  if (!cache) caches.set(measure, (cache = new Map()));
  const key = `${s.type}|${s.fontSize}|${s.textWidth ?? ""}|${s.font}|${s.text}`;
  const hit = cache.get(key);
  if (hit) return hit;
  const out = computeLayout(s, measure);
  if (cache.size >= CACHE_MAX) cache.delete(cache.keys().next().value!);
  cache.set(key, out);
  return out;
}

function computeLayout(s: AnnotationText, measure: MeasureText): TextLayout {
  const pad = textPadding(s.type, s.fontSize);
  const limit = s.textWidth == null ? Infinity : Math.max(1, s.textWidth - pad.x * 2);
  const lines: string[] = [];
  const width = (text: string) => measure(text, s.fontSize, s.font);
  for (const paragraph of s.text.replace(/\r\n?/g, "\n").split("\n")) {
    let line = "";
    // Keep words intact when possible; long URLs and unspaced text still wrap.
    for (const token of paragraph.match(/\S+|[ \t]+/gu) ?? []) {
      if (/^[ \t]+$/.test(token)) {
        line += token;
        continue;
      }
      if (line && width(line + token) > limit) {
        lines.push(line);
        line = "";
      }
      if (width(token) <= limit) {
        line += token;
      } else {
        for (const { segment } of graphemes.segment(token)) {
          if (line && width(line + segment) > limit) {
            lines.push(line);
            line = "";
          }
          line += segment;
        }
      }
    }
    lines.push(line);
  }
  const contentWidth = Math.max(1, ...lines.map(width));
  return {
    lines,
    w: s.textWidth ?? contentWidth + pad.x * 2,
    h: lines.length * s.fontSize * TEXT_LINE_HEIGHT + pad.y * 2,
    pad,
  };
}

export function fitText<T extends AnnotationText>(s: T, measure: MeasureText): T {
  const layout = layoutText(s, measure);
  return { ...s, x1: s.x0 + layout.w, y1: s.y0 + layout.h };
}

// Handle numbering (LT): 0 TL · 2 TR · 3 R · 4 BR · 6 BL · 7 L.
// Side handles reflow a fixed-width box; corners scale text about the opposite corner.
// Always derive from the pointer-down snapshot, never the previous pointermove.
export function resizeText<T extends AnnotationText>(start: T, node: number, dx: number, dy: number, measure: MeasureText, unit = 1): T {
  const w = start.x1 - start.x0;
  const h = start.y1 - start.y0;
  const left = node === 0 || node === 6 || node === 7;
  const top = node === 0 || node === 2;
  const pad = textPadding(start.type, start.fontSize);
  if (node === 3 || node === 7) {
    const textWidth = Math.max(pad.x * 2 + start.fontSize, w + (left ? -dx : dx));
    return fitText({ ...start, textWidth, x0: left ? start.x1 - textWidth : start.x0 }, measure);
  }
  const desiredW = w + (left ? -dx : dx);
  const desiredH = h + (top ? -dy : dy);
  const scale = (desiredW * w + desiredH * h) / Math.max(1, w * w + h * h);
  const fontSize = Math.max(TEXT_MIN * unit, Math.min(TEXT_MAX * unit, Math.round(start.fontSize * scale)));
  const ratio = fontSize / start.fontSize;
  const nextPad = textPadding(start.type, fontSize);
  const resized = fitText(
    {
      ...start,
      fontSize,
      textWidth: start.textWidth == null ? undefined : nextPad.x * 2 + (w - pad.x * 2) * ratio,
    },
    measure,
  );
  const nextW = resized.x1 - resized.x0;
  const nextH = resized.y1 - resized.y0;
  const x0 = left ? start.x1 - nextW : start.x0;
  const y0 = top ? start.y1 - nextH : start.y0;
  return { ...resized, x0, y0, x1: x0 + nextW, y1: y0 + nextH };
}

/** The canvas surface `paintText` needs. */
export interface TextCtx {
  font: string;
  fillStyle: string | CanvasGradient | CanvasPattern;
  textAlign: CanvasTextAlign;
  textBaseline: CanvasTextBaseline;
  measureText(text: string): { fontBoundingBoxAscent?: number; fontBoundingBoxDescent?: number };
  fillText(text: string, x: number, y: number): void;
}

/** Filled glyphs only (never an outline), the same wrapped layout as the box. */
export function paintText(ctx: TextCtx, s: AnnotationText, color: string, measure: MeasureText, weight = 600) {
  const { lines, pad } = layoutText(s, measure);
  ctx.font = `${weight} ${s.fontSize}px ${s.font}`;
  ctx.fillStyle = color;
  ctx.textAlign = "left";
  ctx.textBaseline = "alphabetic";
  const metrics = ctx.measureText("Mg");
  const ascent = metrics.fontBoundingBoxAscent ?? s.fontSize * 0.8;
  const descent = metrics.fontBoundingBoxDescent ?? s.fontSize * 0.2;
  const lh = s.fontSize * TEXT_LINE_HEIGHT;
  let y = s.y0 + pad.y + (lh - ascent - descent) / 2 + ascent;
  for (const line of lines) {
    ctx.fillText(line, s.x0 + pad.x, y);
    y += lh;
  }
}

// ── v1 adapters ──────────────────────────────────────────────────────────────

/** A v1 text object as the ported shape (source px; `fontSize × unit`). */
export function textShape(o: TextObj, unit: number, measure: MeasureText): AnnotationText {
  return fitText(
    {
      type: o.style === "badge" ? "badge" : "text",
      text: o.text,
      fontSize: o.fontSize * unit,
      font: TEXT_FONT,
      x0: o.at[0],
      y0: o.at[1],
      x1: o.at[0],
      y1: o.at[1],
      textWidth: o.width == null ? undefined : o.width,
    },
    measure,
  );
}

/** Back to v1 after a resize. */
export function fromShape(o: TextObj, s: AnnotationText, unit: number): TextObj {
  return { ...o, at: [s.x0, s.y0], width: s.textWidth == null ? null : s.textWidth, fontSize: s.fontSize / unit };
}

/** Black or white for text on a plate of `hex` (WCAG relative luminance). */
export function contrastInk(hex: string): string {
  const v = parseInt(hex.slice(1), 16);
  const ch = (c: number) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  const L = 0.2126 * ch((v >> 16) & 255) + 0.7152 * ch((v >> 8) & 255) + 0.0722 * ch(v & 255);
  return L > 0.4 ? "#000000" : "#ffffff";
}
