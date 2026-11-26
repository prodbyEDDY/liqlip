// The capture overlay host (W6-D, ARCHITECTURE §18.7): its state machine (select → frame →
// committing, text and quick sessions, a failed commit keeps the work), which session payload
// restarts it, the commit header (§18.4), the Esc ladder (SPEC §9.2) and the frame-relative
// annotation document.
import assert from "node:assert/strict";
import test from "node:test";
import { importTs } from "./helpers/bundle.mjs";

const m = await importTs("src/windows/capture/machine.ts", "src/windows/capture/frameDoc.ts");

const session = (over = {}) => ({ id: 7, mode: "region", quick: false, ...over });
const R = { x: 100, y: 50, w: 300, h: 200 };

test("a region session: select → frame on a target → committing → idle", () => {
  let st = m.begin(session());
  assert.equal(st.stage, "select");
  const chosen = m.choose(st, R);
  assert.equal(chosen.commit, null);
  st = chosen.state;
  assert.deepEqual([st.stage, st.frame], ["frame", R]);
  // a new area dragged outside the frame keeps the frame stage
  const again = m.choose(st, { x: 0, y: 0, w: 10, h: 10 });
  assert.deepEqual([again.state.stage, again.commit], ["frame", null]);
  const c = m.startCommit(st, "copy");
  assert.deepEqual([c.stage, c.resume], ["committing", "frame"]);
  assert.equal(m.startCommit(c, "done"), null, "no second commit while committing");
  assert.deepEqual(m.finish(c, 7), m.IDLE);
});

test("text sessions commit `text` on the target; quick sessions commit `done` (no stage 2)", () => {
  const text = m.choose(m.begin(session({ mode: "text" })), R);
  assert.deepEqual([text.commit, text.state.stage, text.state.frame], ["text", "select", R]);
  assert.equal(m.startCommit(text.state, "done"), null, "a text session commits only `text`");
  assert.equal(m.startCommit(text.state, "text").stage, "committing");
  const quick = m.choose(m.begin(session({ quick: true })), R);
  assert.deepEqual([quick.commit, quick.state.stage], ["done", "select"]);
});

test("nothing chosen, idle or committing: no commit; a failed commit goes back with the frame", () => {
  assert.equal(m.startCommit(m.begin(session()), "done"), null, "no frame yet");
  assert.equal(m.startCommit(m.IDLE, "done"), null);
  const frame = m.choose(m.begin(session()), R).state;
  const failed = m.commitFailed(m.startCommit(frame, "done"));
  assert.deepEqual([failed.stage, failed.frame, failed.resume], ["frame", R, null]);
  const textFailed = m.commitFailed(m.startCommit(m.choose(m.begin(session({ mode: "text" })), R).state, "text"));
  assert.equal(textFailed.stage, "select");
  assert.equal(m.commitFailed(frame), frame, "only a committing state fails");
});

test("finish ignores another session's end", () => {
  const st = m.begin(session());
  assert.equal(m.finish(st, 8), st);
  assert.deepEqual(m.finish(st), m.IDLE);
});

test("sessionChange: a new id restarts; the same id updates while selecting, is ignored later", () => {
  assert.equal(m.sessionChange(null, "idle", { id: 1 }), "new");
  assert.equal(m.sessionChange({ id: 1 }, "select", { id: 2 }), "new");
  assert.equal(m.sessionChange({ id: 1 }, "select", { id: 1 }), "update", "followed the cursor to another monitor");
  assert.equal(m.sessionChange({ id: 1 }, "frame", { id: 1 }), "ignore");
  assert.equal(m.sessionChange({ id: 1 }, "committing", { id: 1 }), "ignore");
  assert.equal(m.sessionChange({ id: 1 }, "idle", { id: 1 }), "new");
});

test("commit header (§18.4): no pixels → width/height 0 and no doc; pixels → their size and the doc", () => {
  const doc = m.emptyDoc(300, 200, 1.5);
  assert.deepEqual(m.commitHeader(7, R, "done", doc, null), { sessionId: 7, rect: R, action: "done", doc: null, width: 0, height: 0 });
  assert.deepEqual(m.commitHeader(7, R, "copy", doc, { width: 300, height: 200 }), { sessionId: 7, rect: R, action: "copy", doc, width: 300, height: 200 });
});

