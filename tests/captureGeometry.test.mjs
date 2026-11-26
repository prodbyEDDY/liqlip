// Geometry of the capture overlay (W6-D, ARCHITECTURE §18.7): view px → frame px at scale 1 / 1.5
// / 2 (frame.width / innerWidth, never devicePixelRatio), drawing an area (Shift, Alt, the frame
// edges), the stage-2 frame (move, resize, Ctrl+arrows), handles OUTSIDE the frame and what a
// point hits around it, the magnifier's maths.
import assert from "node:assert/strict";
import test from "node:test";
import { importTs } from "./helpers/bundle.mjs";

const g = await importTs("src/windows/capture/geometry.ts", "src/windows/capture/loupe.ts");

test("view → frame px uses frame.width / innerWidth at scale 1, 1.5 and 2", () => {
  for (const scale of [1, 1.5, 2]) {
    const frame = { w: 1920 * scale, h: 1080 * scale };
    const k = g.viewScale(frame, { w: 1920, h: 1080 });
    assert.deepEqual(k, { kx: scale, ky: scale });
    assert.deepEqual(g.toFrame({ x: 100, y: 50 }, k), { x: 100 * scale, y: 50 * scale });
    assert.deepEqual(g.framePoint({ x: 100.6, y: 50.2 }, k, frame), { x: Math.floor(100.6 * scale), y: Math.floor(50.2 * scale) });
    assert.deepEqual(g.rectToView({ x: 300, y: 150, w: 600, h: 300 }, k), { x: 300 / scale, y: 150 / scale, w: 600 / scale, h: 300 / scale });
    // the far edge of the view reaches the frame edge (a region can end there)
    assert.deepEqual(g.framePoint({ x: 1920, y: 1080 }, k, frame), { x: frame.w, y: frame.h });
  }
  assert.deepEqual(g.viewScale({ w: 2880, h: 1620 }, { w: 0, h: 0 }), { kx: 1, ky: 1 }, "a view not laid out yet");
});

test("click vs drag: 5 view px on either axis", () => {
  assert.equal(g.isDrag({ x: 10, y: 10 }, { x: 14, y: 14 }), false);
  assert.equal(g.isDrag({ x: 10, y: 10 }, { x: 15, y: 10 }), true);
  assert.equal(g.isDrag({ x: 10, y: 10 }, { x: 10, y: 4 }), true);
});

test("an area: any direction, Shift — square, Alt — from the centre, always inside the frame", () => {
  const F = { w: 1000, h: 500 };
  const plain = { square: false, fromCenter: false };
  assert.deepEqual(g.regionFrom({ x: 100, y: 100 }, { x: 300, y: 150 }, plain, F), { x: 100, y: 100, w: 200, h: 50 });
  assert.deepEqual(g.regionFrom({ x: 300, y: 150 }, { x: 100, y: 100 }, plain, F), { x: 100, y: 100, w: 200, h: 50 });
  assert.deepEqual(g.regionFrom({ x: 100, y: 100 }, { x: 300, y: 150 }, { square: true, fromCenter: false }, F), { x: 100, y: 100, w: 200, h: 200 });
  assert.deepEqual(g.regionFrom({ x: 500, y: 250 }, { x: 600, y: 300 }, { square: false, fromCenter: true }, F), { x: 400, y: 200, w: 200, h: 100 });
  assert.deepEqual(g.regionFrom({ x: 500, y: 250 }, { x: 600, y: 270 }, { square: true, fromCenter: true }, F), { x: 400, y: 150, w: 200, h: 200 });
  // a square at the bottom edge stays square (limited by the room on both axes)
  assert.deepEqual(g.regionFrom({ x: 100, y: 450 }, { x: 400, y: 480 }, { square: true, fromCenter: false }, F), { x: 100, y: 450, w: 50, h: 50 });
  // from the centre near the left edge: limited by the room on the short side
  assert.deepEqual(g.regionFrom({ x: 20, y: 250 }, { x: 120, y: 260 }, { square: false, fromCenter: true }, F), { x: 0, y: 240, w: 40, h: 20 });
});

test("snapRect rounds to whole pixels, keeps inside the frame, drops an empty rectangle", () => {
  const F = { w: 100, h: 50 };
  assert.deepEqual(g.snapRect({ x: -3.4, y: 10.6, w: 20, h: 100 }, F), { x: 0, y: 11, w: 17, h: 39 });
  assert.deepEqual(g.snapRect({ x: 30, y: 30, w: -10, h: -10 }, F), { x: 20, y: 20, w: 10, h: 10 });
  assert.equal(g.snapRect({ x: 120, y: 0, w: 10, h: 10 }, F), null);
});

test("the frame moves and resizes inside the monitor; an edge dragged past the other flips it", () => {
  const F = { w: 1000, h: 500 };
  const r = { x: 100, y: 100, w: 200, h: 100 };
  assert.deepEqual(g.moveRect(r, 50, -20, F), { x: 150, y: 80, w: 200, h: 100 });
  assert.deepEqual(g.moveRect(r, 5000, 5000, F), { x: 800, y: 400, w: 200, h: 100 }, "stops at the edge, keeps its size");
  assert.deepEqual(g.resizeRect(r, "se", 30, 40, F), { x: 100, y: 100, w: 230, h: 140 });
  assert.deepEqual(g.resizeRect(r, "nw", -150, -150, F), { x: 0, y: 0, w: 300, h: 200 }, "clamped to the monitor");
  assert.deepEqual(g.resizeRect(r, "e", -250, 0, F), { x: 50, y: 100, w: 50, h: 100 }, "flips past the left edge");
  assert.deepEqual(g.resizeRect(r, "n", 0, 100, F), { x: 100, y: 200, w: 200, h: 1 }, "never thinner than 1 px");
  assert.deepEqual(g.resizeRect(r, "s", 999, 999, F), { x: 100, y: 100, w: 200, h: 400 }, "a side handle moves one edge");
});

