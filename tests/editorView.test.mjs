// Zoom and pan of the editor window (W6-C, SPEC §9.3): fit by default (never above 100 %), zoom
// around the pointer, fixed steps for Ctrl+= / Ctrl+-, panning never loses the picture.
import assert from "node:assert/strict";
import test from "node:test";
import { importTs } from "./helpers/bundle.mjs";

const V = await importTs("src/lib/editor/view.ts");

test("fit: the whole picture, centred, with a margin; small pictures stay at 100 %", () => {
  const big = V.fitView({ x: 0, y: 0, w: 3840, h: 2160 }, 1000, 600);
  assert.ok(big.z < 1);
  const [x0, y0] = V.toScreen(big, [0, 0]);
  const [x1, y1] = V.toScreen(big, [3840, 2160]);
  assert.ok(x0 >= V.FIT_MARGIN - 1e-9 && x1 <= 1000 - V.FIT_MARGIN + 1e-9 && y0 >= 0 && y1 <= 600);
  assert.ok(Math.abs((x0 + x1) / 2 - 500) < 1e-9 && Math.abs((y0 + y1) / 2 - 300) < 1e-9, "centred");
  assert.equal(V.fitView({ x: 0, y: 0, w: 200, h: 100 }, 1000, 600).z, 1, "no blurry upscaling");
  // a crop is fitted, not the whole picture
  const crop = V.fitView({ x: 1000, y: 500, w: 400, h: 300 }, 1000, 600);
  assert.deepEqual(V.toScreen(crop, [1200, 650]).map(Math.round), [500, 300]);
});

test("zoom keeps the point under the pointer still; source ↔ screen round trip", () => {
  const v = { z: 0.5, tx: 30, ty: -12 };
  const p = V.toSource(v, 400, 250);
  const z = V.zoomAt(v, 2, 400, 250);
  assert.deepEqual(V.toScreen(z, p).map((n) => Math.round(n * 1e6) / 1e6), [400, 250]);
  assert.equal(V.zoomAt(v, 100, 0, 0).z, V.ZOOM_MAX, "clamped");
});

test("Ctrl+= / Ctrl+- step through fixed levels (100 % is one of them)", () => {
  assert.equal(V.stepZoom(1, 1), 1.25);
  assert.equal(V.stepZoom(1, -1), 0.75);
  assert.equal(V.stepZoom(0.8, -1), 0.75, "from between two levels to the next one down");
  assert.equal(V.stepZoom(0.8, 1), 1);
  assert.equal(V.stepZoom(V.ZOOM_MAX, 1), V.ZOOM_MAX);
  assert.equal(V.stepZoom(V.ZOOM_MIN, -1), V.ZOOM_MIN);
});

test("panning never pushes the picture out of the window", () => {
  const content = { x: 0, y: 0, w: 1000, h: 800 };
  const lost = V.clampPan({ z: 1, tx: -5000, ty: 5000 }, content, 800, 600);
  const [, y0] = V.toScreen(lost, [0, 0]);
  const [x1] = V.toScreen(lost, [1000, 800]);
  assert.ok(x1 >= 48 - 1e-9, "a strip stays visible on the left");
  assert.ok(y0 <= 600 - 48 + 1e-9, "and at the bottom");
});
