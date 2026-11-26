// Crop (SPEC §9.3 tool C, editor window only): a non-destructive rectangle in source px. A drag
// on a handle resizes it, inside moves it, outside draws a new one; it always stays inside the
// picture. `Enter` applies (`doc.crop`), `Esc` cancels, `C` again returns the full picture
// (interaction.ts). Pure.
import type { Pt, Rect } from "../doc";
import { dragBox } from "../geometry";
import type { HandleId } from "../hit";
import { resizeBox } from "./transform";
import type { Mods } from "./create";

export type CropGrip = HandleId | "move" | "new";

export function fullRect(w: number, h: number): Rect {
  return { x: 0, y: 0, w, h };
}

export function isFull(r: Rect, w: number, h: number): boolean {
  return Math.round(r.x) <= 0 && Math.round(r.y) <= 0 && Math.round(r.x + r.w) >= w && Math.round(r.y + r.h) >= h;
}

/** Keep `r` inside `0..w × 0..h` with at least `min` px each way (a move keeps its size). */
export function clampCrop(r: Rect, w: number, h: number, min: number, move = false): Rect {
  if (move) {
    const cw = Math.min(r.w, w),
      ch = Math.min(r.h, h);
    return { x: Math.max(0, Math.min(w - cw, r.x)), y: Math.max(0, Math.min(h - ch, r.y)), w: cw, h: ch };
  }
  const x0 = Math.max(0, Math.min(w - min, r.x)),
    y0 = Math.max(0, Math.min(h - min, r.y));
  const x1 = Math.min(w, Math.max(x0 + min, r.x + r.w)),
    y1 = Math.min(h, Math.max(y0 + min, r.y + r.h));
  return { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
}

/** The crop rectangle after a drag from `start` to `cur` on `grip` (whole source px). */
export function dragCrop(orig: Rect, grip: CropGrip, start: Pt, cur: Pt, w: number, h: number, mods: Mods, min: number): Rect {
  const dx = cur[0] - start[0],
    dy = cur[1] - start[1];
  let r: Rect;
  if (grip === "move") r = clampCrop({ ...orig, x: orig.x + dx, y: orig.y + dy }, w, h, min, true);
  else if (grip === "new") r = clampCrop(dragBox(start, cur, mods.shift, mods.alt), w, h, min);
  else r = clampCrop(resizeBox(orig, grip, dx, dy, mods.shift), w, h, min);
  return roundRect(r);
}

function roundRect(r: Rect): Rect {
  const x = Math.round(r.x),
    y = Math.round(r.y);
  return { x, y, w: Math.round(r.x + r.w) - x, h: Math.round(r.y + r.h) - y };
}