test("Ctrl+arrows move the frame by 1 / 10, Alt+arrows resize it from the right/bottom", () => {
  const F = { w: 1000, h: 500 };
  const r = { x: 100, y: 100, w: 200, h: 100 };
  assert.deepEqual(g.nudgeRect(r, g.arrowDelta("ArrowRight"), 1, "move", F), { x: 101, y: 100, w: 200, h: 100 });
  assert.deepEqual(g.nudgeRect(r, g.arrowDelta("ArrowUp"), 10, "move", F), { x: 100, y: 90, w: 200, h: 100 });
  assert.deepEqual(g.nudgeRect(r, g.arrowDelta("ArrowLeft"), 10, "resize", F), { x: 100, y: 100, w: 190, h: 100 });
  assert.deepEqual(g.nudgeRect(r, g.arrowDelta("ArrowDown"), 1000, "resize", F), { x: 100, y: 100, w: 200, h: 400 }, "not past the monitor");
  assert.deepEqual(g.nudgeRect({ x: 0, y: 0, w: 1, h: 1 }, g.arrowDelta("ArrowLeft"), 10, "resize", F).w, 1, "not below 1 px");
  assert.equal(g.arrowDelta("KeyA"), null);
});

test("handles sit 12 px OUTSIDE the frame edge; at a monitor edge they are pulled back on screen", () => {
  const view = { w: 1000, h: 600 };
  const c = g.handleCenters({ x: 100, y: 100, w: 200, h: 100 }, view);
  assert.deepEqual(c.nw, { x: 88, y: 88 });
  assert.deepEqual(c.se, { x: 312, y: 212 });
  assert.deepEqual(c.n, { x: 200, y: 88 });
  assert.deepEqual(c.w, { x: 88, y: 150 });
  const full = g.handleCenters({ x: 0, y: 0, w: 1000, h: 600 }, view);
  assert.deepEqual(full.nw, { x: g.HANDLE_R, y: g.HANDLE_R });
  assert.deepEqual(full.se, { x: 1000 - g.HANDLE_R, y: 600 - g.HANDLE_R });
});

test("hitFrame: handles first, the inside goes to the editor, a 10 px band moves, the rest is outside", () => {
  const view = { w: 1000, h: 600 };
  const r = { x: 100, y: 100, w: 200, h: 100 };
  assert.deepEqual(g.hitFrame({ x: 88, y: 88 }, r, view), { kind: "handle", handle: "nw" });
  assert.deepEqual(g.hitFrame({ x: 200, y: 92 }, r, view), { kind: "handle", handle: "n" });
  assert.deepEqual(g.hitFrame({ x: 101, y: 101 }, r, view), { kind: "inside" }, "drawing at the very corner is the editor's");
  assert.deepEqual(g.hitFrame({ x: 150, y: 95 }, r, view), { kind: "band" });
  assert.deepEqual(g.hitFrame({ x: 305, y: 120 }, r, view), { kind: "band" });
  assert.deepEqual(g.hitFrame({ x: 305, y: 150 }, r, view), { kind: "handle", handle: "e" }, "the side handle sits on the band");
  assert.deepEqual(g.hitFrame({ x: 150, y: 50 }, r, view), { kind: "outside" });
  // a frame filling the monitor: corner handles sit just inside and still win
  assert.deepEqual(g.hitFrame({ x: 6, y: 6 }, { x: 0, y: 0, w: 1000, h: 600 }, view), { kind: "handle", handle: "nw" });
  assert.deepEqual(g.hitFrame({ x: 500, y: 300 }, { x: 0, y: 0, w: 1000, h: 600 }, view), { kind: "inside" });
});

test("side handles hide on a short side (they would cover the corners)", () => {
  assert.deepEqual(g.visibleHandles({ x: 0, y: 0, w: 30, h: 100 }), ["nw", "ne", "se", "sw", "e", "w"]);
  assert.deepEqual(g.visibleHandles({ x: 0, y: 0, w: 30, h: 30 }), ["nw", "ne", "se", "sw"]);
  assert.equal(g.visibleHandles({ x: 0, y: 0, w: 300, h: 300 }).length, 8);
});

test("magnifier: an odd pixel count around the cursor pixel, wheel steps through the zooms", () => {
  assert.equal(g.loupeCount(10), 15, "ShareX default: 15 pixels × 10 px");
  for (const z of g.LOUPE_ZOOMS) {
    const n = g.loupeCount(z);
    assert.equal(n % 2, 1);
    assert.ok(n >= 3 && n * z <= g.LOUPE_SPAN);
  }
  assert.equal(g.zoomBy(10, 1), 12);
  assert.equal(g.zoomBy(10, -1), 8);
  assert.equal(g.zoomBy(30, 1), 30, "stops at the last step");
  assert.equal(g.zoomBy(4, -1), 4);
  assert.equal(g.zoomBy(7, 1), 8, "between steps: the next one");
  assert.deepEqual(g.loupeSource({ x: 100, y: 50 }, 15), { x: 93, y: 43, w: 15, h: 15 });
});
