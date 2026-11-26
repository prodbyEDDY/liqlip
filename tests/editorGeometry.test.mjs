// Annotation geometry (W6-C). The first 7 tests are Liquid Tools `tests/annotationGeometry.test.mjs`
// ported as they were (same inputs, same expectations); the rest cover the v1 arrow model and the
// helpers Liqlip added (contour distances, pen thinning/smoothing).
import assert from "node:assert/strict";
import test from "node:test";
import { importTs } from "./helpers/bundle.mjs";

const G = await importTs("src/lib/editor/geometry.ts");
const { arrowHead, arrowBend, paintArrow, resizePath, constrainEndpoint } = G;
const arrow = (extra = {}) => ({ x0: 0, y0: 0, x1: 100, y1: 0, mx: 0, my: 0, width: 4, ...extra });
const plain = (value) => JSON.parse(JSON.stringify(value));

// ── ported from Liquid Tools ─────────────────────────────────────────────────

test("bend handle is on the quadratic midpoint, and follows the pointer without a jump", () => {
  assert.deepEqual(plain(arrowBend(arrow({ mx: 40, my: -60 }))), [70, -30]);
  // Pointer at (80, 20) stores twice its offset from the endpoints' midpoint.
  assert.deepEqual(plain(arrowBend(arrow({ mx: 60, my: 40 }))), [80, 20]);
});

test("rounded head points backward along the end tangent for every direction", () => {
  for (let angle = 0; angle < Math.PI * 2; angle += Math.PI / 12) {
    const s = arrow({ x1: Math.cos(angle) * 100, y1: Math.sin(angle) * 100 });
    const points = arrowHead(s);
    assert.ok(points);
    for (const [x, y] of points) {
      assert.ok(Number.isFinite(x) && Number.isFinite(y));
      assert.ok((s.x1 - x) * s.x1 + (s.y1 - y) * s.y1 > 0);
    }
  }
});

test("short or collapsed arrows never grow oversized heads or nonfinite geometry", () => {
  assert.equal(arrowHead(arrow({ x1: 0 })), null);
  for (const p of arrowHead(arrow({ x1: 3, width: 7 }))) assert.ok(Math.hypot(p[0] - 3, p[1]) <= 1.21);
});

test("arrow paints a single constant-width stroke without the old filled wedge", () => {
  const calls = [];
  const ctx = Object.fromEntries(["beginPath", "moveTo", "lineTo", "quadraticCurveTo", "stroke", "fill"].map((name) => [name, (...args) => calls.push([name, ...args])]));
  paintArrow(ctx, arrow());
  assert.equal(ctx.lineCap, "round");
  assert.equal(ctx.lineJoin, "round");
  assert.equal(ctx.lineWidth, 4);
  assert.equal(calls.filter((c) => c[0] === "stroke").length, 1);
  assert.equal(calls.filter((c) => c[0] === "fill").length, 0);
});

test("stroke resize preserves points and handles crossing without cumulative drift", () => {
  const points = [
    [10, 20],
    [30, 40],
    [50, 60],
  ];
  const from = { x: 10, y: 20, w: 40, h: 40 };
  assert.deepEqual(plain(resizePath(points, from, { x: 100, y: 80, w: -80, h: 20 })), [
    [100, 80],
    [60, 90],
    [20, 100],
  ]);
  assert.deepEqual(points, [
    [10, 20],
    [30, 40],
    [50, 60],
  ]);
});

test("vertical and horizontal marker resizing never divides by zero", () => {
  assert.deepEqual(
    plain(
      resizePath(
        [
          [10, 20],
          [10, 80],
        ],
        { x: 10, y: 20, w: 0, h: 60 },
        { x: 40, y: 50, w: 0, h: 120 },
      ),
    ),
    [
      [40, 50],
      [40, 170],
    ],
  );
});

test("Shift locks lines/arrows to 45 degree steps and squares/circles in each quadrant", () => {
  for (const type of ["line", "arrow"]) {
    const [x, y] = constrainEndpoint(type, 10, 20, 110, 23, true);
    assert.equal(y, 20);
    assert.ok(x > 110 && x < 111);
    const diagonal = constrainEndpoint(type, 0, 0, 70, 80, true);
    assert.ok(Math.abs(diagonal[0] - diagonal[1]) < 1e-9);
  }
  for (const type of ["rect", "ellipse"])
    for (const sx of [-1, 1])
      for (const sy of [-1, 1]) {
        assert.deepEqual(plain(constrainEndpoint(type, 10, 20, 10 + sx * 30, 20 + sy * 50, true)), [10 + sx * 50, 20 + sy * 50]);
      }
  assert.deepEqual(plain(constrainEndpoint("rect", 0, 0, 30, 50, false)), [30, 50]);
});

// ── Liqlip additions ─────────────────────────────────────────────────────────

