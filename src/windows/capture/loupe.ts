// The magnifier of the capture overlay (ARCHITECTURE §18.7; W6-D) — the ShareX model ported from
// Liquid Tools (`Capture.svelte:1560-1635`, research §1.3): a circle of `count × count` frame
// pixels around the cursor pixel, nearest-neighbour, `cell` view px per pixel, a pixel grid, a
// crosshair one cell thick and the centre pixel outlined. Unlike LT the lens keeps its size
// (~150 view px) and the wheel changes the magnification (SPEC §9.2: "колесо — увеличение").
// The chrome is monochrome (white/black) — the colour inside the lens is the user's screen.
// Pure maths here — node-tested (tests/captureLoupe.test.mjs); `Loupe.svelte` draws.
import type { Rect } from "$lib/ipc/types";
import type { Point } from "./geometry";

/** The lens diameter the pixel count aims at (view px). */
export const LOUPE_SPAN = 150;
/** Magnification steps: view px per frame pixel. */
export const LOUPE_ZOOMS: readonly number[] = [4, 5, 6, 8, 10, 12, 15, 20, 25, 30];
/** ShareX default: 15 pixels × 10 px. */
export const LOUPE_DEFAULT_ZOOM = 10;

/** How many frame pixels the lens shows across: odd (a true centre pixel), at least 3. */
export function loupeCount(cell: number): number {
  let n = Math.floor(LOUPE_SPAN / cell);
  if (n % 2 === 0) n -= 1;
  return Math.max(3, n);
}

/** The next magnification step (`dir` 1 — closer, −1 — farther), clamped to the list. */
export function zoomBy(cell: number, dir: 1 | -1): number {
  const i = LOUPE_ZOOMS.findIndex((z) => z >= cell);
  const at = i < 0 ? LOUPE_ZOOMS.length - 1 : LOUPE_ZOOMS[i] === cell ? i : dir > 0 ? i - 1 : i;
  return LOUPE_ZOOMS[Math.min(LOUPE_ZOOMS.length - 1, Math.max(0, at + dir))];
}

/** Frame pixels the lens draws: `count × count` centred on the cursor pixel (may leave the frame —
 *  those cells show the lens background). */
export function loupeSource(pixel: Point, count: number): Rect {
  const half = (count - 1) / 2;
  return { x: pixel.x - half, y: pixel.y - half, w: count, h: count };
}
