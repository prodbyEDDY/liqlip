// Window picking of the capture overlay (W6-D, ARCHITECTURE §18.7) — the Liquid Tools tests of
// `captureTargets.ts` ported as is (LT `tests/captureTargets.test.mjs`, research §1.19).
import assert from "node:assert/strict";
import test from "node:test";
import { importTs } from "./helpers/bundle.mjs";

const { targetAt } = await importTs("src/windows/capture/captureTargets.ts");

const viewport = { x: 280, y: 200, w: 390, h: 640 };
const page = { x: 10, y: 100, w: 1100, h: 760, children: [viewport] };
const browser = { x: 10, y: 10, w: 1300, h: 900, children: [page] };

test("browser titlebar, page and responsive viewport select distinct rectangles", () => {
  assert.equal(targetAt([browser], 400, 40), browser);
  assert.equal(targetAt([browser], 50, 160), page);
  assert.equal(targetAt([browser], 400, 500), viewport);
});

test("Alt selects the entire owning window even over a responsive viewport", () => {
  assert.equal(targetAt([browser], 400, 500, true), browser);
});

test("foreground window occludes smaller child regions of background browsers", () => {
  const dialog = { x: 320, y: 350, w: 300, h: 200 };
  assert.equal(targetAt([dialog, browser], 400, 400), dialog);
  assert.equal(targetAt([dialog, browser], 300, 300), viewport);
});

test("overlapping siblings follow z-order, not area sorting", () => {
  const front = { x: 0, y: 0, w: 80, h: 80 };
  const back = { x: 10, y: 10, w: 20, h: 20 };
  assert.equal(targetAt([{ x: 0, y: 0, w: 100, h: 100, children: [front, back] }], 15, 15), front);
});

test("exclusive right/bottom edges, empty list and negative monitor positions", () => {
  assert.equal(targetAt([browser], 1310, 910), null);
  assert.equal(targetAt([], 10, 10), null);
  const negative = { x: -30, y: -10, w: 100, h: 100 };
  assert.equal(targetAt([negative], -20, -5), negative);
});

test("the contract's WinRect (children: []) on the mock desktop: a pane, its window, the desktop", async () => {
  const { fakeDesktop } = await importTs("src/lib/ipc/mock/shots.ts");
  const { windows } = fakeDesktop(1.5);
  const browserWin = windows.find((w) => w.children.length === 2);
  const [side] = browserWin.children;
  assert.equal(targetAt(windows, side.x + 5, side.y + 5), side);
  assert.equal(targetAt(windows, side.x + 5, side.y + 5, true), browserWin);
  assert.equal(targetAt(windows, 1, 1), null, "the desktop: no window (the host takes the whole monitor)");
});
