// SPEC §9.3 interaction rules 1–12 (W6-C) — each one a test on the PURE dispatcher
// (src/lib/editor/interaction.ts): synthetic pointer / key events in, document / selection / tool
// out. Owner's two complaints about the Liquid Tools editor are rules 1 and 3 (+ 7):
//   A) "inside a drawn rectangle I can't draw a new one — the old one gets selected";
//   B) "a pen stroke is immediately wrapped in a frame that hides the drawing".
import assert from "node:assert/strict";
import test from "node:test";
import { importTs } from "./helpers/bundle.mjs";

const I = await importTs("src/lib/editor/interaction.ts", "src/lib/editor/doc.ts", "src/lib/editor/defaults.ts", "src/lib/editor/hit.ts");

const measure = (text, size) => (Array.from(text).length * size) / 2;
const env = (over = {}) => ({ mode: "window", scale: 1, measure, clip: null, intents: true, ...over });
const NO = { shift: false, ctrl: false, alt: false };
let clock = 10_000;
const ptr = (type, x, y, mods = {}, button = 0) => ({ type, pt: [x, y], button, mods: { ...NO, ...mods }, time: (clock += 16) });
const down = (x, y, m, b) => ptr("down", x, y, m, b);
const move = (x, y, m) => ptr("move", x, y, m);
const up = (x, y, m) => ptr("up", x, y, m);
const drag = (x0, y0, x1, y1, m = {}) => [down(x0, y0, m), move((x0 + x1) / 2, (y0 + y1) / 2, m), move(x1, y1, m), up(x1, y1, m)];
const key = (code, m = {}) => ({ type: "key", code, ctrl: false, shift: false, alt: false, repeat: false, composing: false, time: (clock += 16), ...m });
const fresh = (tool = "rect", w = 800, h = 600) => I.createState(I.emptyDoc(w, h, 1), I.factoryDefaults(), tool);

/** Run events; returns the final state (and the last result). */
function run(s, events, e = env()) {
  let r = { state: s, handled: false };
  for (const ev of events.flat()) r = I.dispatch(r.state, ev, e);
  return Object.assign(r.state, { $last: r });
}
const kinds = (s) => s.doc.objects.map((o) => o.kind);
const byId = (s, id) => s.doc.objects.find((o) => o.id === id);

// ── rule 1: a drawing tool always creates ────────────────────────────────────

test("rule 1 (complaint A): a rectangle INSIDE a rectangle is a new rectangle; the first never moves", () => {
  let s = run(fresh("rect"), drag(100, 100, 400, 400));
  const first = structuredClone(s.doc.objects[0]);
  s = run(s, drag(150, 150, 250, 250)); // inside
  s = run(s, drag(100, 100, 180, 180)); // exactly on the first one's corner (where LT had a handle)
  s = run(s, drag(400, 250, 500, 300)); // starting on its edge
  assert.deepEqual(kinds(s), ["rect", "rect", "rect", "rect"]);
  assert.deepEqual(s.doc.objects[0], first, "the first rectangle is untouched");
  assert.deepEqual(s.sel, [], "nothing got selected");
});

test("rule 1: over any other object too — pen over text, arrow over a filled rect, marker over a counter", () => {
  let s = fresh("rect");
  s.defaults.tools.rect.fill = true;
  s = run(s, drag(0, 0, 300, 300));
  s = run(s, [{ type: "tool", tool: "arrow" }, ...drag(50, 50, 250, 250)]);
  s = run(s, [{ type: "tool", tool: "counter" }, down(100, 100), up(100, 100)]);
  s = run(s, [{ type: "tool", tool: "marker" }, ...drag(80, 100, 140, 100)]);
  s = run(s, [{ type: "tool", tool: "pen" }, ...drag(100, 100, 120, 130)]);
  assert.equal(s.doc.objects.length, 5);
  assert.deepEqual(s.sel, []);
});

test("rule 1: a click without a drag (< 4 screen px) with R / E / A / L creates nothing", () => {
  for (const tool of ["rect", "ellipse", "arrow", "line"]) {
    const s = run(fresh(tool), [down(100, 100), move(102, 101), up(102, 101)]);
    assert.equal(s.doc.objects.length, 0, tool);
    assert.equal(s.hist.past.length, 0, `${tool}: no history step`);
  }
  // …measured in SCREEN px: at 400 % a 2-source-px drag is 8 screen px — a real rectangle
  const zoomed = run(fresh("rect"), [down(100, 100), move(102, 102), up(102, 102)], env({ scale: 4 }));
  assert.equal(zoomed.doc.objects.length, 1);
  // Text and Counter do place an object on a click
  assert.equal(run(fresh("counter"), [down(5, 5), up(5, 5)]).doc.objects.length, 1);
  assert.ok(run(fresh("text"), [down(5, 5), up(5, 5)]).text, "text starts typing");
});

// ── rule 2: the tool stays ───────────────────────────────────────────────────

