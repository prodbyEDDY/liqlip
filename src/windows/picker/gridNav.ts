// Two-dimensional keyboard navigation over the shelf grid (SPEC §3.5). Liquid Tools walked
// the cards as a 1-D list; here ↑/↓ move by rows, ←/→ by cards, Home/End jump to the ends,
// PgUp/PgDn move by a screenful. In the single-row strip (top/bottom, centre "one row") ←/→
// are the primary axis and ↑/↓ alias them, so the strip stays navigable while the search
// field owns ←/→ for the caret. Pure — node-tested (tests/pickerGridNav).
import { GAP, type Geometry } from "./layout";

export type NavDir = "up" | "down" | "left" | "right" | "home" | "end" | "pageUp" | "pageDown";

export interface NavGrid {
  count: number;
  columns: number;
  horizontal: boolean;
  /** Rows (grid) or tiles (strip) that fit in one viewport — the PgUp/PgDn step. */
  perPage: number;
}

export interface NavResult {
  /** Target index (−1 only when there are no items). */
  index: number;
  /** The move ran into the end of the loaded items — the caller may load the next page
   *  and retry. */
  hitEnd: boolean;
}

/** Build the navigation grid from the layout and the viewport extent along the scroll axis. */
export function navGrid(g: Geometry, extent: number): NavGrid {
  const stride = g.horizontal ? g.strideX : g.strideY;
  return {
    count: g.count,
    columns: g.horizontal ? Math.max(1, g.count) : g.columns,
    horizontal: g.horizontal,
    perPage: Math.max(1, Math.floor((extent + GAP) / stride)),
  };
}

export function navigate(index: number, dir: NavDir, grid: NavGrid): NavResult {
  const { count, horizontal, perPage } = grid;
  if (count <= 0) return { index: -1, hitEnd: false };
  const last = count - 1;
  // No current card yet: any move lands on the first one (End on the last).
  if (index < 0 || index > last) return { index: dir === "end" ? last : 0, hitEnd: false };

  if (dir === "home") return { index: 0, hitEnd: false };
  if (dir === "end") return { index: last, hitEnd: true };

  if (horizontal) {
    const step = dir === "pageUp" || dir === "pageDown" ? perPage : 1;
    const back = dir === "up" || dir === "left" || dir === "pageUp";
    const next = back ? index - step : index + step;
    if (next < 0) return { index: 0, hitEnd: false };
    if (next > last) return { index: last, hitEnd: true };
    return { index: next, hitEnd: false };
  }

  const cols = Math.max(1, grid.columns);
  const row = Math.floor(index / cols);
  const lastRow = Math.floor(last / cols);
  switch (dir) {
    case "left":
      return { index: Math.max(0, index - 1), hitEnd: false };
    case "right":
      return index < last ? { index: index + 1, hitEnd: false } : { index, hitEnd: true };
    case "up":
      return { index: index - cols >= 0 ? index - cols : index, hitEnd: false };
    case "down":
      if (index + cols <= last) return { index: index + cols, hitEnd: false };
      // The column is missing in a shorter last row: land on the last card.
      if (row < lastRow) return { index: last, hitEnd: false };
      return { index, hitEnd: true };
    case "pageUp":
      return { index: Math.max(index % cols, index - cols * perPage), hitEnd: false };
    case "pageDown": {
      const next = index + cols * perPage;
      if (next <= last) return { index: next, hitEnd: false };
      return { index: last, hitEnd: true };
    }
  }
  return { index, hitEnd: false };
}

/**
 * Where the cursor goes when the card it was on (index `at`) left the list together with
 * the cards at `removed` (their indices before the removal): onto the card that followed
 * the removed block — `at` minus the removed cards that stood before it — or the last card.
 * (Shift+→ twice + Delete keeps the cursor at the gap instead of jumping ahead.) −1 = empty.
 */
export function indexAfterRemoval(at: number, removed: readonly number[], count: number): number {
  if (count <= 0) return -1;
  const before = removed.filter((i) => i < at).length;
  return Math.max(0, Math.min(at - before, count - 1));
}

/** Inclusive index range between two cards (Shift+arrows / Shift+click). */
export function rangeBetween(a: number, b: number): number[] {
  const lo = Math.min(a, b);
  const hi = Math.max(a, b);
  return Array.from({ length: hi - lo + 1 }, (_, i) => lo + i);
}
