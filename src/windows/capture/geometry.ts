// Geometry of the capture overlay (ARCHITECTURE §18.7; W6-D). Two coordinate spaces:
//
//   frame px — physical pixels of the monitor frame (`session.monitor`, `session.windows`,
//              `ShotCommit.rect`). Every selection and the stage-2 frame live here.
//   view px  — CSS px of the overlay window (pointer events, the chrome's layout).
//
// The factor is `frame.width / innerWidth` — never `devicePixelRatio`: the window has just been
// moved to the monitor of the cursor and the ratio the page sees can be stale (research §4,
// П-22). Pure — node-tested (tests/captureGeometry.test.mjs).
import type { Rect } from "$lib/ipc/types";

export interface Point {
  x: number;
  y: number;
}

export interface Size {
  w: number;
  h: number;
}

/** View px → frame px factors (horizontal and vertical; equal in the real window). */
export interface Scale {
  kx: number;
  ky: number;
}

export const clamp = (v: number, lo: number, hi: number): number => Math.min(hi, Math.max(lo, v));

/** `frame.width / innerWidth` (and the same vertically). A zero-sized view keeps 1. */
export function viewScale(frame: Size, view: Size): Scale {
  const kx = view.w > 0 && frame.w > 0 ? frame.w / view.w : 1;
  const ky = view.h > 0 && frame.h > 0 ? frame.h / view.h : kx;
  return { kx, ky };
}

export const toFrame = (p: Point, k: Scale): Point => ({ x: p.x * k.kx, y: p.y * k.ky });
export const toView = (p: Point, k: Scale): Point => ({ x: p.x / k.kx, y: p.y / k.ky });
export const rectToView = (r: Rect, k: Scale): Rect => ({ x: r.x / k.kx, y: r.y / k.ky, w: r.w / k.kx, h: r.h / k.ky });

/**
 * The integer frame position under a view point: the pixel the cursor is on, as a grid
 * position `0…frame.w` (the right/bottom edge is reachable, so a region can end at the edge).
 */
export function framePoint(p: Point, k: Scale, frame: Size): Point {
  return { x: clamp(Math.floor(p.x * k.kx), 0, frame.w), y: clamp(Math.floor(p.y * k.ky), 0, frame.h) };
}

/** A pointer that moved at least this far (view px, either axis) is a drag, not a click. */
export const DRAG_THRESHOLD = 5;

export const isDrag = (down: Point, now: Point, threshold = DRAG_THRESHOLD): boolean =>
  Math.max(Math.abs(now.x - down.x), Math.abs(now.y - down.y)) >= threshold;

export const contains = (r: Rect, p: Point): boolean => p.x >= r.x && p.x < r.x + r.w && p.y >= r.y && p.y < r.y + r.h;

export const sameRect = (a: Rect | null, b: Rect | null): boolean =>
  a === b || (!!a && !!b && a.x === b.x && a.y === b.y && a.w === b.w && a.h === b.h);

/** The whole frame as a rectangle. */
export const fullRect = (frame: Size): Rect => ({ x: 0, y: 0, w: frame.w, h: frame.h });

/** Round to whole frame pixels and keep inside the frame (`null` when nothing is left). */
export function snapRect(r: Rect, frame: Size): Rect | null {
  const x0 = clamp(Math.round(Math.min(r.x, r.x + r.w)), 0, frame.w);
  const y0 = clamp(Math.round(Math.min(r.y, r.y + r.h)), 0, frame.h);
  const x1 = clamp(Math.round(Math.max(r.x, r.x + r.w)), 0, frame.w);
  const y1 = clamp(Math.round(Math.max(r.y, r.y + r.h)), 0, frame.h);
  return x1 - x0 >= 1 && y1 - y0 >= 1 ? { x: x0, y: y0, w: x1 - x0, h: y1 - y0 } : null;
}

// ── drawing a region ─────────────────────────────────────────────────────────

