// Annotation document v1 (ARCHITECTURE §18.6): types, constants, validation, migrations.
// Coordinates are pixels of the SOURCE picture; thickness = TABLE[size] * doc.unit. The stored
// shape is `AnnotDoc` from the IPC contract (`types.ts`, TS-only; Rust keeps it opaque). The
// editor adds one optional field (`round` on rect/ellipse — rounded rectangle corners), which
// is additive: older readers ignore it, and a document without it draws square corners.
// Pure, node-tested (tests/editorDoc.test.mjs).
import type { AnnotDoc, AnnotObject, Pt, Rect, Size } from "$lib/ipc/types";

export type { AnnotDoc, AnnotObject, Pt, Rect, Size };

export type ArrowObj = Extract<AnnotObject, { kind: "arrow" }>;
export type LineObj = Extract<AnnotObject, { kind: "line" }>;
export type BoxObj = Extract<AnnotObject, { kind: "rect" | "ellipse" }> & { round?: boolean };
export type StrokeObj = Extract<AnnotObject, { kind: "pen" | "marker" }>;
export type TextObj = Extract<AnnotObject, { kind: "text" }>;
export type CounterObj = Extract<AnnotObject, { kind: "counter" }>;
export type RedactObj = Extract<AnnotObject, { kind: "redact" }>;
export type SpotObj = Extract<AnnotObject, { kind: "spotlight" }>;
export type ImageObj = Extract<AnnotObject, { kind: "image" }>;
/** An annotation object as the editor holds it (`AnnotObject` + the optional `round`). */
export type EdObject = ArrowObj | LineObj | BoxObj | StrokeObj | TextObj | CounterObj | RedactObj | SpotObj | ImageObj;
export type Kind = EdObject["kind"];
/** The document as the editor holds it; assignable to and from `AnnotDoc`. */
export interface EdDoc extends Omit<AnnotDoc, "objects"> {
  objects: EdObject[];
}

/** Every tool of SPEC §9.3 (`crop` only in the editor window). */
export type ToolId =
  | "select"
  | "arrow"
  | "line"
  | "rect"
  | "ellipse"
  | "pen"
  | "marker"
  | "text"
  | "counter"
  | "redact"
  | "spotlight"
  | "crop"
  | "eyedropper";

export const TOOLS: readonly ToolId[] = [
  "select",
  "arrow",
  "line",
  "rect",
  "ellipse",
  "pen",
  "marker",
  "text",
  "counter",
  "redact",
  "spotlight",
  "crop",
  "eyedropper",
];

/** Tools that create objects (rule 1: a press always starts a new object). */
export const DRAW_TOOLS: ReadonlySet<ToolId> = new Set<ToolId>([
  "arrow",
  "line",
  "rect",
  "ellipse",
  "pen",
  "marker",
  "text",
  "counter",
  "redact",
  "spotlight",
]);

/** Thickness steps (px per `unit`) for sizes 1…5. */
export const TABLE = [2, 3, 5, 8, 12] as const;
/** Text sizes (logical px) for the size slot 1…5. */
export const FONT_SIZES = [14, 18, 24, 32, 48] as const;
/** Counter radius (logical px) for sizes 1…5. */
export const COUNTER_R = [10, 12, 15, 19, 24] as const;
export const TEXT_FONT = '"Inter Variable", "Inter", "Segoe UI", sans-serif';
export const TEXT_WEIGHT = 600;
/** Marker width = pen width × this (SPEC: wide, translucent). */
export const MARKER_SCALE = 4;
export const MARKER_ALPHA = 0.45;

export const SIZES: readonly Size[] = [1, 2, 3, 4, 5];

/** Stroke width in source px. */
export function strokePx(size: Size, unit: number): number {
  return TABLE[size - 1] * unit;
}

/** Fixed layers bottom → top (SPEC §9.3 rule 9). */
export function layerOf(kind: Kind): number {
  switch (kind) {
    case "redact":
      return 0;
    case "spotlight":
      return 1;
    case "marker":
      return 2;
    case "counter":
      return 4;
    default:
      return 3;
  }
}

/** Index at which a new object of `kind` goes: the end of its layer (array order = z-order). */
export function insertIndex(objects: readonly EdObject[], kind: Kind): number {
  const layer = layerOf(kind);
  let i = 0;
  while (i < objects.length && layerOf(objects[i].kind) <= layer) i++;
  return i;
}

