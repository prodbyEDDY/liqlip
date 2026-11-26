// Geometry of annotations. The first block is a port of Liquid Tools
// `src/windows/capture/annotationGeometry.ts` (arrow bend/head, stroke resize, Shift
// constraints) with its tests (tests/editorGeometry.test.mjs); the rest is new for Liqlip:
// the v1 arrow model (scalar `bend`), distances for contour hit-testing, the pen's thinning
// and smoothing. Pure — no DOM; `paintArrow` only needs the 2D-context methods it calls.
import type { Pt, Rect } from "./doc";

// ── ported from Liquid Tools (annotationGeometry.ts) ─────────────────────────

export type Point = [number, number];
export interface ArrowGeometry {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
  mx: number;
  my: number;
  width: number;
}

/** mx/my are control offsets; the visible bend handle lies ON the curve at t=.5. */
export function arrowBend(s: ArrowGeometry): Point {
  return [(s.x0 + s.x1) / 2 + s.mx / 2, (s.y0 + s.y1) / 2 + s.my / 2];
}

/** The two barbs of the open head at the `(x1, y1)` end (null for a collapsed arrow). */
export function arrowHead(s: ArrowGeometry): [Point, Point] | null {
  const cx = (s.x0 + s.x1) / 2 + s.mx,
    cy = (s.y0 + s.y1) / 2 + s.my;
  const length = Math.hypot(cx - s.x0, cy - s.y0) + Math.hypot(s.x1 - cx, s.y1 - cy);
  if (length < 1) return null;
  let dx = s.x1 - cx,
    dy = s.y1 - cy;
  if (Math.hypot(dx, dy) < 0.01) {
    dx = s.x1 - s.x0;
    dy = s.y1 - s.y0;
  }
  const angle = Math.atan2(dy, dx),
    size = Math.min(length * 0.4, Math.max(10, s.width * 3.5));
  return [-1, 1].map(
    (sign) => [s.x1 - Math.cos(angle + (sign * Math.PI) / 6) * size, s.y1 - Math.sin(angle + (sign * Math.PI) / 6) * size] as Point,
  ) as [Point, Point];
}

/** The 2D-context surface `paintArrow` needs (a real canvas context satisfies it). */
export interface StrokeCtx {
  lineCap: CanvasLineCap;
  lineJoin: CanvasLineJoin;
  lineWidth: number;
  beginPath(): void;
  moveTo(x: number, y: number): void;
  lineTo(x: number, y: number): void;
  quadraticCurveTo(cpx: number, cpy: number, x: number, y: number): void;
  stroke(): void;
}

/** A conventional constant-width arrow with a rounded open head (`double` — a head at both ends). */
export function paintArrow(ctx: StrokeCtx, s: ArrowGeometry, double = false): void {
  const head = arrowHead(s);
  if (!head) return;
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  ctx.lineWidth = s.width;
  ctx.beginPath();
  ctx.moveTo(s.x0, s.y0);
  ctx.quadraticCurveTo((s.x0 + s.x1) / 2 + s.mx, (s.y0 + s.y1) / 2 + s.my, s.x1, s.y1);
  ctx.moveTo(...head[0]);
  ctx.lineTo(s.x1, s.y1);
  ctx.lineTo(...head[1]);
  if (double) {
    const tail = arrowHead({ ...s, x0: s.x1, y0: s.y1, x1: s.x0, y1: s.y0 });
    if (tail) {
      ctx.moveTo(...tail[0]);
      ctx.lineTo(s.x0, s.y0);
      ctx.lineTo(...tail[1]);
    }
  }
  ctx.stroke();
}

/** Re-map from the gesture-start path, never compound scaling across move events.
 * Zero-width/height strokes translate instead of dividing by zero. */
export function resizePath(points: Point[], from: Rect, to: Rect): Point[] {
  return points.map(([x, y]) => [
    from.w ? to.x + ((x - from.x) * to.w) / from.w : to.x + to.w / 2,
    from.h ? to.y + ((y - from.y) * to.h) / from.h : to.y + to.h / 2,
  ]);
}

/** Shift: squares/circles for boxes, 45° steps for lines and arrows. */
export function constrainEndpoint(type: string, x0: number, y0: number, cxp: number, cyp: number, shift: boolean): [number, number] {
  if (!shift) return [cxp, cyp];
  if (type === "rect" || type === "ellipse" || type === "redact" || type === "spotlight") {
    const s = Math.max(Math.abs(cxp - x0), Math.abs(cyp - y0));
    return [x0 + Math.sign(cxp - x0 || 1) * s, y0 + Math.sign(cyp - y0 || 1) * s];
  }
  if (type === "line" || type === "arrow" || type === "pen") {
    const a = Math.round(Math.atan2(cyp - y0, cxp - x0) / (Math.PI / 4)) * (Math.PI / 4);
    const len = Math.hypot(cxp - x0, cyp - y0);
    return [x0 + Math.cos(a) * len, y0 + Math.sin(a) * len];
  }
  return [cxp, cyp];
}

