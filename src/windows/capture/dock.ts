// Where the capture overlay puts its floating chrome (ARCHITECTURE §18.7; W6-D): the panel docks
// to the frame (below outside → above outside → inside at the bottom edge) and stays on the
// monitor; the size chip sits above the frame (or inside its corner); the magnifier follows
// the cursor and flips at the monitor edges. All in view px. Pure — node-tested
// (tests/captureDock.test.mjs).
import type { Rect } from "$lib/ipc/types";
import { clamp, HANDLE_OFFSET, HANDLE_R, type Point, type Size } from "./geometry";

/** Frame edge → panel: clears the handles (offset 12 + radius 8) with 4 px to spare. */
export const DOCK_GAP = HANDLE_OFFSET + HANDLE_R + 4;
/** Nothing floats closer than this to a monitor edge. */
export const EDGE = 8;
/** The panel inside a frame that fills the monitor: this far above the frame's bottom edge. */
export const INSIDE_INSET = 16;

export type DockSide = "below" | "above" | "inside";

export interface Docked {
  x: number;
  y: number;
  side: DockSide;
}

/**
 * Dock the panel to the frame: below it outside; if there is no room — above it outside; if
 * neither fits (the frame fills the monitor) — inside, at its bottom edge. Centred on the frame
 * and pressed against the monitor edges, so it never leaves the screen.
 */
export function dockPanel(frame: Rect, panel: Size, view: Size): Docked {
  const x = clamp(frame.x + frame.w / 2 - panel.w / 2, EDGE, Math.max(EDGE, view.w - panel.w - EDGE));
  const below = frame.y + frame.h + DOCK_GAP;
  if (below + panel.h <= view.h - EDGE) return { x, y: below, side: "below" };
  const above = frame.y - DOCK_GAP - panel.h;
  if (above >= EDGE) return { x, y: above, side: "above" };
  const inside = clamp(frame.y + frame.h - INSIDE_INSET - panel.h, EDGE, Math.max(EDGE, view.h - panel.h - EDGE));
  return { x, y: inside, side: "inside" };
}

export const overlaps = (a: Rect, b: Rect): boolean => a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;

/** Above the frame edge, past the corner handle. */
export const CHIP_GAP = DOCK_GAP;
/** Inside the frame's top-left corner, clear of the corner handle (pulled inside at a monitor edge). */
export const CHIP_INSET = 2 * HANDLE_R + 4;

/**
 * The size chip: above the frame's top-left corner, outside (past the handles). Without room
 * there, or when it would cover `avoid` (the docked panel), it moves inside the corner.
 */
export function placeChip(frame: Rect, chip: Size, view: Size, avoid: Rect | null = null): Point & { inside: boolean } {
  const x = clamp(frame.x, EDGE, Math.max(EDGE, view.w - chip.w - EDGE));
  const y = frame.y - CHIP_GAP - chip.h;
  const out = { x, y, w: chip.w, h: chip.h };
  if (y >= EDGE && !(avoid && overlaps(out, avoid))) return { x, y, inside: false };
  return {
    x: clamp(frame.x + CHIP_INSET, EDGE, Math.max(EDGE, view.w - chip.w - EDGE)),
    y: clamp(frame.y + CHIP_INSET, EDGE, Math.max(EDGE, view.h - chip.h - EDGE)),
    inside: true,
  };
}

/** Cursor → magnifier corner. */
export const LOUPE_OFFSET = 16;

/**
 * The magnifier box (lens + caption) at the cursor's lower right; past the right or bottom
 * monitor edge it flips to the other side of the cursor (ShareX), never off screen.
 */
export function placeLoupe(cursor: Point, box: Size, view: Size, offset = LOUPE_OFFSET): Point {
  let x = cursor.x + offset;
  let y = cursor.y + offset;
  if (x + box.w > view.w) x = cursor.x - offset - box.w;
  if (y + box.h > view.h) y = cursor.y - offset - box.h;
  return { x: clamp(x, 0, Math.max(0, view.w - box.w)), y: clamp(y, 0, Math.max(0, view.h - box.h)) };
}