/** Stable sort by layer (repairs a document whose array order breaks the layers). */
export function sortByLayer(objects: readonly EdObject[]): EdObject[] {
  return objects
    .map((o, i) => [o, i] as const)
    .sort((a, b) => layerOf(a[0].kind) - layerOf(b[0].kind) || a[1] - b[1])
    .map(([o]) => o);
}

let seq = 0;
/** A short unique id (unique within the document; never persisted meaningfully). */
export function newId(): string {
  seq = (seq + 1) % 1_000_000;
  return `${Date.now().toString(36)}${seq.toString(36)}${Math.floor(Math.random() * 1296).toString(36)}`;
}

/** A new empty document for a picture of `w × h` source px. */
export function emptyDoc(w: number, h: number, unit = 1): EdDoc {
  return { v: 1, w: Math.round(w), h: Math.round(h), unit: unit > 0 && Number.isFinite(unit) ? unit : 1, crop: null, objects: [] };
}

/** Counter numbers in array order: explicit `n`, else previous + 1 (first = 1). */
export function counterNumbers(objects: readonly EdObject[]): Map<string, number> {
  const out = new Map<string, number>();
  let prev = 0;
  for (const o of objects) {
    if (o.kind !== "counter") continue;
    const n = o.n ?? prev + 1;
    out.set(o.id, n);
    prev = n;
  }
  return out;
}

// ── validation ───────────────────────────────────────────────────────────────

const HEX = /^#[0-9a-f]{6}$/i;
const num = (v: unknown): v is number => typeof v === "number" && Number.isFinite(v);
const pt = (v: unknown): v is Pt => Array.isArray(v) && v.length >= 2 && num(v[0]) && num(v[1]);
const rect = (v: unknown): v is Rect => !!v && typeof v === "object" && num((v as Rect).x) && num((v as Rect).y) && num((v as Rect).w) && num((v as Rect).h);
const size = (v: unknown): Size => (v === 1 || v === 2 || v === 3 || v === 4 || v === 5 ? v : 3);
const color = (v: unknown, fallback: string): string => (typeof v === "string" && HEX.test(v) ? v.toLowerCase() : fallback);
const P = (v: Pt): Pt => [v[0], v[1]];
const R = (v: Rect): Rect => normRect(v);

/** A rectangle with non-negative width/height. */
export function normRect(r: Rect): Rect {
  return { x: r.w < 0 ? r.x + r.w : r.x, y: r.h < 0 ? r.y + r.h : r.y, w: Math.abs(r.w), h: Math.abs(r.h) };
}

/** One object from untrusted JSON, or `null` when it can't be read. Unknown fields are dropped. */
export function readObject(raw: unknown): EdObject | null {
  if (!raw || typeof raw !== "object") return null;
  const o = raw as Record<string, unknown>;
  const id = typeof o.id === "string" && o.id ? o.id : newId();
  const c = color(o.color, "#ff3b30");
  switch (o.kind) {
    case "arrow": {
      if (!pt(o.a) || !pt(o.b)) return null;
      const style = o.style === "curved" || o.style === "double" ? o.style : "straight";
      return { id, color: c, kind: "arrow", a: P(o.a), b: P(o.b), bend: num(o.bend) ? o.bend : 0, style, size: size(o.size) };
    }
    case "line":
      return pt(o.a) && pt(o.b) ? { id, color: c, kind: "line", a: P(o.a), b: P(o.b), size: size(o.size) } : null;
    case "rect":
    case "ellipse": {
      if (!rect(o.r)) return null;
      const box: BoxObj = { id, color: c, kind: o.kind, r: R(o.r), fill: o.fill === true, size: size(o.size) };
      if (o.round === true) box.round = true;
      return box;
    }
    case "pen":
    case "marker": {
      if (!Array.isArray(o.points)) return null;
      const points = o.points.filter(pt).map(P);
      return points.length ? { id, color: c, kind: o.kind, points, size: size(o.size) } : null;
    }
    case "text": {
      if (!pt(o.at) || typeof o.text !== "string") return null;
      const width = num(o.width) && o.width > 0 ? o.width : null;
      const fontSize = num(o.fontSize) && o.fontSize > 0 ? o.fontSize : FONT_SIZES[2];
      return { id, color: c, kind: "text", at: P(o.at), width, text: o.text, fontSize, style: o.style === "badge" ? "badge" : "plain" };
    }
    case "counter":
      return pt(o.at)
        ? { id, color: c, kind: "counter", at: P(o.at), n: num(o.n) && o.n >= 0 ? Math.round(o.n) : null, size: size(o.size) }
        : null;
    case "redact": {
      if (!rect(o.r)) return null;
      const mode = o.mode === "pixelate" || o.mode === "blur" ? o.mode : "solid";
      return { id, color: color(o.color, "#000000"), kind: "redact", r: R(o.r), mode, size: size(o.size) };
    }
    case "spotlight":
      return rect(o.r) ? { id, kind: "spotlight", r: R(o.r), shape: o.shape === "ellipse" ? "ellipse" : "rect" } : null;
    case "image":
      return rect(o.r) && typeof o.src === "string" && o.src.startsWith("data:image/")
        ? { id, kind: "image", r: R(o.r), src: o.src }
        : null;
    default:
      return null;
  }
}