/** Shift — a square; Alt — from the centre (SPEC §9.2). */
export interface RegionMods {
  square: boolean;
  fromCenter: boolean;
}

/**
 * The region from the anchor (where the drag started) to the cursor, inside the frame. A square
 * keeps square at the frame edges (its side is limited by the room on both axes); from the
 * centre the anchor is the middle of the region.
 */
export function regionFrom(a: Point, c: Point, mods: RegionMods, frame: Size): Rect {
  const sx = c.x - a.x < 0 ? -1 : 1;
  const sy = c.y - a.y < 0 ? -1 : 1;
  const roomX = mods.fromCenter ? Math.min(a.x, frame.w - a.x) : sx > 0 ? frame.w - a.x : a.x;
  const roomY = mods.fromCenter ? Math.min(a.y, frame.h - a.y) : sy > 0 ? frame.h - a.y : a.y;
  let dx = Math.min(Math.abs(c.x - a.x), Math.max(0, roomX));
  let dy = Math.min(Math.abs(c.y - a.y), Math.max(0, roomY));
  if (mods.square) dx = dy = Math.min(Math.max(dx, dy), Math.max(0, roomX), Math.max(0, roomY));
  if (mods.fromCenter) return { x: a.x - dx, y: a.y - dy, w: 2 * dx, h: 2 * dy };
  return { x: sx > 0 ? a.x : a.x - dx, y: sy > 0 ? a.y : a.y - dy, w: dx, h: dy };
}

/** Keep a point on the frame grid (`0…w`, `0…h`). */
export const clampPoint = (p: Point, frame: Size): Point => ({ x: clamp(p.x, 0, frame.w), y: clamp(p.y, 0, frame.h) });

// ── the stage-2 frame ────────────────────────────────────────────────────────

/** Move a rectangle, keeping it inside the frame (its size never changes). */
export function moveRect(r: Rect, dx: number, dy: number, frame: Size): Rect {
  return { x: clamp(r.x + dx, 0, Math.max(0, frame.w - r.w)), y: clamp(r.y + dy, 0, Math.max(0, frame.h - r.h)), w: r.w, h: r.h };
}

export type Handle = "nw" | "n" | "ne" | "e" | "se" | "s" | "sw" | "w";

/** Hit priority: corners first (on a small frame the side handles overlap them). */
export const HANDLES: readonly Handle[] = ["nw", "ne", "se", "sw", "n", "e", "s", "w"];

export const HANDLE_CURSOR: Record<Handle, string> = {
  nw: "nwse-resize",
  se: "nwse-resize",
  ne: "nesw-resize",
  sw: "nesw-resize",
  n: "ns-resize",
  s: "ns-resize",
  e: "ew-resize",
  w: "ew-resize",
};

/**
 * Resize from the rectangle the drag started with: the handle's edges follow the pointer
 * delta (frame px), clamped to the frame. Dragging an edge past the opposite one flips the
 * rectangle; it never gets thinner than 1 px.
 */
export function resizeRect(start: Rect, handle: Handle, dx: number, dy: number, frame: Size): Rect {
  let x0 = start.x;
  let y0 = start.y;
  let x1 = start.x + start.w;
  let y1 = start.y + start.h;
  if (handle.includes("w")) x0 = clamp(x0 + dx, 0, frame.w);
  if (handle.includes("e")) x1 = clamp(x1 + dx, 0, frame.w);
  if (handle.includes("n")) y0 = clamp(y0 + dy, 0, frame.h);
  if (handle.includes("s")) y1 = clamp(y1 + dy, 0, frame.h);
  const [x, w] = span(x0, x1, frame.w);
  const [y, h] = span(y0, y1, frame.h);
  return { x, y, w, h };
}

/** `[start, length]` of an edge pair, at least 1 px and inside `0…max`. */
function span(a: number, b: number, max: number): [number, number] {
  const len = Math.abs(b - a);
  return len >= 1 ? [Math.min(a, b), len] : [Math.min(Math.min(a, b), max - 1), 1];
}

