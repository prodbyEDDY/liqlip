// Pinned picture math (W6-E, src/windows/pin/pinMath.ts): wheel zoom, opacity, 100 % toggle,
// the scale a window shows, the menu's form and place.
import assert from "node:assert/strict";
import test from "node:test";
import { importTs } from "./helpers/bundle.mjs";

const p = await importTs("src/windows/pin/pinMath.ts");

test("wheel deltas in pixels: px, lines, pages", () => {
  assert.equal(p.wheelPixels(100, 0), 100);
  assert.equal(p.wheelPixels(-3, 1), -120);
  assert.equal(p.wheelPixels(1, 2), 800);
  assert.equal(p.wheelPixels(Number.NaN, 0), 0);
});

test("zoom: a notch is 10 %, up enlarges, clamped 10–400 %, crossing 100 % stops there", () => {
  assert.equal(p.zoomBy(2, -100), 2.2);
  assert.equal(p.zoomBy(2, 100), 1.818);
  assert.equal(p.zoomBy(3.9, -500), p.SCALE_MAX);
  assert.equal(p.zoomBy(0.11, 1000), p.SCALE_MIN);
  assert.equal(p.zoomBy(0.95, -100), 1, "0.95 → 1.045 stops at 100 %");
  assert.equal(p.zoomBy(1.05, 100), 1, "1.05 → 0.955 stops at 100 %");
  assert.equal(p.zoomBy(1, -100), 1.1, "from exactly 100 % it moves on");
  assert.equal(p.clampScale(Number.NaN), 1);
  assert.equal(p.clampScale(-2), 1);
});

test("opacity: 10 % a notch, 20–100 %", () => {
  assert.equal(p.fadeBy(1, 100), 0.9);
  assert.equal(p.fadeBy(0.9, -100), 1);
  assert.equal(p.fadeBy(0.25, 300), p.OPACITY_MIN);
  assert.equal(p.fadeBy(1, -500), p.OPACITY_MAX);
  assert.equal(p.fadeBy(0.5, 50), 0.45, "touchpad half-notch");
});

test("double click: 100 % ↔ the opening size", () => {
  assert.equal(p.toggleActual(0.42, 0.42), 1);
  assert.equal(p.toggleActual(1, 0.42), 0.42);
  assert.equal(p.toggleActual(2.5, 0.42), 1);
  assert.equal(p.toggleActual(1, 1), 1, "a pin that opened at 100 % stays");
});

test("double click remembers the zoom it left (W7-R4): 150 % → 100 % → 150 %", () => {
  // opened at 100 %, zoomed to 150 %
  let s = p.doubleClick(1.5, 1);
  assert.deepEqual(s, { scale: 1, back: 1.5 });
  s = p.doubleClick(s.scale, s.back);
  assert.deepEqual(s, { scale: 1.5, back: 1.5 });
  // opened shrunk (a big shot) and never zoomed: 42 % ↔ 100 %
  s = p.doubleClick(0.42, 0.42);
  assert.deepEqual(s, { scale: 1, back: 0.42 });
  assert.equal(p.doubleClick(s.scale, s.back).scale, 0.42);
  // at 100 % with nothing to go back to: stays at 100 %
  assert.deepEqual(p.doubleClick(1, 1), { scale: 1, back: 1 });
});

test("the scale a pin window shows (window − bleed, physical px per picture px)", () => {
  const b = 2 * p.PIN_BLEED;
  assert.equal(p.scaleOfWindow(1280 + b, 1.5, 1920), 1, "1280 logical × 1.5 = 1920 physical");
  assert.equal(p.scaleOfWindow(960 + b, 1, 1920), 0.5);
  assert.equal(p.scaleOfWindow(1290 + b, 1.5, 1920), 1, "within 2 % → 100 %");
  assert.equal(p.scaleOfWindow(10, 1, 1920), 1, "degenerate window");
  assert.equal(p.fitScale(1000, 800, 1, 400, 300), 1);
  assert.equal(p.fitScale(1000, 800, 1, 4000, 1000), 0.2);
});

test("corner radius: r16, smaller on a tiny pin, never under 4", () => {
  assert.equal(p.cornerRadius(400, 300), 16);
  assert.equal(p.cornerRadius(40, 200), 10);
  assert.equal(p.cornerRadius(8, 8), 4);
});

test("menu form: the list when it fits, else a capsule row, else a column", () => {
  assert.equal(p.menuMode({ w: 600, h: 400 }), "list");
  assert.equal(p.menuMode({ w: 300, h: 100 }), "row");
  assert.equal(p.menuMode({ w: 60, h: 300 }), "column");
  assert.equal(p.menuMode({ w: 30, h: 30 }), "row", "nothing fits → the row, clamped");
});

test("menu place: at the pointer, flipped and clamped inside the window", () => {
  const m = { w: 200, h: 140 };
  assert.deepEqual(p.placeMenu({ x: 10, y: 10 }, m, { w: 600, h: 400 }), { x: 10, y: 10 });
  assert.deepEqual(p.placeMenu({ x: 590, y: 390 }, m, { w: 600, h: 400 }), { x: 390, y: 250 }, "flips left and up");
  assert.deepEqual(p.placeMenu({ x: 100, y: 100 }, m, { w: 220, h: 160 }), { x: 16, y: 16 }, "clamped to the margin");
  assert.deepEqual(p.placeMenu({ x: 50, y: 50 }, m, { w: 100, h: 100 }), { x: 4, y: 4 }, "too small: top-left");
});
