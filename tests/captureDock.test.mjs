// Where the capture overlay puts its chrome (W6-D, ARCHITECTURE §18.7): the panel docks below the
// frame outside → above it outside → inside at its bottom edge, pressed against the monitor
// edges; the size chip; the magnifier flipping at the edges. All view px.
import assert from "node:assert/strict";
import test from "node:test";
import { importTs } from "./helpers/bundle.mjs";

const d = await importTs("src/windows/capture/dock.ts");

const view = { w: 1920, h: 1080 };
const panel = { w: 600, h: 48 };
const inView = (p, size) => p.x >= d.EDGE && p.y >= d.EDGE && p.x + size.w <= view.w - d.EDGE && p.y + size.h <= view.h - d.EDGE;

test("panel: below the frame, outside, centred on it", () => {
  const frame = { x: 600, y: 200, w: 400, h: 300 };
  const p = d.dockPanel(frame, panel, view);
  assert.equal(p.side, "below");
  assert.equal(p.y, frame.y + frame.h + d.DOCK_GAP);
  assert.equal(p.x, frame.x + frame.w / 2 - panel.w / 2);
  assert.ok(p.y > frame.y + frame.h, "never covers the frame");
});

test("panel: above the frame when there is no room below", () => {
  const frame = { x: 600, y: 600, w: 400, h: 440 };
  const p = d.dockPanel(frame, panel, view);
  assert.equal(p.side, "above");
  assert.equal(p.y + panel.h, frame.y - d.DOCK_GAP);
});

test("panel: inside at the bottom edge when the frame fills the monitor", () => {
  const p = d.dockPanel({ x: 0, y: 0, w: 1920, h: 1080 }, panel, view);
  assert.equal(p.side, "inside");
  assert.equal(p.y, 1080 - d.INSIDE_INSET - panel.h);
  assert.ok(inView(p, panel));
});

test("panel: inside for a tall frame with no room above or below", () => {
  const frame = { x: 300, y: 30, w: 500, h: 1030 };
  const p = d.dockPanel(frame, panel, view);
  assert.equal(p.side, "inside");
  assert.equal(p.y, frame.y + frame.h - d.INSIDE_INSET - panel.h);
});

test("panel: no room for the full gap — still outside, closer, never over the picture (W7-R4)", () => {
  // 1280 × 720 at 150 %: view 853 × 480, the one-column panel 538 × 144, a medium frame
  const v = { w: 853, h: 480 };
  const tall = { w: 538, h: 144 };
  const frame = { x: 132, y: 115, w: 535, h: 196 };
  const p = d.dockPanel(frame, tall, v);
  assert.equal(p.side, "below");
  assert.equal(p.y, v.h - d.EDGE - tall.h);
  assert.ok(p.y >= frame.y + frame.h + d.TIGHT_GAP, "leaves the inner half of the bottom handles free");
  // the same frame near the bottom of the monitor: above, pressed to the top edge
  const low = { x: 132, y: 170, w: 535, h: 230 };
  const q = d.dockPanel(low, tall, v);
  assert.equal(q.side, "above");
  assert.equal(q.y, d.EDGE);
  assert.ok(q.y + tall.h <= low.y - d.TIGHT_GAP);
  // neither: inside (as before)
  assert.equal(d.dockPanel({ x: 132, y: 100, w: 535, h: 300 }, tall, v).side, "inside");
});

test("panel: pressed against the left and right monitor edges", () => {
  const left = d.dockPanel({ x: 0, y: 100, w: 100, h: 100 }, panel, view);
  assert.equal(left.x, d.EDGE);
  const right = d.dockPanel({ x: 1850, y: 100, w: 70, h: 100 }, panel, view);
  assert.equal(right.x, view.w - panel.w - d.EDGE);
  for (const p of [left, right]) assert.ok(inView(p, panel));
});

test("panel: wider than the monitor starts at the left edge", () => {
  const p = d.dockPanel({ x: 100, y: 100, w: 100, h: 100 }, { w: 2400, h: 48 }, view);
  assert.equal(p.x, d.EDGE);
});

test("size chip: above the frame's corner past the handles; inside when no room or over the panel", () => {
  const chip = { w: 90, h: 24 };
  const above = d.placeChip({ x: 600, y: 200, w: 400, h: 300 }, chip, view);
  assert.deepEqual(above, { x: 600, y: 200 - d.CHIP_GAP - chip.h, inside: false });
  const top = d.placeChip({ x: 600, y: 10, w: 400, h: 300 }, chip, view);
  assert.deepEqual(top, { x: 600 + d.CHIP_INSET, y: 10 + d.CHIP_INSET, inside: true });
  const panelAbove = { x: 500, y: 140, w: 600, h: 48 };
  assert.equal(d.placeChip({ x: 600, y: 200, w: 400, h: 300 }, chip, view, panelAbove).inside, true);
});

test("magnifier: at the cursor's lower right, flips at the right and bottom edges, never off screen", () => {
  const box = { w: 150, h: 180 };
  assert.deepEqual(d.placeLoupe({ x: 100, y: 100 }, box, view), { x: 116, y: 116 });
  assert.deepEqual(d.placeLoupe({ x: 1900, y: 1070 }, box, view), { x: 1900 - 16 - 150, y: 1070 - 16 - 180 });
  const tiny = d.placeLoupe({ x: 60, y: 60 }, box, { w: 200, h: 200 });
  assert.ok(tiny.x >= 0 && tiny.y >= 0 && tiny.x + box.w <= 200 && tiny.y + box.h <= 200);
});
