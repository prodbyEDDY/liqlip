// Pinned picture math (SPEC §9.5, ARCHITECTURE §18.8) — pure, node-tested (tests/pinMath.test.mjs).
//
// Scale contract with W6-B (`shot_pin_resize({ scale })`): `scale` is ABSOLUTE — physical screen
// px per picture px; 1 = 100 % = pixel-exact (a pinned shot lies 1:1 over what it was taken
// from). Rust sets the window to round(picture × scale / monitor scale) logical px + PIN_BLEED on
// every side, around the cursor, within 10 %–400 %.

/** Transparent margin of a pin window around the picture (logical px): holds `--shadow-lift`
 *  (0 8px 22px: at 24 px below the picture it is < 1 % alpha — no visible cut). */
export const PIN_BLEED = 24;
export const SCALE_MIN = 0.1;
export const SCALE_MAX = 4;
export const OPACITY_MIN = 0.2;
export const OPACITY_MAX = 1;
/** One wheel notch (100 px) zooms by 10 %; a notch of Ctrl+wheel changes opacity by 10 %. */
const ZOOM_PER_NOTCH = 1.1;
const NOTCH_PX = 100;

/** `WheelEvent.deltaY` in pixels (deltaMode 0 = px, 1 = lines, 2 = pages; MDN WheelEvent). */
export function wheelPixels(deltaY: number, deltaMode: number): number {
  if (!Number.isFinite(deltaY)) return 0;
  return deltaMode === 1 ? deltaY * 40 : deltaMode === 2 ? deltaY * 800 : deltaY;
}

export function clampScale(s: number): number {
  if (!Number.isFinite(s) || s <= 0) return 1;
  return Math.round(Math.min(SCALE_MAX, Math.max(SCALE_MIN, s)) * 1000) / 1000;
}

/** Wheel zoom: up (negative delta) enlarges. Crossing 100 % stops at exactly 100 %. */
export function zoomBy(scale: number, deltaPx: number): number {
  const next = clampScale(scale * Math.pow(ZOOM_PER_NOTCH, -deltaPx / NOTCH_PX));
  if ((scale < 1 && next > 1) || (scale > 1 && next < 1)) return 1;
  return next;
}

/** Ctrl+wheel opacity, 20–100 %: up (negative delta) makes the picture more opaque. */
export function fadeBy(opacity: number, deltaPx: number): number {
  const base = Number.isFinite(opacity) ? opacity : 1;
  const next = base - deltaPx / (NOTCH_PX * 10);
  return Math.round(Math.min(OPACITY_MAX, Math.max(OPACITY_MIN, next)) * 100) / 100;
}

/** Double click: 100 % ↔ the size the pin opened at. */
export function toggleActual(scale: number, initial: number): number {
  return Math.abs(scale - 1) < 0.005 ? clampScale(initial) : 1;
}

/** The scale the window shows now: picture area (window − bleed) in physical px / picture px.
 *  Within 2 % of 100 % counts as 100 % (rounding of the window size). */
export function scaleOfWindow(viewWidth: number, dpr: number, pictureWidth: number): number {
  if (!(pictureWidth > 0) || !(viewWidth > 2 * PIN_BLEED) || !(dpr > 0)) return 1;
  const s = clampScale(((viewWidth - 2 * PIN_BLEED) * dpr) / pictureWidth);
  return Math.abs(s - 1) < 0.02 ? 1 : s;
}

/** Browser preview (no native window): 100 %, or smaller to fit 80 % of the page. */
export function fitScale(viewW: number, viewH: number, dpr: number, picW: number, picH: number): number {
  if (!(picW > 0) || !(picH > 0)) return 1;
  return clampScale(Math.min(1, (0.8 * viewW * dpr) / picW, (0.8 * viewH * dpr) / picH));
}

/** Corner radius of the picture: r16 (`--r-card`), smaller on a tiny pin (never a pill). */
export function cornerRadius(w: number, h: number): number {
  return Math.max(4, Math.min(16, Math.floor(Math.min(w, h) / 4)));
}

export interface Size {
  w: number;
  h: number;
}

/** Menu forms: the shelf-style list; a capsule of icon buttons (row / column) when the pin
 *  window is too small for the list — the menu can't leave its window. */
export type MenuMode = "list" | "row" | "column";
export const MENU_SIZE: Record<MenuMode, Size> = {
  list: { w: 200, h: 4 * 32 + 2 * 6 },
  row: { w: 4 * 32 + 8, h: 40 },
  column: { w: 40, h: 4 * 32 + 8 },
};
const MENU_PAD = 4;

const fits = (m: Size, view: Size) => m.w + 2 * MENU_PAD <= view.w && m.h + 2 * MENU_PAD <= view.h;

export function menuMode(view: Size): MenuMode {
  return (["list", "row", "column"] as const).find((m) => fits(MENU_SIZE[m], view)) ?? "row";
}

/** Place a menu at the pointer inside the window: flip left / up when it would leave it, then
 *  clamp to a 4 px margin (top-left wins when the window is smaller than the menu). */
export function placeMenu(at: { x: number; y: number }, menu: Size, view: Size): { x: number; y: number } {
  const axis = (p: number, size: number, room: number) => {
    const max = room - MENU_PAD - size;
    if (p <= max) return p; // fits after the pointer
    if (p - size >= MENU_PAD) return p - size; // flipped before it
    return max; // neither: as far as the window allows
  };
  const x = axis(at.x, menu.w, view.w);
  const y = axis(at.y, menu.h, view.h);
  return { x: Math.max(MENU_PAD, Math.round(x)), y: Math.max(MENU_PAD, Math.round(y)) };
}
