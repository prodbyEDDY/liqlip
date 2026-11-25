// 2-D grid navigation (W1-E): rows/columns, edges, a short last row, Home/End, PgUp/PgDn,
// the single-row strip, and the "hit the end → load more" signal.
import assert from "node:assert/strict";
import test from "node:test";
import { importTs } from "./helpers/bundle.mjs";

const { navigate, navGrid, rangeBetween, indexAfterRemoval, geometry } = await importTs("src/windows/picker/gridNav.ts", "src/windows/picker/layout.ts");

// 4 columns, 10 items: rows [0-3] [4-7] [8-9]
const grid = { count: 10, columns: 4, horizontal: false, perPage: 2 };
const go = (i, dir, g = grid) => navigate(i, dir, g);

test("arrows move by rows and cards; edges hold", () => {
  assert.deepEqual(go(5, "up"), { index: 1, hitEnd: false });
  assert.deepEqual(go(1, "up"), { index: 1, hitEnd: false }, "top row stays");
  assert.deepEqual(go(1, "down"), { index: 5, hitEnd: false });
  assert.deepEqual(go(5, "left"), { index: 4, hitEnd: false });
  assert.deepEqual(go(4, "left"), { index: 3, hitEnd: false }, "left wraps to the previous row end");
  assert.deepEqual(go(0, "left"), { index: 0, hitEnd: false });
  assert.deepEqual(go(3, "right"), { index: 4, hitEnd: false });
});

test("down into a shorter last row lands on the last card; the last row signals the end", () => {
  assert.deepEqual(go(6, "down"), { index: 9, hitEnd: false });
  assert.deepEqual(go(4, "down"), { index: 8, hitEnd: false });
  assert.deepEqual(go(8, "down"), { index: 8, hitEnd: true });
  assert.deepEqual(go(9, "right"), { index: 9, hitEnd: true });
});

test("Home / End / PgUp / PgDn", () => {
  assert.deepEqual(go(6, "home"), { index: 0, hitEnd: false });
  assert.deepEqual(go(2, "end"), { index: 9, hitEnd: true });
  assert.deepEqual(go(1, "pageDown"), { index: 9, hitEnd: false }, "1 + 4×2 = 9 exists");
  assert.deepEqual(go(2, "pageDown"), { index: 9, hitEnd: true }, "clamped to the last card");
  assert.deepEqual(go(9, "pageUp"), { index: 1, hitEnd: false });
  assert.deepEqual(go(5, "pageUp"), { index: 1, hitEnd: false }, "keeps the column on the top row");
});

test("no current card: any move selects the first (End the last); empty grid → -1", () => {
  assert.deepEqual(go(-1, "down"), { index: 0, hitEnd: false });
  assert.deepEqual(go(-1, "end"), { index: 9, hitEnd: false });
  assert.deepEqual(navigate(0, "down", { ...grid, count: 0 }), { index: -1, hitEnd: false });
});

test("strip: ←/→ primary, ↑/↓ alias them, PgUp/PgDn by a screen of tiles", () => {
  const strip = { count: 20, columns: 20, horizontal: true, perPage: 4 };
  assert.deepEqual(go(3, "right", strip), { index: 4, hitEnd: false });
  assert.deepEqual(go(3, "down", strip), { index: 4, hitEnd: false });
  assert.deepEqual(go(3, "up", strip), { index: 2, hitEnd: false });
  assert.deepEqual(go(0, "left", strip), { index: 0, hitEnd: false });
  assert.deepEqual(go(19, "right", strip), { index: 19, hitEnd: true });
  assert.deepEqual(go(3, "pageDown", strip), { index: 7, hitEnd: false });
  assert.deepEqual(go(2, "pageUp", strip), { index: 0, hitEnd: false });
});

test("navGrid derives columns and the page step from the layout", () => {
  const g = geometry(100, 728, 480, false);
  const n = navGrid(g, 480);
  assert.equal(n.columns, 4);
  assert.equal(n.perPage, Math.floor((480 + 10) / g.strideY));
  const s = navGrid(geometry(50, 800, 200, true), 800);
  assert.equal(s.horizontal, true);
  assert.ok(s.perPage >= 4);
});

test("rangeBetween is inclusive in both directions", () => {
  assert.deepEqual(rangeBetween(2, 5), [2, 3, 4, 5]);
  assert.deepEqual(rangeBetween(5, 2), [2, 3, 4, 5]);
  assert.deepEqual(rangeBetween(3, 3), [3]);
});

test("cursor after a removal lands at the gap, not past it (W3)", () => {
  // Shift+→ twice from 10, cursor on 12, Delete: the card that followed (old 13) is now 10.
  assert.equal(indexAfterRemoval(12, [10, 11, 12], 297), 10);
  // One card: its successor slides into its place.
  assert.equal(indexAfterRemoval(5, [5], 20), 5);
  // Removed cards after the cursor don't move it.
  assert.equal(indexAfterRemoval(3, [3, 7, 8], 9), 3);
  // At the end: the new last card; everything gone: −1.
  assert.equal(indexAfterRemoval(9, [8, 9], 8), 7);
  assert.equal(indexAfterRemoval(0, [0, 1], 0), -1);
  // The cursor wasn't in the list: the first card.
  assert.equal(indexAfterRemoval(-1, [2], 5), 0);
});