test("Esc ladder: host gesture → editor steps → cancel; with objects: arm, then a second Esc within 3 s", () => {
  const base = { hostGesture: false, editorEscaped: false, hasObjects: false, armedAt: null, now: 10_000 };
  assert.equal(m.escapeLadder({ ...base, hostGesture: true, editorEscaped: true }), "gesture");
  assert.equal(m.escapeLadder({ ...base, editorEscaped: true, hasObjects: true }), "editor");
  assert.equal(m.escapeLadder(base), "cancel", "nothing drawn: close at once");
  assert.equal(m.escapeLadder({ ...base, hasObjects: true }), "arm");
  assert.equal(m.escapeLadder({ ...base, hasObjects: true, armedAt: 10_000 - 2_999 }), "cancel");
  assert.equal(m.escapeLadder({ ...base, hasObjects: true, armedAt: 10_000 - m.CONFIRM_MS }), "arm", "the notice expired");
});

// ── the frame-relative document ──

const arrow = { id: "a", color: "#e5484d", kind: "arrow", a: [150, 100], b: [250, 150], bend: 0, style: "straight", size: 3 };
const rect = { id: "r", color: "#e5484d", kind: "rect", r: { x: 120, y: 60, w: 50, h: 40 }, fill: false, size: 2 };
const pen = { id: "p", color: "#e5484d", kind: "pen", points: [[110, 70], [130, 90]], size: 1 };
const text = { id: "t", color: "#e5484d", kind: "text", at: [200, 120], width: null, text: "Hi\nthere", fontSize: 16, style: "plain" };
const counter = { id: "c", color: "#e5484d", kind: "counter", at: [380, 230], n: null, size: 3 };
const spot = { id: "s", kind: "spotlight", r: { x: 90, y: 40, w: 30, h: 30 }, shape: "rect" };
const outside = { id: "o", color: "#e5484d", kind: "rect", r: { x: 900, y: 900, w: 20, h: 20 }, fill: true, size: 1 };

test("docForFrame: objects move by −frame.x/y, the size is the frame's, objects wholly outside are dropped", () => {
  const doc = { v: 1, w: 1920, h: 1080, unit: 1, crop: null, objects: [arrow, rect, pen, text, counter, spot, outside] };
  const out = m.docForFrame(doc, R);
  assert.deepEqual([out.v, out.w, out.h, out.unit, out.crop], [1, 300, 200, 1, null]);
  assert.deepEqual(out.objects.map((o) => o.id), ["a", "r", "p", "t", "c", "s"], "the far rectangle is gone, order kept");
  const byId = Object.fromEntries(out.objects.map((o) => [o.id, o]));
  assert.deepEqual([byId.a.a, byId.a.b], [[50, 50], [150, 100]]);
  assert.deepEqual(byId.r.r, { x: 20, y: 10, w: 50, h: 40 });
  assert.deepEqual(byId.p.points, [[10, 20], [30, 40]]);
  assert.deepEqual(byId.t.at, [100, 70]);
  assert.deepEqual(byId.c.at, [280, 180], "a counter on the edge is kept");
  assert.deepEqual(byId.s.r, { x: -10, y: -10, w: 30, h: 30 }, "partly inside: kept, coordinates may go negative");
  assert.equal(doc.objects[0].a[0], 150, "the editor's document is not mutated");
});

test("objectBounds grow with the stroke and the monitor scale (unit)", () => {
  const b1 = m.objectBounds(rect, 1);
  const b2 = m.objectBounds(rect, 2);
  assert.ok(b2.x < b1.x && b2.w > b1.w);
  assert.deepEqual(m.objectBounds(spot, 1), spot.r);
  const negative = m.objectBounds({ ...rect, r: { x: 170, y: 100, w: -50, h: -40 }, size: 1 }, 1);
  assert.deepEqual(negative, { x: 118, y: 58, w: 54, h: 44 }, "a rectangle drawn up-left is normalised");
});

test("isEmptyAt: double-click → Done only away from every object", () => {
  const doc = { v: 1, w: 1920, h: 1080, unit: 1, crop: null, objects: [rect] };
  assert.equal(m.isEmptyAt(doc, 140, 80), false, "inside the rectangle's box");
  assert.equal(m.isEmptyAt(doc, 300, 300), true);
  assert.equal(m.isEmptyAt(doc, 178, 80, 6), false, "within the tolerance");
  assert.equal(m.isEmptyAt(m.emptyDoc(10, 10, 1), 5, 5), true);
});
