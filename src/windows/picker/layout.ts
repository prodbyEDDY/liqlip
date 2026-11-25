// Grid geometry of the shelf — port of Liquid Tools `shotlibrary/layout.ts` (the 2026-09-22
// regular grid): square previews + a 36 px meta row, 10 px gutters, as many ≥172 px columns as
// fit; the top/bottom strip (and the centre "one row" sheet) is a single horizontal row whose
// preview height follows the strip height. Geometry is a pure function of the item count and
// the viewport, so appending a page never moves a placed card. Liquid Tools' "compact density"
// parameter was dead code there and is dropped. Pure — node-tested (tests/pickerLayout).

/** Meta row under the preview (app icon · title · format badge). */
export const CARD_FOOTER = 36;
/** Gutter between cards. */
export const GAP = 10;
/** Minimum column width (and the strip's maximum preview height). */
export const MIN_TILE = 172;
/** Rows/columns rendered beyond the viewport on each side (virtualization margin). */
export const OVERSCAN = 300;

export interface Tile {
  index: number;
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface Geometry {
  count: number;
  horizontal: boolean;
  /** Columns of the vertical grid (1 row of `count` tiles in the strip). */
  columns: number;
  tileW: number;
  tileH: number;
  previewH: number;
  /** Distance between the starts of two neighbouring tiles along each axis. */
  strideX: number;
  strideY: number;
  /** Size of the whole laid-out content. */
  width: number;
  height: number;
}

/** Lay out `count` tiles in a viewport of `width` × `height` (px, padding already removed). */
export function geometry(count: number, width: number, height: number, horizontal: boolean): Geometry {
  const n = Math.max(0, Math.floor(count));
  const fit = Math.max(1, Math.floor((width + GAP) / (MIN_TILE + GAP)));
  const tileWidth = Math.max(80, (width - (fit - 1) * GAP) / fit);
  const previewH = horizontal ? Math.max(48, Math.min(MIN_TILE, height - CARD_FOOTER - 8)) : tileWidth;
  const tileW = horizontal ? previewH : tileWidth;
  const tileH = previewH + CARD_FOOTER;
  const columns = horizontal ? Math.max(1, n) : fit;
  const strideX = tileW + GAP;
  const strideY = tileH + GAP;
  return {
    count: n,
    horizontal,
    columns,
    tileW,
    tileH,
    previewH,
    strideX,
    strideY,
    width: horizontal ? Math.max(0, n * strideX - GAP) : width,
    height: horizontal ? height : Math.max(0, Math.ceil(n / columns) * strideY - GAP),
  };
}

/** Position of tile `index` (also valid past `count`, for skeleton placeholders). */
export function tileRect(g: Geometry, index: number): Tile {
  if (g.horizontal) return { index, x: index * g.strideX, y: 0, width: g.tileW, height: g.tileH };
  return {
    index,
    x: (index % g.columns) * g.strideX,
    y: Math.floor(index / g.columns) * g.strideY,
    width: g.tileW,
    height: g.tileH,
  };
}

/** Half-open index range `[start, end)` of the tiles within `offset … offset + extent` along
 *  the scroll axis, widened by `margin` px on both sides (viewport ± 300 px by default). */
export function visibleRange(g: Geometry, offset: number, extent: number, margin = OVERSCAN): [number, number] {
  if (g.count === 0) return [0, 0];
  if (g.horizontal) {
    const start = Math.max(0, Math.ceil((offset - margin - g.tileW) / g.strideX));
    const end = Math.min(g.count, Math.floor((offset + extent + margin) / g.strideX) + 1);
    return [Math.min(start, end), end];
  }
  const firstRow = Math.max(0, Math.ceil((offset - margin - g.tileH) / g.strideY));
  const lastRow = Math.floor((offset + extent + margin) / g.strideY);
  const start = firstRow * g.columns;
  const end = Math.min(g.count, (lastRow + 1) * g.columns);
  return [Math.min(start, end), end];
}

/** Indices of the tiles whose leading edge lies inside the viewport, in order — what
 *  `Ctrl+1…9` addresses ("the N-th visible card"). A tile counts once at least half of it
 *  is on screen. */
export function onScreen(g: Geometry, offset: number, extent: number, limit = 9): number[] {
  const out: number[] = [];
  const [start, end] = visibleRange(g, offset, extent, 0);
  const size = g.horizontal ? g.tileW : g.tileH;
  for (let i = start; i < end && out.length < limit; i++) {
    const t = tileRect(g, i);
    const lead = g.horizontal ? t.x : t.y;
    if (lead + size / 2 >= offset && lead + size / 2 <= offset + extent) out.push(i);
  }
  return out;
}

/** Scroll offset that brings tile `index` fully into view (unchanged if it already is). */
export function scrollFor(g: Geometry, index: number, offset: number, extent: number): number {
  const t = tileRect(g, index);
  const start = g.horizontal ? t.x : t.y;
  const end = start + (g.horizontal ? t.width : t.height);
  if (start < offset) return Math.max(0, start);
  if (end > offset + extent) return Math.max(0, end - extent);
  return offset;
}