// ── v1 arrow model ───────────────────────────────────────────────────────────
// `bend` is the signed offset, in SOURCE px, of the curve's midpoint from the chord's midpoint,
// perpendicular to the chord (positive = along (−dy, dx) of a → b). The quadratic's control point
// sits at twice that offset, so |bend| is exactly how far the arch leaves the chord (hosts can
// pad bounds by it). `bend = 0` is a straight arrow.

/** Default arch of the "curved" style: 0.2 × the chord length. */
export const CURVED_RATIO = 0.2;

function unitNormal(a: Pt, b: Pt): Pt {
  const dx = b[0] - a[0],
    dy = b[1] - a[1];
  const len = Math.hypot(dx, dy);
  return len < 1e-9 ? [0, 0] : [-dy / len, dx / len];
}

/** The quadratic control point of an arrow `a → b` with `bend`. */
export function arrowControl(a: Pt, b: Pt, bend: number): Pt {
  const [nx, ny] = unitNormal(a, b);
  return [(a[0] + b[0]) / 2 + 2 * bend * nx, (a[1] + b[1]) / 2 + 2 * bend * ny];
}

/** The bend handle: the curve's point at t = 0.5. */
export function arrowMid(a: Pt, b: Pt, bend: number): Pt {
  const [nx, ny] = unitNormal(a, b);
  return [(a[0] + b[0]) / 2 + bend * nx, (a[1] + b[1]) / 2 + bend * ny];
}

/** `bend` that puts the curve's midpoint at the pointer's perpendicular projection. */
export function bendFromPoint(a: Pt, b: Pt, p: Pt): number {
  const [nx, ny] = unitNormal(a, b);
  return (p[0] - (a[0] + b[0]) / 2) * nx + (p[1] - (a[1] + b[1]) / 2) * ny;
}

/** The v1 arrow as the ported `ArrowGeometry` (control offset = control − chord midpoint). */
export function arrowGeometry(a: Pt, b: Pt, bend: number, width: number): ArrowGeometry {
  const c = arrowControl(a, b, bend);
  return { x0: a[0], y0: a[1], x1: b[0], y1: b[1], mx: c[0] - (a[0] + b[0]) / 2, my: c[1] - (a[1] + b[1]) / 2, width };
}

/** Points along a quadratic curve (inclusive ends). */
export function quadPoints(a: Pt, c: Pt, b: Pt, n = 24): Pt[] {
  const out: Pt[] = [];
  for (let i = 0; i <= n; i++) {
    const t = i / n,
      u = 1 - t;
    out.push([u * u * a[0] + 2 * u * t * c[0] + t * t * b[0], u * u * a[1] + 2 * u * t * c[1] + t * t * b[1]]);
  }
  return out;
}

// ── distances ────────────────────────────────────────────────────────────────

export function dist(a: Pt, b: Pt): number {
  return Math.hypot(a[0] - b[0], a[1] - b[1]);
}

/** Distance from `p` to the segment `a–b`. */
export function distSeg(p: Pt, a: Pt, b: Pt): number {
  const dx = b[0] - a[0],
    dy = b[1] - a[1];
  const len2 = dx * dx + dy * dy;
  const t = len2 ? Math.max(0, Math.min(1, ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2)) : 0;
  return Math.hypot(p[0] - (a[0] + t * dx), p[1] - (a[1] + t * dy));
}

/** Distance from `p` to a polyline (a single point counts as a dot). */
export function distPolyline(p: Pt, pts: readonly Pt[]): number {
  if (!pts.length) return Infinity;
  if (pts.length === 1) return dist(p, pts[0]);
  let d = Infinity;
  for (let i = 1; i < pts.length; i++) d = Math.min(d, distSeg(p, pts[i - 1], pts[i]));
  return d;
}

/** Distance from `p` to the BORDER of a rectangle (0 on the border; positive inside and out). */
export function distRectEdge(p: Pt, r: Rect): number {
  const x0 = r.x,
    y0 = r.y,
    x1 = r.x + r.w,
    y1 = r.y + r.h;
  const inside = p[0] >= x0 && p[0] <= x1 && p[1] >= y0 && p[1] <= y1;
  if (inside) return Math.min(p[0] - x0, x1 - p[0], p[1] - y0, y1 - p[1]);
  const dx = Math.max(x0 - p[0], 0, p[0] - x1),
    dy = Math.max(y0 - p[1], 0, p[1] - y1);
  return Math.hypot(dx, dy);
}

export function inRect(p: Pt, r: Rect, pad = 0): boolean {
  return p[0] >= r.x - pad && p[0] <= r.x + r.w + pad && p[1] >= r.y - pad && p[1] <= r.y + r.h + pad;
}

/** The ellipse inscribed in `r` as a closed polygon. */
export function ellipsePoints(r: Rect, n = 48): Pt[] {
  const cx = r.x + r.w / 2,
    cy = r.y + r.h / 2;
  const out: Pt[] = [];
  for (let i = 0; i <= n; i++) {
    const t = (i / n) * Math.PI * 2;
    out.push([cx + (Math.cos(t) * r.w) / 2, cy + (Math.sin(t) * r.h) / 2]);
  }
  return out;
}

