// Scroll-edge fades (W3-D): src/lib/scrollFade.ts::fadeEdges — which edges of a scroller
// hide content (design-system §7).
import assert from "node:assert/strict";
import test from "node:test";
import { importTs } from "./helpers/bundle.mjs";

const { fadeEdges } = await importTs("src/lib/scrollFade.ts");

test("fadeEdges: none when it fits, end at the top, both mid-way, start at the bottom", () => {
  assert.equal(fadeEdges(0, 200, 200), null);
  assert.equal(fadeEdges(0, 200, 150), null);
  assert.equal(fadeEdges(0, 200, 600), "end");
  assert.equal(fadeEdges(120, 200, 600), "both");
  assert.equal(fadeEdges(400, 200, 600), "start");
});

test("fadeEdges ignores sub-pixel leftovers (zoom, 150 % scale)", () => {
  assert.equal(fadeEdges(0.6, 200, 200.8), null);
  assert.equal(fadeEdges(399.4, 200, 600), "start");
});
