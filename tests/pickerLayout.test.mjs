// Shelf grid geometry (W1-E) — ported from Liquid Tools tests/libraryLayout.test.mjs onto the
// Liqlip layout API (geometry / tileRect / visibleRange) + the new helpers.
import assert from "node:assert/strict";
import test from "node:test";
import { importTs } from "./helpers/bundle.mjs";

const { geometry, tileRect, visibleRange, onScreen, scrollFor, CARD_FOOTER, GAP, PREVIEW_RATIO } = await importTs(
  "src/windows/picker/layout.ts",
);

const tiles = (g) => Array.from({ length: g.count }, (_, i) => tileRect(g, i));

test("a 360 px shelf fits two cards; previews keep the 4:3 ratio", () => {
  const g = geometry(3, 360, 600, false);
  assert.equal(g.columns, 2);
  const t = tiles(g);
  assert.equal(t[0].width, 176);
  assert.equal(t[1].x, 184);
  assert.equal(t[0].height - CARD_FOOTER, Math.round(t[0].width * PREVIEW_RATIO));
  assert.equal(t[2].y, t[0].height + GAP);
});

test("centre sheet (728 px) has four columns; side panel (372 px) two", () => {
  assert.equal(geometry(10, 728, 480, false).columns, 4);
  assert.equal(geometry(10, 372, 700, false).columns, 2);
});

test("regular grid: cards never overlap and stay inside the width", () => {
  const g = geometry(23, 720, 500, false);
  const t = tiles(g);
  for (const a of t) {
    assert.ok(a.x >= 0 && a.x + a.width <= 720.01);
    for (const b of t) {
      if (b.index >= a.index) continue;
      const overlap = a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;
      assert.ok(!overlap, `${a.index} overlaps ${b.index}`);
    }
  }
  assert.equal(g.height, Math.ceil(23 / g.columns) * g.strideY - GAP);
});

test("ten thousand records render only the viewport ± 300 px", () => {
  const g = geometry(10_000, 720, 500, false);
  const [start, end] = visibleRange(g, g.height / 2, 500);
  assert.ok(end - start < 40, `rendered ${end - start}`);
  assert.ok(start > 0 && end < 10_000);
  for (let i = start; i < end; i++) {
    const t = tileRect(g, i);
    assert.ok(t.y + t.height >= g.height / 2 - 300 && t.y <= g.height / 2 + 500 + 300);
  }
});

test("horizontal strip: one row, preview follows the strip height, virtualizes along x", () => {
  const g = geometry(1000, 720, 160, true);
  const t = tiles(g);
  assert.equal(t[0].width, t[1].width);
  assert.ok(t.every((x) => x.y === 0 && x.height <= 160));
  assert.equal(g.previewH, 160 - CARD_FOOTER - 8);
  const [start, end] = visibleRange(g, 4000, 720);
  assert.ok(end - start < 25);
  const capped = geometry(5, 720, 400, true);
  assert.equal(capped.previewH, 172, "strip previews are capped at 172 px");
});

test("appending a page does not move already placed cards", () => {
  const a = geometry(40, 360, 600, false);
  const b = geometry(80, 360, 600, false);
  for (let i = 0; i < 40; i++) assert.deepEqual(tileRect(a, i), tileRect(b, i));
  const h1 = geometry(40, 800, 200, true);
  const h2 = geometry(80, 800, 200, true);
  for (let i = 0; i < 40; i++) assert.deepEqual(tileRect(h1, i), tileRect(h2, i));
});

test("onScreen lists the first nine cards in view (for Ctrl+1…9)", () => {
  const g = geometry(100, 728, 480, false);
  // Two rows whose middles are in a 340 px viewport; a third row would add a 9th card.
  assert.deepEqual(onScreen(g, 0, 340), [0, 1, 2, 3, 4, 5, 6, 7]);
  assert.deepEqual(onScreen(g, 0, 480), [0, 1, 2, 3, 4, 5, 6, 7, 8], "capped at nine");
  const scrolled = onScreen(g, g.strideY, 340);
  assert.equal(scrolled[0], 4);
  assert.ok(scrolled.length <= 9);
});

test("scrollFor brings a tile fully into view and leaves visible tiles alone", () => {
  const g = geometry(100, 728, 480, false);
  assert.equal(scrollFor(g, 0, 0, 480), 0);
  const far = tileRect(g, 40);
  assert.equal(scrollFor(g, 40, 0, 480), far.y + far.height - 480);
  assert.equal(scrollFor(g, 0, 900, 480), 0);
});
