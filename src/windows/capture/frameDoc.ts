// The overlay's annotation document → the shot's document (ARCHITECTURE §18.4, §18.7; W6-D).
// In the overlay the editor's source is the whole monitor frame, so its objects are in frame
// px. The shot is the frame only: coordinates move by −frame.x/y, the size becomes the frame's
// and objects lying wholly outside the frame are not stored (SPEC §9.2 "ничего не теряется"
// covers what the user can see in the frame).
//
// Object bounds are CONSERVATIVE (a little larger than the drawing): the host doesn't lay out
// text or arrowheads — the editor (W6-C) does — so "outside" means "certainly outside", and
// "empty here" (double-click → Done) means "certainly nothing here". Pure — node-tested
// (tests/captureFrameDoc.test.mjs).
import type { AnnotDoc, AnnotObject, Pt, Rect, Size as StrokeSize } from "$lib/ipc/types";

/** Stroke widths by size step, logical px (§18.6: `px = TABLE[size] * unit`). */
export const LINE_TABLE: readonly number[] = [2, 3, 5, 8, 12];

const stroke = (size: StrokeSize, unit: number): number => (LINE_TABLE[size - 1] ?? LINE_TABLE[2]) * unit;

const norm = (r: Rect): Rect => ({ x: Math.min(r.x, r.x + r.w), y: Math.min(r.y, r.y + r.h), w: Math.abs(r.w), h: Math.abs(r.h) });

const pad = (r: Rect, p: number): Rect => ({ x: r.x - p, y: r.y - p, w: r.w + 2 * p, h: r.h + 2 * p });

function boxOf(points: readonly Pt[]): Rect {
  let x0 = Infinity;
  let y0 = Infinity;
  let x1 = -Infinity;
  let y1 = -Infinity;
  for (const [x, y] of points) {
    x0 = Math.min(x0, x);
    y0 = Math.min(y0, y);
    x1 = Math.max(x1, x);
    y1 = Math.max(y1, y);
  }
  return Number.isFinite(x0) ? { x: x0, y: y0, w: x1 - x0, h: y1 - y0 } : { x: 0, y: 0, w: 0, h: 0 };
}

/** A conservative bounding box of an object (source px). */
export function objectBounds(o: AnnotObject, unit: number): Rect {
  const u = unit > 0 ? unit : 1;
  switch (o.kind) {
    case "arrow":
      // the head is a few strokes wide; `bend` is taken as the curve's offset in source px
      return pad(boxOf([o.a, o.b]), stroke(o.size, u) * 4 + Math.abs(o.bend));
    case "line":
      return pad(boxOf([o.a, o.b]), stroke(o.size, u));
    case "rect":
    case "ellipse":
    case "redact":
      return pad(norm(o.r), stroke(o.size, u));
    case "spotlight":
    case "image":
      return norm(o.r);
    case "pen":
      return pad(boxOf(o.points), stroke(o.size, u));
    case "marker":
      return pad(boxOf(o.points), stroke(o.size, u) * 3);
    case "text": {
      const font = o.fontSize * u;
      const lines = o.text.split("\n");
      const longest = Math.max(1, ...lines.map((l) => l.length));
      const w = o.width ?? longest * font * 0.75;
      const extra = o.style === "badge" ? font : font * 0.5;
      return pad({ x: o.at[0], y: o.at[1] - font, w, h: (lines.length + 1) * font * 1.5 }, extra);
    }
    case "counter": {
      const r = (12 + LINE_TABLE[(o.size ?? 3) - 1] * 3) * u;
      return { x: o.at[0] - r, y: o.at[1] - r, w: 2 * r, h: 2 * r };
    }
  }
}

export const intersects = (a: Rect, b: Rect): boolean => a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;

const shiftPt = (p: Pt, dx: number, dy: number): Pt => [p[0] + dx, p[1] + dy];
const shiftRect = (r: Rect, dx: number, dy: number): Rect => ({ ...r, x: r.x + dx, y: r.y + dy });

/** One object moved by (dx, dy). */
export function shiftObject(o: AnnotObject, dx: number, dy: number): AnnotObject {
  switch (o.kind) {
    case "arrow":
    case "line":
      return { ...o, a: shiftPt(o.a, dx, dy), b: shiftPt(o.b, dx, dy) };
    case "rect":
    case "ellipse":
    case "redact":
    case "spotlight":
    case "image":
      return { ...o, r: shiftRect(o.r, dx, dy) };
    case "pen":
    case "marker":
      return { ...o, points: o.points.map((p) => shiftPt(p, dx, dy)) };
    case "text":
    case "counter":
      return { ...o, at: shiftPt(o.at, dx, dy) };
  }
}

/**
 * The document of the shot: objects that touch the frame, moved to frame-relative coordinates;
 * the source size is the frame's; no crop (in the overlay the crop IS the frame).
 */
export function docForFrame(doc: AnnotDoc, frame: Rect): AnnotDoc {
  const objects = doc.objects.filter((o) => intersects(objectBounds(o, doc.unit), frame)).map((o) => shiftObject(o, -frame.x, -frame.y));
  return { v: 1, w: frame.w, h: frame.h, unit: doc.unit, crop: null, objects };
}

/** True when no object's (conservative) bounds come within `tolerance` source px of the point. */
export function isEmptyAt(doc: AnnotDoc, x: number, y: number, tolerance = 0): boolean {
  return !doc.objects.some((o) => intersects(pad(objectBounds(o, doc.unit), tolerance), { x, y, w: 0.001, h: 0.001 }));
}

/** An empty document for a frozen monitor frame (`unit` = the monitor scale, §18.6). */
export function emptyDoc(w: number, h: number, unit: number): AnnotDoc {
  return { v: 1, w, h, unit: unit > 0 ? unit : 1, crop: null, objects: [] };
}