test("rule 2: three arrows in a row without touching a key", () => {
  const s = run(fresh("arrow"), [drag(10, 10, 100, 100), drag(20, 10, 110, 100), drag(30, 10, 120, 100)]);
  assert.deepEqual(kinds(s), ["arrow", "arrow", "arrow"]);
  assert.equal(s.tool, "arrow");
  // the text tool too: committing a text keeps Text (LT switched to Select, П-5)
  let t = run(fresh("text"), [down(10, 10), up(10, 10), { type: "text", value: "one" }, down(200, 200), up(200, 200)]);
  assert.equal(t.tool, "text");
  assert.ok(t.text, "the second click started a second text");
  t = run(t, [{ type: "text", value: "two" }, { type: "escape" }]);
  assert.deepEqual(
    t.doc.objects.map((o) => o.text),
    ["one", "two"],
  );
});

// ── rule 3: new objects are not selected ─────────────────────────────────────

test("rule 3 (complaint B): a pen stroke is never selected — no frame, no handles — during or after", () => {
  let s = run(fresh("pen"), [down(10, 10), move(30, 40), move(60, 20)]);
  assert.ok(s.gesture, "drawing");
  assert.deepEqual(s.sel, [], "no selection while drawing");
  assert.equal(s.hover, null, "no hover outline while drawing");
  s = run(s, [up(80, 50)]);
  assert.deepEqual(s.sel, [], "no selection after");
  assert.equal(s.last, s.doc.objects[0].id, "remembered as the last object only");
  const he = { unit: 1, measure, scale: 1 };
  assert.deepEqual(I.handlesOf(s.doc.objects[0], he), [], "a stroke has no handles at all");
  for (const tool of ["rect", "ellipse", "arrow", "line", "marker", "redact", "spotlight", "counter"]) {
    const t = run(fresh(tool), drag(10, 10, 90, 90));
    assert.deepEqual(t.sel, [], tool);
  }
});

// ── rule 4: properties right after creation ──────────────────────────────────

test("rule 4: a size change right after drawing changes that arrow AND the arrow default; the next arrow uses it", () => {
  let s = run(fresh("arrow"), [drag(10, 10, 200, 10), key("Digit5")]);
  assert.equal(s.doc.objects[0].size, 5, "the just-drawn arrow got thicker");
  assert.equal(s.defaults.tools.arrow.size, 5, "and the default");
  s = run(s, drag(10, 50, 200, 50));
  assert.equal(s.doc.objects[1].size, 5, "the next arrow is thick too");
  // after the next press, a change no longer touches the previous object
  s = run(s, [down(300, 300), up(301, 300), key("Digit1")]);
  assert.equal(s.doc.objects[1].size, 5);
  assert.equal(s.defaults.tools.arrow.size, 1);
});

test("rule 4: a tool change or Esc ends the 'last object' window; colour from the bar works the same", () => {
  let s = run(fresh("rect"), [drag(0, 0, 50, 50), { type: "prop", prop: "color", value: "#007aff", time: 1 }]);
  assert.equal(s.doc.objects[0].color, "#007aff");
  s = run(s, [{ type: "tool", tool: "ellipse" }, { type: "prop", prop: "color", value: "#34c759", time: 2 }]);
  assert.equal(s.doc.objects[0].color, "#007aff", "rect untouched after a tool change");
  assert.equal(s.defaults.tools.ellipse.color, "#34c759");
  assert.equal(s.defaults.tools.rect.color, "#007aff", "rect default kept from before");
  s = run(s, [drag(100, 100, 150, 150), { type: "escape" }, { type: "prop", prop: "size", value: 4, time: 3 }]);
  assert.equal(s.doc.objects[1].size, 3, "Esc ended it");
});

test("rule 4: with a selection the change goes to the selected objects and the default of THEIR type", () => {
  let s = run(fresh("rect"), [drag(0, 0, 100, 100), { type: "tool", tool: "arrow" }, drag(200, 200, 300, 300)]);
  s = run(s, [{ type: "tool", tool: "select" }, down(0, 50), up(0, 50)]); // rect contour
  assert.equal(s.sel.length, 1);
  s = run(s, [key("Digit2", { shift: true })]); // Shift+2 — orange
  assert.equal(byId(s, s.sel[0]).color, "#ff9500");
  assert.equal(s.defaults.tools.rect.color, "#ff9500", "rect default");
  assert.equal(s.defaults.tools.arrow.color, "#ff3b30", "arrow default untouched");
  assert.equal(s.doc.objects[1].color, "#ff3b30", "the unselected arrow untouched");
});

test("rule 4: no selection and no last object — only the tool default", () => {
  const s = run(fresh("pen"), [key("Digit4"), key("Digit8", { shift: true })]);
  assert.equal(s.doc.objects.length, 0);
  assert.equal(s.defaults.tools.pen.size, 4);
  assert.equal(s.defaults.tools.pen.color, "#ffffff");
  assert.equal(s.hist.past.length, 0, "no history for defaults");
});