/** Is `p` inside the ellipse inscribed in `r` (grown by `pad`)? */
export function inEllipse(p: Pt, r: Rect, pad = 0): boolean {
  const rx = r.w / 2 + pad,
    ry = r.h / 2 + pad;
  if (rx <= 0 || ry <= 0) return false;
  const nx = (p[0] - (r.x + r.w / 2)) / rx,
    ny = (p[1] - (r.y + r.h / 2)) / ry;
  return nx * nx + ny * ny <= 1;
}

/** The rectangle spanned by two corners (non-negative size). */
export function rectFrom(a: Pt, b: Pt): Rect {
  return { x: Math.min(a[0], b[0]), y: Math.min(a[1], b[1]), w: Math.abs(b[0] - a[0]), h: Math.abs(b[1] - a[1]) };
}

/** A box drag: Shift — square, Alt — from the centre (both — square from the centre). */
export function dragBox(start: Pt, cur: Pt, shift: boolean, alt: boolean): Rect {
  const [ex, ey] = constrainEndpoint("rect", start[0], start[1], cur[0], cur[1], shift);
  if (alt) {
    const dx = Math.abs(ex - start[0]),
      dy = Math.abs(ey - start[1]);
    return { x: start[0] - dx, y: start[1] - dy, w: dx * 2, h: dy * 2 };
  }
  return rectFrom(start, [ex, ey]);
}

/** Bounding box of points. */
export function boundsOf(pts: readonly Pt[]): Rect {
  let x0 = Infinity,
    y0 = Infinity,
    x1 = -Infinity,
    y1 = -Infinity;
  for (const [x, y] of pts) {
    x0 = Math.min(x0, x);
    y0 = Math.min(y0, y);
    x1 = Math.max(x1, x);
    y1 = Math.max(y1, y);
  }
  return pts.length ? { x: x0, y: y0, w: x1 - x0, h: y1 - y0 } : { x: 0, y: 0, w: 0, h: 0 };
}

export function growRect(r: Rect, by: number): Rect {
  return { x: r.x - by, y: r.y - by, w: r.w + by * 2, h: r.h + by * 2 };
}

export function rectsIntersect(a: Rect, b: Rect): boolean {
  return a.x <= b.x + b.w && b.x <= a.x + a.w && a.y <= b.y + b.h && b.y <= a.y + a.h;
}

// ── pen: thinning + smoothing ────────────────────────────────────────────────

/** Append `p` to a live stroke unless it is closer than `minStep` to the last kept point. */
export function addPoint(points: readonly Pt[], p: Pt, minStep: number): Pt[] {
  const last = points[points.length - 1];
  if (last && dist(last, p) < minStep) return points as Pt[];
  return [...points, p];
}

/** Ramer–Douglas–Peucker simplification (keeps both ends). */
export function simplify(points: readonly Pt[], eps: number): Pt[] {
  if (points.length < 3) return points.slice();
  const keep = new Uint8Array(points.length);
  keep[0] = keep[points.length - 1] = 1;
  const stack: [number, number][] = [[0, points.length - 1]];
  while (stack.length) {
    const [i, j] = stack.pop()!;
    let best = -1,
      bestD = eps;
    for (let k = i + 1; k < j; k++) {
      const d = distSeg(points[k], points[i], points[j]);
      if (d > bestD) {
        bestD = d;
        best = k;
      }
    }
    if (best >= 0) {
      keep[best] = 1;
      stack.push([i, best], [best, j]);
    }
  }
  return points.filter((_, i) => keep[i]);
}

/** Exponential smoothing of the pointer (streamline): each point moves toward the raw input. */
export function streamline(prev: Pt | undefined, raw: Pt, amount: number): Pt {
  if (!prev) return raw;
  return [prev[0] + (raw[0] - prev[0]) * (1 - amount), prev[1] + (raw[1] - prev[1]) * (1 - amount)];
}

/** The 2D-context surface the smooth path needs. */
export interface PathCtx {
  moveTo(x: number, y: number): void;
  lineTo(x: number, y: number): void;
  quadraticCurveTo(cpx: number, cpy: number, x: number, y: number): void;
}

/** Trace a smooth curve through `pts`: quadratic segments between midpoints (C¹ continuous). */
export function tracePath(ctx: PathCtx, pts: readonly Pt[]): void {
  if (!pts.length) return;
  ctx.moveTo(pts[0][0], pts[0][1]);
  if (pts.length === 1) {
    ctx.lineTo(pts[0][0] + 0.01, pts[0][1]);
    return;
  }
  if (pts.length === 2) {
    ctx.lineTo(pts[1][0], pts[1][1]);
    return;
  }
  for (let i = 1; i < pts.length - 1; i++) {
    const mx = (pts[i][0] + pts[i + 1][0]) / 2,
      my = (pts[i][1] + pts[i + 1][1]) / 2;
    ctx.quadraticCurveTo(pts[i][0], pts[i][1], mx, my);
  }
  const last = pts[pts.length - 1];
  ctx.lineTo(last[0], last[1]);
}