/** Arrow key → unit direction (`KeyboardEvent.code`). */
export function arrowDelta(code: string): Point | null {
  switch (code) {
    case "ArrowLeft":
      return { x: -1, y: 0 };
    case "ArrowRight":
      return { x: 1, y: 0 };
    case "ArrowUp":
      return { x: 0, y: -1 };
    case "ArrowDown":
      return { x: 0, y: 1 };
    default:
      return null;
  }
}

/**
 * `Ctrl+arrows` move the frame, `Alt+arrows` resize it from the right/bottom edge
 * (→ / ↓ grow, ← / ↑ shrink, never below 1 px, never past the frame) — SPEC §9.2 (INT-5: not
 * `Ctrl+Alt+arrows`, which some Intel drivers take for screen rotation).
 */
export function nudgeRect(r: Rect, d: Point, step: number, mode: "move" | "resize", frame: Size): Rect {
  if (mode === "move") return moveRect(r, d.x * step, d.y * step, frame);
  return {
    x: r.x,
    y: r.y,
    w: clamp(r.w + d.x * step, 1, frame.w - r.x),
    h: clamp(r.h + d.y * step, 1, frame.h - r.y),
  };
}

// ── frame chrome (view px) ───────────────────────────────────────────────────

/** Handle centres sit this far OUTSIDE the frame edge, so drawing inside never grabs one (§18.7). */
export const HANDLE_OFFSET = 12;
/** Hit radius of a handle. */
export const HANDLE_R = 8;
/** The move band: this wide, just outside the frame edge. */
export const BAND = 10;
/** Side handles hide on a side shorter than this (they would sit on the corner handles). */
export const SIDE_HANDLE_MIN = 40;

/** The handles a frame shows (side handles only on a long enough side). */
export function visibleHandles(r: Rect): Handle[] {
  return HANDLES.filter((h) => (h === "n" || h === "s" ? r.w >= SIDE_HANDLE_MIN : h === "e" || h === "w" ? r.h >= SIDE_HANDLE_MIN : true));
}

/**
 * Handle centres around a frame (view px). At a monitor edge a handle would be off screen, so it
 * is pulled back into the view (it then sits just inside the frame and still works).
 */
export function handleCenters(r: Rect, view: Size): Record<Handle, Point> {
  const o = HANDLE_OFFSET;
  const xs = { w: r.x - o, m: r.x + r.w / 2, e: r.x + r.w + o };
  const ys = { n: r.y - o, m: r.y + r.h / 2, s: r.y + r.h + o };
  const at = (x: number, y: number): Point => ({ x: clamp(x, HANDLE_R, view.w - HANDLE_R), y: clamp(y, HANDLE_R, view.h - HANDLE_R) });
  return {
    nw: at(xs.w, ys.n),
    n: at(xs.m, ys.n),
    ne: at(xs.e, ys.n),
    e: at(xs.e, ys.m),
    se: at(xs.e, ys.s),
    s: at(xs.m, ys.s),
    sw: at(xs.w, ys.s),
    w: at(xs.w, ys.m),
  };
}

export type FrameHit = { kind: "handle"; handle: Handle } | { kind: "band" } | { kind: "inside" } | { kind: "outside" };

/** What a view point hits around the frame: a handle, the move band, the inside, or outside. */
export function hitFrame(p: Point, r: Rect, view: Size): FrameHit {
  const centers = handleCenters(r, view);
  for (const h of visibleHandles(r)) {
    const c = centers[h];
    if (Math.hypot(p.x - c.x, p.y - c.y) <= HANDLE_R) return { kind: "handle", handle: h };
  }
  if (contains(r, p)) return { kind: "inside" };
  if (contains({ x: r.x - BAND, y: r.y - BAND, w: r.w + 2 * BAND, h: r.h + 2 * BAND }, p)) return { kind: "band" };
  return { kind: "outside" };
}