// ── rule 5: Ctrl = temporary select; cursors ─────────────────────────────────

test("rule 5: with the pen and Ctrl held a drag on a stroke MOVES it; Ctrl released — drawing again", () => {
  let s = run(fresh("pen"), drag(100, 100, 200, 100));
  const before = s.doc.objects[0].points.map((p) => [...p]);
  s = run(s, drag(150, 100, 150, 140, { ctrl: true }));
  assert.equal(s.doc.objects.length, 1, "no new stroke");
  assert.deepEqual(
    s.doc.objects[0].points,
    before.map(([x, y]) => [x, y + 40]),
    "moved down 40",
  );
  assert.equal(s.tool, "pen", "still the pen");
  s = run(s, drag(150, 300, 250, 300));
  assert.equal(s.doc.objects.length, 2, "without Ctrl the pen draws");
  assert.deepEqual(s.sel, [], "and the temporary selection is gone");
});

test("rule 5: cursor — crosshair for a tool; move / outline over an object with Ctrl or Select", () => {
  let s = run(fresh("rect"), drag(100, 100, 300, 300));
  s = run(s, [move(100, 200)]);
  assert.equal(I.cursorFor(s), "crosshair", "a drawing tool over an object: still crosshair");
  assert.equal(s.hover, null, "no hover outline while drawing");
  s = run(s, [{ type: "mods", mods: { ...NO, ctrl: true } }]);
  assert.equal(s.hover, s.doc.objects[0].id, "Ctrl → hover outline (without moving the mouse)");
  assert.equal(I.cursorFor(s), "move");
  s = run(s, [{ type: "mods", mods: NO }]);
  assert.equal(I.cursorFor(s), "crosshair");
  s = run(s, [{ type: "tool", tool: "select" }, move(500, 500)]);
  assert.equal(I.cursorFor(s), "default");
  s = run(s, [down(100, 200), up(100, 200), move(300, 300)]);
  assert.equal(I.cursorFor(s), "nwse-resize", "over the selected rect's corner handle");
  assert.equal(I.cursorFor(run(fresh("text"), [])), "text");
});

// ── rule 6: contour vs area ──────────────────────────────────────────────────

test("rule 6: outlined shapes are hit only on the contour; a small rect inside a big one is selectable", () => {
  let s = run(fresh("rect"), [drag(100, 100, 500, 500), drag(250, 250, 290, 290)]);
  const [big, small] = s.doc.objects;
  s = run(s, [{ type: "tool", tool: "select" }, down(250, 270), up(250, 270)]); // small's left edge
  assert.deepEqual(s.sel, [small.id], "the small one inside");
  s = run(s, [down(200, 200), move(240, 240)]);
  assert.equal(s.gesture.kind, "marquee", "empty space inside the big outline starts a marquee");
  s = run(s, [up(240, 240), down(104, 300), up(104, 300)]);
  assert.deepEqual(s.sel, [big.id], "the big one by its contour (± tolerance)");
});

test("rule 6: tolerance ≥ 6 SCREEN px + half the stroke, at any zoom", () => {
  const he = (scale) => ({ unit: 1, measure, scale });
  const line = { id: "l", kind: "line", color: "#000000", a: [0, 0], b: [100, 0], size: 1 }; // 2 px wide
  assert.ok(I.hitObject(line, [50, 6.9], he(1)), "6 + 1 px at 100 %");
  assert.ok(!I.hitObject(line, [50, 8], he(1)));
  assert.ok(I.hitObject(line, [50, 3.9], he(2)), "3 + 1 source px at 200 % (= 6 screen px)");
  assert.ok(!I.hitObject(line, [50, 5], he(2)));
});

test("rule 6: filled shapes, text, counters and 'hide' plates are hit by area", () => {
  const he = { unit: 1, measure, scale: 1 };
  const r = (fill) => ({ id: "r", kind: "rect", color: "#000000", r: { x: 0, y: 0, w: 100, h: 100 }, fill, size: 3 });
  assert.ok(I.hitObject(r(true), [50, 50], he));
  assert.ok(!I.hitObject(r(false), [50, 50], he));
  const e = (fill) => ({ id: "e", kind: "ellipse", color: "#000000", r: { x: 0, y: 0, w: 100, h: 60 }, fill, size: 3 });
  assert.ok(I.hitObject(e(true), [50, 30], he) && !I.hitObject(e(false), [50, 30], he));
  assert.ok(I.hitObject(e(false), [100, 30], he), "outlined ellipse on its contour");
  assert.ok(I.hitObject({ id: "x", kind: "redact", color: "#000000", r: { x: 0, y: 0, w: 40, h: 20 }, mode: "solid", size: 3 }, [20, 10], he));
  assert.ok(I.hitObject({ id: "c", kind: "counter", color: "#000000", at: [50, 50], n: null, size: 3 }, [52, 55], he));
  assert.ok(I.hitObject({ id: "t", kind: "text", color: "#000000", at: [0, 0], width: null, text: "Hello", fontSize: 20, style: "plain" }, [30, 12], he));
});

