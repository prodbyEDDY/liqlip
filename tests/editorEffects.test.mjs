// "Hide" effects (W6-C): pure functions over RGBA. Nothing of the original shows through the
// edges (LT П-3: CSS blur on a cropped tile left a translucent fringe), every step is stronger than
// the previous one (LT: S = M), and the export draws with the very same `drawDoc` as the screen.
import assert from "node:assert/strict";
import test from "node:test";
import { importTs } from "./helpers/bundle.mjs";

const E = await importTs("src/lib/editor/effects.ts");

/** A w×h RGBA picture filled by `f(x, y) → [r, g, b]`. */
function pic(w, h, f) {
  const data = new Uint8ClampedArray(w * h * 4);
  for (let y = 0; y < h; y++)
    for (let x = 0; x < w; x++) {
      const [r, g, b] = f(x, y);
      data.set([r, g, b, 255], (y * w + x) * 4);
    }
  return { width: w, height: h, data };
}
const alphaAll255 = (px) => px.every((v, i) => i % 4 !== 3 || v === 255);

test("blur: every output pixel is opaque and the edges are computed from the real neighbours", () => {
  // white outside, black inside: a naive tile blur would leave the inner edge black-ish and
  // translucent; with the margin the edge mixes in the white neighbours
  const src = pic(60, 60, (x, y) => (x >= 20 && x < 40 && y >= 20 && y < 40 ? [0, 0, 0] : [255, 255, 255]));
  const r = { x: 20, y: 20, w: 20, h: 20 };
  const out = E.blur(src, r, 8);
  assert.equal(out.length, 20 * 20 * 4);
  assert.ok(alphaAll255(out), "no transparent fringe");
  const at = (x, y) => out[(y * 20 + x) * 4];
  assert.ok(at(0, 10) > 60, `the edge pixel took the outside into account (got ${at(0, 10)})`);
  assert.ok(at(10, 10) < at(0, 10), "the centre stays darker than the edge");
});

test("blur hides a sharp detail: a 1 px line spreads out", () => {
  const src = pic(64, 32, (x) => (x === 32 ? [255, 255, 255] : [0, 0, 0]));
  const out = E.blur(src, { x: 16, y: 8, w: 32, h: 16 }, E.blurRadius(3, 1));
  const row = Array.from({ length: 32 }, (_, x) => out[(8 * 32 + x) * 4]);
  assert.ok(Math.max(...row) < 80, `the line is no longer readable (peak ${Math.max(...row)})`);
});

test("pixelate: opaque, cells anchored at the area, edge cells as coarse as inner ones", () => {
  const src = pic(40, 40, (x, y) => [(x * 7) % 256, (y * 11) % 256, 128]);
  const r = { x: 10, y: 10, w: 13, h: 9 };
  const cell = 6;
  const out = E.pixelate(src, r, cell);
  assert.ok(alphaAll255(out));
  const px = (x, y) => Array.from(out.slice((y * r.w + x) * 4, (y * r.w + x) * 4 + 3));
  // one value per cell, including the partial cell at the right edge (x 12 of 13)
  assert.deepEqual(px(0, 0), px(5, 5));
  assert.deepEqual(px(12, 0), px(12, 5));
  assert.notDeepEqual(px(0, 0), px(6, 0));
  // the partial edge cell averaged its whole square (it reached into the margin)
  let sum = 0;
  for (let y = 10; y < 16; y++) for (let x = 22; x < 28; x++) sum += src.data[(y * 40 + x) * 4];
  assert.equal(px(12, 0)[0], Math.round(sum / 36));
});

test("strength: every size step is stronger (LT had S = M)", () => {
  const cells = [1, 2, 3, 4, 5].map((s) => E.pixelCell(s, 1));
  const radii = [1, 2, 3, 4, 5].map((s) => E.blurRadius(s, 1));
  for (let i = 1; i < 5; i++) {
    assert.ok(cells[i] > cells[i - 1], `cell ${cells}`);
    assert.ok(radii[i] > radii[i - 1], `radius ${radii}`);
  }
  assert.ok(cells[0] >= 6, "cell ≥ 6 px");
  assert.equal(E.blurRadius(3, 2), 2 * E.blurRadius(3, 1), "scaled by the unit (200 % screens)");
  assert.equal(E.effectMargin("blur", 3, 1), 2 * E.blurRadius(3, 1), "margin = 2 × radius");
});