test("v1 arrow: bend = the curve midpoint's offset in source px; the handle is ON the curve", () => {
  const a = [0, 0],
    b = [100, 0];
  for (const bend of [0, 20, -35]) {
    const mid = G.arrowMid(a, b, bend);
    assert.deepEqual(plain(mid), [50, bend], "offset perpendicular to the chord, in px");
    const q = G.quadPoints(a, G.arrowControl(a, b, bend), b, 2)[1]; // t = 0.5
    assert.ok(Math.hypot(q[0] - mid[0], q[1] - mid[1]) < 1e-9, "handle on the curve");
    assert.ok(Math.abs(G.bendFromPoint(a, b, mid) - bend) < 1e-9, "round trip");
    // |bend| bounds how far the arch leaves the chord (hosts pad bounds by it)
    const far = Math.max(...G.quadPoints(a, G.arrowControl(a, b, bend), b, 64).map((p) => Math.abs(p[1])));
    assert.ok(far <= Math.abs(bend) + 1e-9);
  }
  // diagonal chords too
  assert.ok(Math.abs(G.bendFromPoint([0, 0], [60, 80], G.arrowMid([0, 0], [60, 80], 12)) - 12) < 1e-9);
  assert.equal(G.bendFromPoint([5, 5], [5, 5], [9, 9]), 0, "collapsed chord");
  // the ported geometry draws the same curve
  const g = G.arrowGeometry(a, b, 20, 4);
  assert.deepEqual(plain(arrowBend(g)), plain(G.arrowMid(a, b, 20)));
});

test("double-headed arrows get a second head at the start", () => {
  const calls = [];
  const ctx = Object.fromEntries(["beginPath", "moveTo", "lineTo", "quadraticCurveTo", "stroke"].map((n) => [n, (...a) => calls.push([n, ...a])]));
  paintArrow(ctx, arrow(), true);
  assert.equal(calls.filter((c) => c[0] === "lineTo").length, 4, "two barbs per end");
  assert.equal(calls.filter((c) => c[0] === "stroke").length, 1, "still one stroke");
});

test("contour distance: zero on the border, positive inside and outside", () => {
  const r = { x: 0, y: 0, w: 100, h: 50 };
  assert.equal(G.distRectEdge([50, 0], r), 0);
  assert.equal(G.distRectEdge([50, 25], r), 25, "centre is far from every edge");
  assert.equal(G.distRectEdge([50, -10], r), 10);
  assert.equal(G.distRectEdge([110, 60], r), Math.hypot(10, 10));
  assert.ok(G.inEllipse([50, 25], r) && !G.inEllipse([2, 2], r));
});

test("pen: thinning drops points closer than a step; RDP keeps the shape and both ends", () => {
  let pts = [[0, 0]];
  pts = G.addPoint(pts, [0.5, 0], 1.5);
  assert.equal(pts.length, 1, "too close");
  pts = G.addPoint(pts, [2, 0], 1.5);
  assert.equal(pts.length, 2);
  const line = Array.from({ length: 50 }, (_, i) => [i, i % 2 ? 0.1 : 0]);
  const s = G.simplify(line, 0.5);
  assert.equal(s.length, 2, "jitter below eps collapses");
  assert.deepEqual(plain(s), [
    [0, 0],
    [49, 0.1],
  ]);
  const corner = [
    [0, 0],
    [5, 0],
    [10, 0],
    [10, 5],
    [10, 10],
  ];
  assert.deepEqual(plain(G.simplify(corner, 0.5)), [
    [0, 0],
    [10, 0],
    [10, 10],
  ]);
});

test("pen: the smooth path is quadratic segments through midpoints (no corners, no rAF)", () => {
  const calls = [];
  const ctx = Object.fromEntries(["moveTo", "lineTo", "quadraticCurveTo"].map((n) => [n, (...a) => calls.push([n, ...a])]));
  G.tracePath(ctx, [
    [0, 0],
    [10, 10],
    [20, 0],
    [30, 10],
  ]);
  assert.deepEqual(
    calls.map((c) => c[0]),
    ["moveTo", "quadraticCurveTo", "quadraticCurveTo", "lineTo"],
  );
  assert.deepEqual(calls[1].slice(3), [15, 5], "ends at the midpoint of the next segment");
});

test("box drag: Shift squares, Alt grows from the centre", () => {
  assert.deepEqual(plain(G.dragBox([10, 10], [40, 20], false, false)), { x: 10, y: 10, w: 30, h: 10 });
  assert.deepEqual(plain(G.dragBox([10, 10], [40, 20], true, false)), { x: 10, y: 10, w: 30, h: 30 });
  assert.deepEqual(plain(G.dragBox([50, 50], [60, 55], false, true)), { x: 40, y: 45, w: 20, h: 10 });
});