// ── rule 7: handles outside small objects; strokes: highlight only ───────────

test("rule 7: handles of a small object sit OUTSIDE it; a big one keeps them on its corners", () => {
  const he = { unit: 1, measure, scale: 1 };
  const small = { id: "s", kind: "rect", color: "#000000", r: { x: 100, y: 100, w: 12, h: 10 }, fill: false, size: 3 };
  for (const h of I.handlesOf(small, he)) {
    const [x, y] = h.pt;
    const inside = x > 100 && x < 112 && y > 100 && y < 110;
    assert.ok(!inside, `${h.id} at ${x},${y} must not cover the object`);
    if (h.id.length === 2) assert.ok(x < 100 || x > 112, `${h.id} out on x`);
  }
  const big = { ...small, r: { x: 0, y: 0, w: 300, h: 200 } };
  assert.deepEqual(I.handlesOf(big, he).find((h) => h.id === "se").pt, [300, 200]);
  for (const kind of ["pen", "marker"]) assert.deepEqual(I.handlesOf({ id: "p", kind, color: "#000000", points: [[0, 0], [5, 5]], size: 3 }, he), [], kind);
  // selecting a stroke selects it (highlight drawn by the renderer), and only moving is possible
  let s = run(fresh("pen"), [drag(0, 0, 100, 0), { type: "tool", tool: "select" }, down(50, 0), up(50, 0)]);
  assert.equal(s.sel.length, 1);
  s = run(s, [move(50, 0)]);
  assert.equal(s.hoverHandle, null, "no handle anywhere on a stroke");
});

// ── rule 8: text ─────────────────────────────────────────────────────────────

test("rule 8: Esc / Ctrl+Enter / a click outside FINISH the text and keep it; empty text is removed", () => {
  const typed = (s, v) => run(s, [{ type: "text", value: v }]);
  let s = typed(run(fresh("text"), [down(10, 10), up(10, 10)]), "kept on Esc");
  s = run(s, [{ type: "escape" }]);
  assert.equal(s.text, null);
  assert.equal(s.doc.objects[0].text, "kept on Esc", "LT discarded it (П-9)");
  s = typed(run(s, [down(10, 100), up(10, 100)]), "kept on Ctrl+Enter");
  s = run(s, [{ type: "textCommit" }]);
  assert.equal(s.doc.objects[1].text, "kept on Ctrl+Enter");
  s = typed(run(s, [down(10, 200), up(10, 200)]), "kept on a click outside");
  s = run(s, [{ type: "tool", tool: "rect" }, ...drag(300, 300, 400, 400)]);
  assert.equal(s.doc.objects.filter((o) => o.kind === "text").length, 3);
  s = run(s, [{ type: "tool", tool: "text" }, down(500, 500), up(500, 500), { type: "text", value: "   " }, { type: "escape" }]);
  assert.equal(s.doc.objects.filter((o) => o.kind === "text").length, 3, "whitespace-only text removed");
  assert.equal(s.hist.past.length, 4, "three text sessions + the rect; the empty one left no step");
});

test("rule 8: Text on existing text edits it; a double click in Select edits it; Enter on a selected text too", () => {
  let s = run(fresh("text"), [down(10, 10), up(10, 10), { type: "text", value: "Hello" }, { type: "escape" }]);
  const id = s.doc.objects[0].id;
  s = run(s, [down(20, 12), up(20, 12)]);
  assert.equal(s.text?.id, id, "a click with Text on it edits, not a new text");
  s = run(s, [{ type: "text", value: "Hello world" }, { type: "escape" }, { type: "tool", tool: "select" }]);
  s = run(s, [down(20, 12), up(20, 12), { type: "dbl", pt: [20, 12], button: 0, mods: NO, time: 1 }]);
  assert.equal(s.text?.id, id, "double click in Select edits");
  s = run(s, [{ type: "escape" }, down(20, 12), up(20, 12), key("Enter")]);
  assert.equal(s.text?.id, id, "Enter on the selected text edits");
  assert.equal(s.doc.objects.length, 1);
});