export interface LoadResult {
  doc: EdDoc;
  /** the stored document has a version this build doesn't know: opened without objects (toast) */
  unknownVersion: boolean;
  /** objects that couldn't be read and were dropped */
  dropped: number;
}

/**
 * Read a stored document for a picture of `w × h` (the real decoded size wins over the stored
 * one). `null` → an empty document. Unknown `v` → empty document + `unknownVersion` (the caller
 * shows a toast; the source still opens). Extra fields are ignored; unreadable objects dropped;
 * duplicate ids renamed; the array is put back into layer order.
 */
export function loadDoc(raw: unknown, w: number, h: number, unit = 1): LoadResult {
  if (raw === null || raw === undefined) return { doc: emptyDoc(w, h, unit), unknownVersion: false, dropped: 0 };
  const d = raw as Record<string, unknown>;
  if (!d || typeof d !== "object" || d.v !== 1) return { doc: emptyDoc(w, h, unit), unknownVersion: true, dropped: 0 };
  const list = Array.isArray(d.objects) ? d.objects : [];
  const seen = new Set<string>();
  const objects: EdObject[] = [];
  for (const item of list) {
    const o = readObject(item);
    if (!o) continue;
    if (seen.has(o.id)) o.id = newId();
    seen.add(o.id);
    objects.push(o);
  }
  const crop = rect(d.crop) ? clampRect(normRect(d.crop), w, h) : null;
  return {
    doc: {
      v: 1,
      w: Math.round(w),
      h: Math.round(h),
      unit: num(d.unit) && d.unit > 0 ? d.unit : unit,
      crop: crop && crop.w >= 1 && crop.h >= 1 ? crop : null,
      objects: sortByLayer(objects),
    },
    unknownVersion: false,
    dropped: list.length - objects.length,
  };
}

/** The stored form (plain JSON; key order stable). */
export function serializeDoc(doc: EdDoc): string {
  return JSON.stringify(doc);
}

/** `r` clamped into `0..w × 0..h`. */
export function clampRect(r: Rect, w: number, h: number): Rect {
  const x = Math.max(0, Math.min(w, r.x));
  const y = Math.max(0, Math.min(h, r.y));
  return { x, y, w: Math.max(0, Math.min(w, r.x + r.w) - x), h: Math.max(0, Math.min(h, r.y + r.h) - y) };
}

// ── geometry helpers shared by every module ──────────────────────────────────

/** Move an object by `(dx, dy)` source px (a new object; the input is never mutated). */
export function translate(o: EdObject, dx: number, dy: number): EdObject {
  const m = (p: Pt): Pt => [p[0] + dx, p[1] + dy];
  switch (o.kind) {
    case "arrow":
    case "line":
      return { ...o, a: m(o.a), b: m(o.b) };
    case "pen":
    case "marker":
      return { ...o, points: o.points.map(m) };
    case "text":
    case "counter":
      return { ...o, at: m(o.at) };
    default:
      return { ...o, r: { ...o.r, x: o.r.x + dx, y: o.r.y + dy } };
  }
}

/**
 * The document of a sub-rectangle `rect` of the picture: objects shifted by `-rect.x/-rect.y`,
 * objects entirely outside `rect` dropped (`bounds` gives an object's box), `w/h` = the
 * rectangle. The capture overlay uses it for the committed frame (§18.7).
 */
export function cropDoc(doc: EdDoc, rect: Rect, bounds: (o: EdObject) => Rect): EdDoc {
  const r = normRect(rect);
  const inside = (b: Rect) => b.x <= r.x + r.w && b.y <= r.y + r.h && b.x + b.w >= r.x && b.y + b.h >= r.y;
  return {
    v: 1,
    w: Math.round(r.w),
    h: Math.round(r.h),
    unit: doc.unit,
    crop: null,
    objects: doc.objects.filter((o) => inside(bounds(o))).map((o) => translate(o, -r.x, -r.y)),
  };
}
