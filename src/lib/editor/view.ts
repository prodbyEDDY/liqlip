// Zoom and pan of the editor window (SPEC §9.3 "Масштаб и панорама"): screen = source × z + t
// (CSS px of the canvas box). Fit by default; Ctrl+wheel / Ctrl+= / Ctrl+- step through fixed
// zoom levels around the pointer; Ctrl+0 fits, Ctrl+1 is 100 %. Pure, node-tested
// (tests/editorView.test.mjs). The overlay host has no zoom: z = canvas width / frame width.
import type { Pt, Rect } from "./doc";

export interface View {
  z: number;
  tx: number;
  ty: number;
}

export const ZOOM_STEPS = [0.05, 0.1, 0.17, 0.25, 0.33, 0.5, 0.67, 0.75, 1, 1.25, 1.5, 2, 3, 4, 6, 8] as const;
export const ZOOM_MIN = ZOOM_STEPS[0];
export const ZOOM_MAX = ZOOM_STEPS[ZOOM_STEPS.length - 1];
/** Breathing room around a fitted picture, CSS px. */
export const FIT_MARGIN = 24;

/** Fit `content` into a `cw × ch` box, centred; never above 100 % (no blurry upscaling). */
export function fitView(content: Rect, cw: number, ch: number, margin = FIT_MARGIN): View {
  const aw = Math.max(1, cw - margin * 2),
    ah = Math.max(1, ch - margin * 2);
  const z = Math.max(ZOOM_MIN, Math.min(1, aw / Math.max(1, content.w), ah / Math.max(1, content.h)));
  return centered(content, z, cw, ch);
}

/** `content` at zoom `z`, centred in the box. */
export function centered(content: Rect, z: number, cw: number, ch: number): View {
  return { z, tx: cw / 2 - (content.x + content.w / 2) * z, ty: ch / 2 - (content.y + content.h / 2) * z };
}

export function toSource(v: View, sx: number, sy: number): Pt {
  return [(sx - v.tx) / v.z, (sy - v.ty) / v.z];
}

export function toScreen(v: View, p: Pt): Pt {
  return [p[0] * v.z + v.tx, p[1] * v.z + v.ty];
}

/** Zoom to `z` keeping the screen point `(sx, sy)` over the same source point. */
export function zoomAt(v: View, z: number, sx: number, sy: number): View {
  const nz = Math.max(ZOOM_MIN, Math.min(ZOOM_MAX, z));
  const [px, py] = toSource(v, sx, sy);
  return { z: nz, tx: sx - px * nz, ty: sy - py * nz };
}

/** The next fixed zoom level up (`dir > 0`) or down. */
export function stepZoom(z: number, dir: number): number {
  if (dir > 0) return ZOOM_STEPS.find((s) => s > z * 1.001) ?? ZOOM_MAX;
  for (let i = ZOOM_STEPS.length - 1; i >= 0; i--) if (ZOOM_STEPS[i] < z / 1.001) return ZOOM_STEPS[i];
  return ZOOM_MIN;
}

/** Keep at least `keep` CSS px of the picture inside the box (panning never loses it). */
export function clampPan(v: View, content: Rect, cw: number, ch: number, keep = 48): View {
  const x0 = content.x * v.z + v.tx,
    x1 = (content.x + content.w) * v.z + v.tx;
  const y0 = content.y * v.z + v.ty,
    y1 = (content.y + content.h) * v.z + v.ty;
  let { tx, ty } = v;
  const kx = Math.min(keep, (x1 - x0) / 2),
    ky = Math.min(keep, (y1 - y0) / 2);
  if (x1 < kx) tx += kx - x1;
  if (x0 > cw - kx) tx -= x0 - (cw - kx);
  if (y1 < ky) ty += ky - y1;
  if (y0 > ch - ky) ty -= y0 - (ch - ky);
  return { z: v.z, tx, ty };
}