test("rule 8: corner handles change the font size, side handles the block width", () => {
  let s = run(fresh("text"), [down(10, 10), up(10, 10), { type: "text", value: "Some text" }, { type: "escape" }, { type: "tool", tool: "select" }]);
  const he = { unit: 1, measure, scale: 1 };
  const t0 = s.doc.objects[0];
  const box = I.shapeBox(t0, he);
  s = run(s, [down(box.x + 5, box.y + 5), up(box.x + 5, box.y + 5)]);
  const se = I.handlesOf(t0, he).find((h) => h.id === "se").pt; // a one-line text is small: the handle sits outside
  assert.ok(se[1] > box.y + box.h, "corner handle below the small text, not on it");
  s = run(s, drag(se[0], se[1], se[0] + 60, se[1] + 30));
  assert.ok(s.doc.objects[0].fontSize > t0.fontSize, "corner → bigger font");
  assert.equal(s.doc.objects[0].width, null, "auto width stays auto");
  const t1 = s.doc.objects[0];
  const b1 = I.shapeBox(t1, he);
  const e = I.handlesOf(t1, he).find((h) => h.id === "e").pt;
  s = run(s, drag(e[0], e[1], e[0] - 40, e[1]));
  assert.equal(s.doc.objects[0].fontSize, t1.fontSize, "side → same font");
  assert.ok(s.doc.objects[0].width !== null && s.doc.objects[0].width < b1.w, "side → narrower block (wraps)");
});

// ── rule 9: layers ───────────────────────────────────────────────────────────

test("rule 9: fixed layers — hide → spotlight → marker → shapes → counters, whatever the drawing order", () => {
  let s = fresh("counter");
  s = run(s, [down(10, 10), up(10, 10)]);
  for (const tool of ["rect", "marker", "spotlight", "redact", "pen"]) s = run(s, [{ type: "tool", tool }, ...drag(20, 20, 120, 80)]);
  assert.deepEqual(kinds(s), ["redact", "spotlight", "marker", "rect", "pen", "counter"]);
});

test("rule 9: Ctrl+] / Ctrl+[ move inside the layer, +Shift to its edge, never across layers", () => {
  let s = fresh("rect");
  for (let i = 0; i < 3; i++) s = run(s, drag(10 + i * 100, 10, 60 + i * 100, 60));
  s = run(s, [{ type: "tool", tool: "marker" }, ...drag(0, 200, 100, 200), { type: "tool", tool: "select" }]);
  const [m, a, b, c] = s.doc.objects.map((o) => o.id);
  s = run(s, [down(10, 30), up(10, 30)]); // select a (first rect)
  s = run(s, [key("BracketRight", { ctrl: true })]);
  assert.deepEqual(s.doc.objects.map((o) => o.id), [m, b, a, c], "one up");
  s = run(s, [key("BracketRight", { ctrl: true, shift: true })]);
  assert.deepEqual(s.doc.objects.map((o) => o.id), [m, b, c, a], "to the top of its layer");
  s = run(s, [key("BracketLeft", { ctrl: true, shift: true })]);
  assert.deepEqual(s.doc.objects.map((o) => o.id), [m, a, b, c], "to the bottom — still above the marker layer");
  assert.equal(s.hist.past.length, 7, "each move is a step");
});

test("rule 9: counters keep their numbers when z-order keys are used (LT П-12)", () => {
  let s = run(fresh("counter"), [down(10, 10), up(10, 10), down(50, 10), up(50, 10), down(90, 10), up(90, 10), { type: "tool", tool: "select" }, down(10, 10), up(10, 10)]);
  const before = [...I.counterNumbers(s.doc.objects).entries()];
  s = run(s, [key("BracketRight", { ctrl: true, shift: true })]);
  assert.deepEqual([...I.counterNumbers(s.doc.objects).entries()], before);
});

// ── rule 10: duplicate, nudge, axis, delete, select all ──────────────────────

test("rule 10: Alt+drag duplicates (the original stays); Ctrl+D duplicates with an offset", () => {
  let s = run(fresh("rect"), [drag(100, 100, 200, 200), { type: "tool", tool: "select" }]);
  const orig = structuredClone(s.doc.objects[0]);
  s = run(s, drag(100, 150, 300, 150, { alt: true }));
  assert.equal(s.doc.objects.length, 2);
  assert.deepEqual(s.doc.objects[0], orig, "original in place");
  assert.equal(s.doc.objects[1].r.x, 300, "the copy moved");
  assert.deepEqual(s.sel, [s.doc.objects[1].id], "the copy is selected");
  s = run(s, [key("KeyD", { ctrl: true })]);
  assert.equal(s.doc.objects.length, 3);
  assert.deepEqual([s.doc.objects[2].r.x, s.doc.objects[2].r.y], [310, 110]);
  // Alt-click without moving leaves no copy behind
  s = run(s, [down(310, 150, { alt: true }), up(310, 150, { alt: true })]);
  assert.equal(s.doc.objects.length, 3);
});

test("rule 10: arrows nudge 1 px (Shift — 10); Shift while dragging locks the axis", () => {
  let s = run(fresh("rect"), [drag(100, 100, 200, 200), { type: "tool", tool: "select" }, down(100, 150), up(100, 150)]);
  s = run(s, [key("ArrowRight"), key("ArrowDown", { shift: true })]);
  assert.deepEqual([s.doc.objects[0].r.x, s.doc.objects[0].r.y], [101, 110]);
  s = run(s, [down(101, 150), move(140, 157, { shift: true }), move(181, 165, { shift: true }), up(181, 165, { shift: true })]);
  assert.deepEqual([s.doc.objects[0].r.x, s.doc.objects[0].r.y], [181, 110], "horizontal only");
  // Shift at the PRESS toggles the selection instead (Shift+click)
  s = run(s, [down(181, 150, { shift: true }), up(181, 150, { shift: true })]);
  assert.deepEqual(s.sel, []);
});

