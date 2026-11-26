// "Hide" effects (SPEC §9.3 tool B) as pure functions over RGBA arrays (ARCHITECTURE §18.6).
// They read the SOURCE pixels of the area grown by a margin (2 × blur radius, one pixelate
// cell), so the result at the edges is computed from the real neighbours and every output
// pixel is opaque — nothing of the original shows through a translucent fringe (Liquid Tools
// defect П-3: CSS `filter: blur()` on a cropped tile). Strength: pixel cell
// max(6, 4·TABLE[size]·unit), blur radius 3·TABLE[size]·unit (LT had 2/4/7 px and S = M).
// The renderer and the export call the same functions, so the export matches the preview
// pixel for pixel. Node-tested (tests/editorEffects.test.mjs).
import { TABLE, type Rect, type Size } from "./doc";

/** An RGBA buffer (`ImageData` satisfies it). */
export interface Pixels {
  width: number;
  height: number;
  data: Uint8ClampedArray | Uint8Array;
}

export type EffectMode = "pixelate" | "blur";

export function pixelCell(size: Size, unit: number): number {
  return Math.max(6, Math.round(4 * TABLE[size - 1] * unit));
}

export function blurRadius(size: Size, unit: number): number {
  return Math.max(1, Math.round(3 * TABLE[size - 1] * unit));
}

/** Margin of source pixels the effect needs around its rectangle. */
export function effectMargin(mode: EffectMode, size: Size, unit: number): number {
  return mode === "blur" ? 2 * blurRadius(size, unit) : pixelCell(size, unit);
}

/** Integer rectangle covering `r` (source px). */
export function pixelRect(r: Rect): Rect {
  const x = Math.floor(r.x),
    y = Math.floor(r.y);
  return { x, y, w: Math.max(1, Math.ceil(r.x + r.w) - x), h: Math.max(1, Math.ceil(r.y + r.h) - y) };
}

/**
 * Pixelate: `src` holds the area plus margin; `r` is the target rectangle inside `src`
 * (integer px). Cells are anchored at `r`'s corner; a cell that sticks out of `r` averages its
 * whole square from `src` (margin included), so edge cells are as coarse as inner ones.
 * Returns `r.w × r.h` RGBA, alpha 255.
 */
export function pixelate(src: Pixels, r: Rect, cell: number): Uint8ClampedArray {
  const out = new Uint8ClampedArray(r.w * r.h * 4);
  const W = src.width,
    H = src.height,
    d = src.data;
  for (let cy = 0; cy < r.h; cy += cell) {
    for (let cx = 0; cx < r.w; cx += cell) {
      const sx0 = Math.max(0, r.x + cx),
        sy0 = Math.max(0, r.y + cy);
      const sx1 = Math.min(W, r.x + cx + cell),
        sy1 = Math.min(H, r.y + cy + cell);
      let R = 0,
        G = 0,
        B = 0,
        n = 0;
      for (let y = sy0; y < sy1; y++) {
        let i = (y * W + sx0) * 4;
        for (let x = sx0; x < sx1; x++, i += 4) {
          R += d[i];
          G += d[i + 1];
          B += d[i + 2];
          n++;
        }
      }
      if (!n) n = 1;
      const rr = R / n,
        gg = G / n,
        bb = B / n;
      const ex = Math.min(r.w, cx + cell),
        ey = Math.min(r.h, cy + cell);
      for (let y = cy; y < ey; y++) {
        let o = (y * r.w + cx) * 4;
        for (let x = cx; x < ex; x++, o += 4) {
          out[o] = rr;
          out[o + 1] = gg;
          out[o + 2] = bb;
          out[o + 3] = 255;
        }
      }
    }
  }
  return out;
}

/** One box-blur pass along a row/column with clamped edges (running sum, O(n)). */
function boxPass(src: Float32Array, dst: Float32Array, W: number, H: number, rad: number, horizontal: boolean): void {
  const len = horizontal ? W : H;
  const lines = horizontal ? H : W;
  const step = horizontal ? 4 : W * 4;
  const win = rad * 2 + 1;
  for (let line = 0; line < lines; line++) {
    const base = horizontal ? line * W * 4 : line * 4;
    for (let c = 0; c < 3; c++) {
      let sum = 0;
      for (let k = -rad; k <= rad; k++) sum += src[base + Math.min(len - 1, Math.max(0, k)) * step + c];
      for (let i = 0; i < len; i++) {
        dst[base + i * step + c] = sum / win;
        const add = Math.min(len - 1, i + rad + 1),
          rem = Math.max(0, i - rad);
        sum += src[base + add * step + c] - src[base + rem * step + c];
      }
    }
  }
}

/**
 * Blur: three box passes each way (≈ Gaussian) over the WHOLE `src` (area + margin), then the
 * target rectangle `r` is cut out. Returns `r.w × r.h` RGBA, alpha 255.
 */
export function blur(src: Pixels, r: Rect, radius: number): Uint8ClampedArray {
  const W = src.width,
    H = src.height;
  let a = new Float32Array(W * H * 4);
  for (let i = 0; i < a.length; i++) a[i] = src.data[i];
  let b = new Float32Array(a.length);
  const rad = Math.max(1, Math.round(radius / 2));
  for (let pass = 0; pass < 3; pass++) {
    boxPass(a, b, W, H, rad, true);
    [a, b] = [b, a];
    boxPass(a, b, W, H, rad, false);
    [a, b] = [b, a];
  }
  const out = new Uint8ClampedArray(r.w * r.h * 4);
  for (let y = 0; y < r.h; y++) {
    const sy = Math.min(H - 1, Math.max(0, r.y + y));
    for (let x = 0; x < r.w; x++) {
      const sx = Math.min(W - 1, Math.max(0, r.x + x));
      const i = (sy * W + sx) * 4,
        o = (y * r.w + x) * 4;
      out[o] = a[i];
      out[o + 1] = a[i + 1];
      out[o + 2] = a[i + 2];
      out[o + 3] = 255;
    }
  }
  return out;
}

/** Run an effect: `src` = the area grown by `effectMargin` (clamped to the picture), `r` = the
 *  target inside it. */
export function applyEffect(mode: EffectMode, src: Pixels, r: Rect, size: Size, unit: number): Uint8ClampedArray {
  return mode === "blur" ? blur(src, r, blurRadius(size, unit)) : pixelate(src, r, pixelCell(size, unit));
}
