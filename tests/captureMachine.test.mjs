// The capture overlay host (W6-D, ARCHITECTURE §18.7): its state machine (select → frame →
// committing, text and quick sessions, a failed commit keeps the work), which session payload
// restarts it, the commit header (§18.4), the Esc ladder (SPEC §9.2) and the frame-relative
// annotation document (INT-5: the editor's `frameDoc` = `cropDoc` + the editor's own object
// bounds, `src/lib/editor/index.ts`; checked here through those two with a stand-in text measure,
// since the real one needs a canvas).
import assert from "node:assert/strict";
import test from "node:test";
import { importTs } from "./helpers/bundle.mjs";

const m = await importTs("src/windows/capture/machine.ts", "src/lib/editor/doc.ts", "src/lib/editor/hit.ts");
/** `frameDoc` of the editor with a stand-in text measure (0.6 em per character). */
const measure = (text, size) => text.length * size * 0.6;
const frameDoc = (doc, frame) => m.cropDoc(doc, frame, (o) => m.bounds(o, { unit: doc.unit, measure }));

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

test("reported stage (W7-R4): the overlay stops following the cursor once a selection starts", () => {
  const st = m.begin(session());
  assert.equal(m.reportedStage(st, false), "select", "hovering: follow the cursor to another monitor");
  assert.equal(m.reportedStage(st, true), "frame", "a press / drag / keyboard area: stay on this monitor");
  const framed = m.choose(st, R).state;
  assert.equal(m.reportedStage(framed, false), "frame");
  const quick = m.startCommit(m.choose(m.begin(session({ quick: true })), R).state, "done");
  assert.equal(m.reportedStage(quick, false), "frame", "committing from stage 1 doesn't follow either");
  assert.equal(m.reportedStage(m.commitFailed(quick), false), "select", "a failed quick commit follows again");
  assert.equal(m.reportedStage(m.IDLE, false), null);
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

test("commit header (§18.4): no pixels → width/height 0; pixels → their size; the doc goes with every picture action", () => {
  const doc = m.emptyDoc(300, 200, 1.5);
  // W8-A: a shot without objects still sends its document — the monitor scale is its `unit`
  assert.deepEqual(m.commitHeader(7, R, "done", doc, null), { sessionId: 7, rect: R, action: "done", doc, width: 0, height: 0 });
  assert.equal(m.commitHeader(7, R, "done", doc, null).doc.unit, 1.5);
  assert.deepEqual(m.commitHeader(7, R, "copy", doc, { width: 300, height: 200 }), { sessionId: 7, rect: R, action: "copy", doc, width: 300, height: 200 });
  assert.equal(m.commitHeader(7, R, "text", doc, null).doc, null, "text keeps no picture: no document");
  assert.equal(m.commitHeader(7, R, "done", null, null).doc, null);
});

test("a commit that never answers (W8-A): after 15 s back where it started — Esc and Cancel work again", () => {
  assert.equal(m.COMMIT_TIMEOUT_MS, 15_000);
  const framed = m.choose(m.begin(session()), R).state;
  const busy = m.startCommit(framed, "done");
  assert.equal(busy.stage, "committing");
  const back = m.commitTimedOut(busy, 7);
  assert.equal(back.stage, "frame", "the frame and the drawing are still there");
  assert.deepEqual(back.frame, R);
  assert.equal(m.escapeLadder({ hostGesture: false, editorEscaped: false, hasObjects: false, armedAt: null, now: 0 }), "cancel");
  assert.equal(m.startCommit(back, "done").stage, "committing", "it can be tried again");
  // From stage 1 (quick / text) it returns to stage 1.
  const quick = m.startCommit(m.choose(m.begin(session({ quick: true })), R).state, "done");
  assert.equal(m.commitTimedOut(quick, 7).stage, "select");
  // A late timer of another session, or of a commit that already finished, changes nothing.
  assert.equal(m.commitTimedOut(busy, 8), busy);
  assert.equal(m.commitTimedOut(m.IDLE, 7), m.IDLE);
  assert.equal(m.commitTimedOut(back, 7), back);
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

test("frameDoc: objects move by −frame.x/y, the size is the frame's, objects wholly outside are dropped", () => {
  const doc = { v: 1, w: 1920, h: 1080, unit: 1, crop: null, objects: [arrow, rect, pen, text, counter, spot, outside] };
  const out = frameDoc(doc, R);
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

test("an empty document of a frozen frame: unit = the monitor scale", () => {
  assert.deepEqual(m.emptyDoc(2880, 1620, 1.5), { v: 1, w: 2880, h: 1620, unit: 1.5, crop: null, objects: [] });
  assert.equal(m.emptyDoc(10, 10, 0).unit, 1, "a broken scale falls back to 1");
});