test("rule 10: Delete removes the selection; Ctrl+A selects all and switches to Select", () => {
  let s = run(fresh("rect"), [drag(0, 0, 50, 50), drag(100, 0, 150, 50), drag(200, 0, 250, 50)]);
  s = run(s, [key("KeyA", { ctrl: true })]);
  assert.equal(s.tool, "select");
  assert.equal(s.sel.length, 3);
  s = run(s, [key("Delete")]);
  assert.equal(s.doc.objects.length, 0);
  assert.deepEqual(s.sel, []);
  assert.equal(s.$last.handled, true);
  assert.equal(run(s, [key("Delete")]).$last.handled, false, "Delete with nothing selected isn't eaten");
});

// ── rule 11: history ─────────────────────────────────────────────────────────

test("rule 11: every finished operation is one step — create, move, resize, property, delete, text session", () => {
  let s = run(fresh("rect"), drag(100, 100, 200, 200)); // create
  s = run(s, [{ type: "tool", tool: "select" }, ...drag(100, 150, 120, 150)]); // move
  s = run(s, drag(220, 200, 260, 240)); // resize by the se handle
  s = run(s, [key("Digit5")]); // property
  s = run(s, [{ type: "tool", tool: "text" }, down(400, 400), up(400, 400), { type: "text", value: "a" }, { type: "text", value: "ab" }, { type: "escape" }]); // text session
  s = run(s, [key("KeyA", { ctrl: true }), key("Delete")]); // delete
  assert.equal(s.hist.past.length, 6);
  for (let i = 0; i < 6; i++) s = run(s, [key("KeyZ", { ctrl: true })]);
  assert.equal(s.doc.objects.length, 0, "six undos → empty");
  s = run(s, [key("KeyZ", { ctrl: true, shift: true }), key("KeyY", { ctrl: true })]);
  assert.equal(s.doc.objects.length, 1, "Ctrl+Shift+Z and Ctrl+Y redo");
});

test("rule 11: nudges faster than 500 ms merge into one step; a pause splits them", () => {
  let s = run(fresh("rect"), [drag(0, 0, 50, 50), { type: "tool", tool: "select" }, down(0, 25), up(0, 25)]);
  const n0 = s.hist.past.length;
  const t = clock + 1000;
  s = run(s, [key("ArrowRight"), key("ArrowRight"), key("ArrowRight")].map((k, i) => ({ ...k, time: t + i * 100 })));
  assert.equal(s.hist.past.length, n0 + 1, "one step for the series");
  s = run(s, [{ ...key("ArrowRight"), time: t + 1200 }]);
  assert.equal(s.hist.past.length, n0 + 2);
  s = run(s, [key("KeyZ", { ctrl: true })]);
  assert.equal(s.doc.objects[0].r.x, 3);
  s = run(s, [key("KeyZ", { ctrl: true })]);
  assert.equal(s.doc.objects[0].r.x, 0, "the series undone in one go");
});

test("rule 11: Ctrl+Z during a gesture cancels the gesture (nothing else)", () => {
  let s = run(fresh("rect"), drag(0, 0, 50, 50));
  s = run(s, [down(100, 100), move(200, 200)]);
  assert.equal(s.doc.objects.length, 2);
  s = run(s, [key("KeyZ", { ctrl: true }), up(200, 200)]);
  assert.equal(s.doc.objects.length, 1, "the half-drawn rect is gone, the first one stays");
  assert.equal(s.hist.past.length, 1);
});

// ── rule 12: keys while typing; physical keys ────────────────────────────────

test("rule 12: single keys do nothing while text is typed or an IME composes", () => {
  let s = run(fresh("text"), [down(10, 10), up(10, 10)]);
  for (const code of ["KeyV", "KeyR", "Digit5", "Delete", "Enter", "BracketRight"]) {
    const r = I.dispatch(s, key(code), env());
    assert.equal(r.handled, true, `${code} eaten`);
    assert.equal(r.state.tool, "text");
    assert.equal(r.intent, undefined);
  }
  assert.equal(I.dispatch(s, key("Escape"), env()).handled, false, "Esc goes on to escape()");
  s = run(s, [{ type: "escape" }, { type: "tool", tool: "arrow" }]);
  const ime = I.dispatch(s, key("KeyR", { composing: true }), env());
  assert.equal(ime.state.tool, "arrow", "IME composition: no tool switch");
});