// ── export = render ──────────────────────────────────────────────────────────

/** A recording 2D context: every method call and property write, in order. */
function recorder(log) {
  const target = {
    measureText: (t) => (log.push(["measureText", t]), { width: t.length * 5, actualBoundingBoxAscent: 7, actualBoundingBoxDescent: 0 }),
    getImageData: (x, y, w, h) => ({ width: w, height: h, data: new Uint8ClampedArray(w * h * 4) }),
  };
  return new Proxy(target, {
    get(t, k) {
      if (k in t) return t[k];
      return (...args) => log.push([String(k), ...args.map((a) => (typeof a === "object" && a !== null ? "[obj]" : a))]);
    },
    set(_t, k, v) {
      log.push(["=" + String(k), typeof v === "object" ? "[obj]" : v]);
      return true;
    },
  });
}

test("the export draws the document with the SAME drawDoc as the screen (WYSIWYG by construction)", async () => {
  const logs = [];
  globalThis.OffscreenCanvas = class {
    constructor(w, h) {
      this.width = w;
      this.height = h;
      this.log = [];
      logs.push(this.log);
    }
    getContext() {
      return recorder(this.log);
    }
  };
  globalThis.ImageData = class {
    constructor(data, w, h) {
      Object.assign(this, { data, width: w, height: h });
    }
  };
  const R = await importTs("src/lib/editor/render.ts", "src/lib/editor/export.ts");
  const doc = {
    v: 1,
    w: 200,
    h: 100,
    unit: 1,
    crop: null,
    objects: [
      { id: "x", kind: "redact", color: "#000000", r: { x: 5, y: 5, w: 30, h: 10 }, mode: "solid", size: 3 },
      { id: "s", kind: "spotlight", r: { x: 50, y: 10, w: 40, h: 40 }, shape: "ellipse" },
      { id: "m", kind: "marker", color: "#ffcc00", points: [[0, 50], [80, 50]], size: 3 },
      { id: "a", kind: "arrow", color: "#ff3b30", a: [10, 90], b: [120, 30], bend: 25, style: "double", size: 3 },
      { id: "r", kind: "rect", color: "#007aff", r: { x: 100, y: 10, w: 60, h: 40 }, fill: false, size: 2, round: true },
      { id: "t", kind: "text", color: "#ffffff", at: [20, 20], width: null, text: "Hi", fontSize: 18, style: "badge" },
      { id: "c", kind: "counter", color: "#ff3b30", at: [180, 80], n: null, size: 3 },
    ],
  };
  const measure = (t, size) => t.length * size * 0.5;
  const env = { measure, effects: new R.EffectCache(), images: new R.ImageCache() };
  const base = { width: 200, height: 100 };
  const rect = { x: 0, y: 0, w: 200, h: 100 };

  const flat = await R.flatten(base, doc, rect, env);
  assert.equal(flat.width, 200);
  assert.equal(flat.data.length, 200 * 100 * 4);
  const exportLog = logs[0];
  // the export canvas: translate, the source 1:1, then the document
  assert.deepEqual(exportLog.slice(0, 2), [
    ["translate", -0, -0],
    ["drawImage", "[obj]", 0, 0],
  ]);
  const screenLog = [];
  R.drawDoc(recorder(screenLog), doc, { measure, base, effects: env.effects, images: env.images, skip: null, area: rect });
  assert.deepEqual(exportLog.slice(2), screenLog, "identical drawing calls, nothing else");
  assert.ok(screenLog.some((c) => c[0] === "=globalCompositeOperation" && c[1] === "multiply"), "marker multiplies");
  assert.ok(screenLog.some((c) => c[0] === "clip"), "spotlight clips the dim to the outside");
  delete globalThis.OffscreenCanvas;
  delete globalThis.ImageData;
});