test("rule 12: shortcuts match PHYSICAL keys — Ctrl+Я is Ctrl+Z (code KeyZ), К is R", () => {
  let s = run(fresh("rect"), drag(0, 0, 50, 50));
  s = run(s, [{ ...key("KeyZ", { ctrl: true }), key: "я" }]);
  assert.equal(s.doc.objects.length, 0, "undone");
  s = run(s, [{ ...key("KeyE"), key: "у" }]);
  assert.equal(s.tool, "ellipse");
  s = run(s, [{ ...key("KeyO"), key: "щ" }, { ...key("KeyD"), key: "в" }]);
  assert.equal(s.tool, "pen", "D = pen as well");
});

// ── the rest of SPEC §9.3 ────────────────────────────────────────────────────

test("Esc ladder: gesture → text → selection → Select tool → nothing left (false)", () => {
  let s = run(fresh("rect"), [drag(0, 0, 50, 50), { type: "tool", tool: "select" }, down(0, 25), up(0, 25), { type: "tool", tool: "rect" }]);
  s = run(s, [{ type: "tool", tool: "select" }, down(0, 25), up(0, 25), { type: "tool", tool: "rect" }]);
  s = run(s, [down(100, 100), move(150, 150)]);
  let r = I.escapeStep(s);
  assert.ok(r.handled && !r.state.gesture && r.state.doc.objects.length === 1, "1: gesture cancelled");
  s = run(r.state, [{ type: "tool", tool: "text" }, down(300, 300), up(300, 300), { type: "text", value: "x" }]);
  r = I.escapeStep(s);
  assert.ok(r.handled && !r.state.text && r.state.doc.objects.length === 2, "2: text committed");
  s = run(r.state, [{ type: "tool", tool: "select" }, down(0, 25), up(0, 25), { type: "tool", tool: "rect" }]);
  s = { ...s, sel: [s.doc.objects[0].id] };
  r = I.escapeStep(s);
  assert.ok(r.handled && r.state.sel.length === 0 && r.state.tool === "rect", "3: selection cleared");
  r = I.escapeStep(r.state);
  assert.ok(r.handled && r.state.tool === "select", "4: Select tool");
  r = I.escapeStep(r.state);
  assert.equal(r.handled, false, "5: nothing to unwind — the host decides");
});

test("right click during drawing cancels the object; otherwise it does nothing (no deleting)", () => {
  let s = run(fresh("rect"), [down(10, 10), move(80, 80), down(80, 80, {}, 2)]);
  assert.equal(s.doc.objects.length, 0);
  assert.equal(s.gesture, null);
  s = run(s, [up(80, 80), ...drag(10, 10, 80, 80), down(10, 40, {}, 2)]);
  assert.equal(s.doc.objects.length, 1, "a right click on an object never deletes it (LT П-13)");
});

test("counters: auto numbers, deleting renumbers, a typed digit sets one number and the next follow", () => {
  let s = run(fresh("counter"), [0, 1, 2, 3].map((i) => [down(10 + i * 40, 10), up(10 + i * 40, 10)]));
  const nums = () => [...I.counterNumbers(s.doc.objects).values()];
  assert.deepEqual(nums(), [1, 2, 3, 4]);
  s = run(s, [{ type: "tool", tool: "select" }, down(50, 10), up(50, 10), key("Delete")]);
  assert.deepEqual(nums(), [1, 2, 3], "1,2,3,4 → delete 2 → 1,2,3");
  s = run(s, [down(10, 10), up(10, 10)]);
  const t = clock + 100;
  s = run(s, [{ ...key("Digit1"), time: t }, { ...key("Digit0"), time: t + 200 }]);
  assert.deepEqual(nums(), [10, 11, 12], "typed 1,0 → 10; the next continue");
  assert.equal(s.defaults.tools.counter.size, 3, "digits on a counter don't change the size");
});

test("overlay: presses outside the frame are ignored; drawing is clamped into it", () => {
  const e = env({ mode: "overlay", clip: { x: 100, y: 100, w: 200, h: 100 } });
  let s = run(fresh("rect"), drag(10, 10, 60, 60), e);
  assert.equal(s.doc.objects.length, 0);
  assert.equal(s.$last.handled, false, "outside — left to the host");
  s = run(s, drag(150, 150, 900, 900), e);
  assert.deepEqual(s.doc.objects[0].r, { x: 150, y: 150, w: 150, h: 50 }, "clamped to the frame");
  assert.equal(I.dispatch(s, key("KeyC"), e).state.tool, "rect", "no crop tool in the overlay");
});

test("crop (window): Enter applies, Esc cancels, C again returns the full picture; it is undoable", () => {
  let s = run(fresh("rect", 400, 300), [{ type: "tool", tool: "crop" }]);
  assert.deepEqual(s.crop, { x: 0, y: 0, w: 400, h: 300 });
  s = run(s, drag(50, 50, 250, 200));
  assert.deepEqual(s.crop, { x: 50, y: 50, w: 200, h: 150 });
  s = run(s, [key("Enter")]);
  assert.deepEqual(s.doc.crop, { x: 50, y: 50, w: 200, h: 150 });
  assert.equal(s.tool, "rect", "back to the previous tool");
  assert.equal(s.$last.intent, undefined, "Enter applied the crop, not Done");
  s = run(s, [{ type: "tool", tool: "crop" }, ...drag(100, 100, 120, 100), { type: "escape" }]);
  assert.deepEqual(s.doc.crop, { x: 50, y: 50, w: 200, h: 150 }, "Esc cancelled the change");
  s = run(s, [{ type: "tool", tool: "crop" }, key("KeyC"), key("Enter")]);
  assert.equal(s.doc.crop, null, "C again → full picture");
  s = run(s, [key("KeyZ", { ctrl: true })]);
  assert.deepEqual(s.doc.crop, { x: 50, y: 50, w: 200, h: 150 }, "undo brings the crop back");
});

test("eyedropper: the sampled colour becomes the current colour; the previous tool comes back", () => {
  let s = run(fresh("arrow"), [{ type: "tool", tool: "eyedropper" }]);
  s = run(s, [down(5, 5), up(5, 5)], env({ sample: () => "#123456" }));
  assert.equal(s.tool, "arrow");
  assert.equal(s.defaults.tools.arrow.color, "#123456");
  assert.deepEqual(s.defaults.recent, ["#123456"]);
});

test("a pasted picture is the last object (not selected); the tool doesn't change", () => {
  const s = run(fresh("pen"), [{ type: "image", src: "data:image/png;base64,AA==", box: { x: 0, y: 0, w: 10, h: 10 }, time: 1 }]);
  assert.equal(s.doc.objects[0].kind, "image");
  assert.deepEqual(s.sel, []);
  assert.equal(s.last, s.doc.objects[0].id);
  assert.equal(s.tool, "pen", "LT switched to Select (П-5)");
});

test("intents: Enter → done, Ctrl+C / Ctrl+S / Ctrl+Shift+S / Ctrl+P / Ctrl+W; not eaten without a listener", () => {
  const s = fresh("arrow");
  const cases = [
    [key("Enter"), "done"],
    [key("KeyC", { ctrl: true }), "copy"],
    [key("KeyS", { ctrl: true }), "save"],
    [key("KeyS", { ctrl: true, shift: true }), "save_as"],
    [key("KeyP", { ctrl: true }), "pin"],
    [key("KeyW", { ctrl: true }), "close"],
  ];
  for (const [k, want] of cases) {
    const r = I.dispatch(s, k, env());
    assert.equal(r.intent, want);
    assert.equal(r.handled, true);
    const bare = I.dispatch(s, k, env({ intents: false }));
    assert.equal(bare.handled, false, `${want} left to the host`);
    assert.equal(bare.intent, undefined);
  }
  assert.equal(I.dispatch(s, key("KeyV", { ctrl: true }), env()).handled, false, "Ctrl+V reaches the paste event");
  assert.equal(I.dispatch(s, key("Escape"), env()).handled, false, "Esc is escape()'s job");
  const ov = env({ mode: "overlay", clip: { x: 0, y: 0, w: 800, h: 600 } });
  const dbl = I.dispatch(fresh("select"), { type: "dbl", pt: [400, 300], button: 0, mods: NO, time: 1 }, ov);
  assert.equal(dbl.intent, "done", "overlay: double click on empty space with Select → Done");
});

test("dirty: any change makes the document dirty; undo back to the start or 'saved' cleans it", () => {
  let s = fresh("rect");
  assert.equal(I.isDirty(s), false);
  s = run(s, drag(0, 0, 50, 50));
  assert.equal(I.isDirty(s), true);
  s = run(s, [key("KeyZ", { ctrl: true })]);
  assert.equal(I.isDirty(s), false, "undone to the loaded state");
  s = run(s, [...drag(0, 0, 50, 50), { type: "saved" }]);
  assert.equal(I.isDirty(s), false);
});

test("the properties bar: what applies to the current tool / selection", () => {
  assert.deepEqual(I.propsFor(fresh("rect")), ["color", "size", "fill", "round"]);
  assert.deepEqual(I.propsFor(fresh("arrow")), ["color", "size", "arrowStyle"]);
  assert.deepEqual(I.propsFor(fresh("text")), ["color", "size", "textStyle"]);
  assert.deepEqual(I.propsFor(fresh("redact")), ["color", "size", "redactMode"]);
  assert.deepEqual(I.propsFor(fresh("spotlight")), ["spotShape"]);
  assert.deepEqual(I.propsFor(fresh("select")), []);
  assert.equal(I.propTarget(fresh("select")), null);
  // F toggles outline ↔ fill on the rect tool
  const f = run(fresh("rect"), [key("KeyF")]);
  assert.equal(f.defaults.tools.rect.fill, true);
  // Shift+B: the next way to hide
  const b = run(fresh("rect"), [key("KeyB", { shift: true })]);
  assert.equal(b.tool, "redact");
  assert.equal(b.defaults.tools.redact.redactMode, "pixelate");
});
