// W7-R3 review of the annotation editor: edge cases found by playing it in the browser — each
// fix pinned by a test on the pure modules (interaction.ts, hit.ts, ui.ts).
import assert from "node:assert/strict";
import test from "node:test";
import { importTs } from "./helpers/bundle.mjs";

const I = await importTs("src/lib/editor/interaction.ts", "src/lib/editor/doc.ts", "src/lib/editor/defaults.ts", "src/lib/editor/hit.ts", "src/lib/editor/ui.ts");

const measure = (text, size) => (Array.from(text).length * size) / 2;
const env = (over = {}) => ({ mode: "window", scale: 1, measure, clip: null, intents: true, ...over });
const NO = { shift: false, ctrl: false, alt: false };
let clock = 50_000;
const ptr = (type, x, y, mods = {}, button = 0) => ({ type, pt: [x, y], button, mods: { ...NO, ...mods }, time: (clock += 16) });
const down = (x, y, m, b) => ptr("down", x, y, m, b);
const move = (x, y, m) => ptr("move", x, y, m);
const up = (x, y, m) => ptr("up", x, y, m);
const drag = (x0, y0, x1, y1, m = {}) => [down(x0, y0, m), move((x0 + x1) / 2, (y0 + y1) / 2, m), move(x1, y1, m), up(x1, y1, m)];
const key = (code, m = {}) => ({ type: "key", code, ctrl: false, shift: false, alt: false, repeat: false, composing: false, time: (clock += 16), ...m });
const fresh = (tool = "rect") => I.createState(I.emptyDoc(800, 600, 1), I.factoryDefaults(), tool);
function run(s, events, e = env()) {
  let r = { state: s, handled: false };
  for (const ev of events.flat()) r = I.dispatch(r.state, ev, e);
  return Object.assign(r.state, { $last: r });
}

test("a marker click leaves nothing (its flat caps would make an invisible object); a pen click is a dot", () => {
  let s = run(fresh("marker"), [down(100, 100), up(100, 100)]);
  assert.equal(s.doc.objects.length, 0, "no invisible marker");
  assert.equal(I.isDirty(s), false);
  s = run(s, [{ type: "tool", tool: "pen" }, down(100, 100), up(100, 100)]);
  assert.deepEqual(s.doc.objects.map((o) => o.kind), ["pen"], "the pen keeps its dot");
  s = run(s, [{ type: "tool", tool: "marker" }, ...drag(100, 200, 300, 200)]);
  assert.deepEqual(s.doc.objects.map((o) => o.kind), ["marker", "pen"], "a real marker stroke is kept (under the pen: its layer)");
});

test("after a move / an Alt-copy the hover is what is under the pointer now (no stale outline)", () => {
  let s = run(fresh("rect"), [...drag(100, 100, 200, 200), { type: "tool", tool: "select" }]);
  const orig = s.doc.objects[0].id;
  s = run(s, [move(100, 150), down(100, 150, { alt: true }), move(300, 150, { alt: true }), move(400, 150, { alt: true }), up(400, 150, { alt: true })]);
  assert.equal(s.doc.objects.length, 2);
  const copy = s.doc.objects.find((o) => o.id !== orig).id;
  assert.equal(s.hover, copy, "the pointer is on the copy");
});

test("re-picking the current colour or size leaves no empty undo step and doesn't make the document dirty", () => {
  let s = run(fresh("rect"), drag(100, 100, 200, 200));
  const steps = s.hist.past.length;
  s = run(s, [{ type: "prop", prop: "color", value: s.defaults.tools.rect.color, time: (clock += 900) }, key("Digit3")]);
  assert.equal(s.hist.past.length, steps, "same colour, same size: nothing recorded");
  s = run(s, [{ type: "tool", tool: "select" }, down(100, 150), up(100, 150), key("Digit3")]);
  assert.equal(s.hist.past.length, steps);
  s = run(s, [key("Digit4")]);
  assert.equal(s.hist.past.length, steps + 1, "a real change still records");
  // a document that is back at its saved state is clean again after a no-op
  let t = run(fresh("rect"), [...drag(0, 0, 50, 50), { type: "saved" }, { type: "tool", tool: "select" }, down(0, 25), up(0, 25)]);
  t = run(t, [{ type: "prop", prop: "fill", value: false, time: (clock += 900) }]);
  assert.equal(I.isDirty(t), false);
});

test("the system colour picker's live drag doesn't flood the recent colours; its final pick does", () => {
  let s = fresh("arrow");
  for (const c of ["#101010", "#202020", "#303030", "#404040"]) s = run(s, [{ type: "prop", prop: "color", value: c, time: (clock += 16), recent: false }]);
  assert.deepEqual(s.defaults.recent, [], "live preview: not recent");
  assert.equal(s.defaults.tools.arrow.color, "#404040", "…but applied");
  s = run(s, [{ type: "prop", prop: "color", value: "#404040", time: (clock += 16) }]);
  assert.deepEqual(s.defaults.recent, ["#404040"], "the final colour is recent");
});

test("pictures scale with their proportions: corner handles only", () => {
  const he = { unit: 1, measure, scale: 1 };
  const img = { id: "i", kind: "image", r: { x: 0, y: 0, w: 400, h: 200 }, src: "data:image/png;base64,AA" };
  assert.deepEqual(I.handlesOf(img, he).map((h) => h.id).sort(), ["ne", "nw", "se", "sw"]);
});

test("handles never cover a small object or a text (one line or big)", () => {
  const he = { unit: 1, measure, scale: 1 };
  const clear = (r, hs, what) => {
    for (const h of hs) {
      const [x, y] = h.pt;
      const d = Math.max(r.x - x, x - (r.x + r.w), r.y - y, y - (r.y + r.h)); // > 0 outside
      assert.ok(d >= I.HANDLE_R, `${what}: ${h.id} at ${x.toFixed(1)},${y.toFixed(1)} touches the object`);
    }
  };
  // a 50 × 40 rectangle (W6: eight handles hid most of its outline)
  const rect = { id: "r", kind: "rect", color: "#000000", r: { x: 100, y: 100, w: 50, h: 40 }, fill: false, size: 3 };
  clear(rect.r, I.handlesOf(rect, he), "50×40 rect");
  // a thin strip: its side handles step out too
  const strip = { ...rect, r: { x: 100, y: 100, w: 400, h: 14 } };
  clear(strip.r, I.handlesOf(strip, he), "strip");
  // a one-line text and a big multi-line one: never on the glyph box
  for (const [text, fontSize] of [["One line", 18], ["Big\ntext\nhere", 48]]) {
    const t = { id: "t", kind: "text", color: "#000000", at: [100, 100], width: null, text, fontSize, style: "plain" };
    clear(I.shapeBox(t, he), I.handlesOf(t, he), `text ${fontSize}`);
  }
  // a big rectangle keeps its handles on the corners
  assert.deepEqual(I.handlesOf({ ...rect, r: { x: 0, y: 0, w: 300, h: 200 } }, he).find((h) => h.id === "se").pt, [300, 200]);
});

test("a right button pressed during a drag is seen in the move it arrives with (chorded press)", () => {
  assert.equal(I.secondaryPress({ button: 2, buttons: 3 }), true, "left held + right pressed");
  assert.equal(I.secondaryPress({ button: 2, buttons: 1 }), false, "right released");
  assert.equal(I.secondaryPress({ button: -1, buttons: 3 }), false, "a plain move with both held");
  // and the dispatcher cancels the object for it
  const s = run(fresh("rect"), [down(10, 10), move(200, 200), down(200, 200, {}, 2), up(220, 220)]);
  assert.equal(s.doc.objects.length, 0);
});

test("keys in the text field: IME owns its keys (Esc too), browser accelerators are swallowed, Ctrl+S finishes and saves", () => {
  const k = (code, o = {}) => I.textKey({ code, ctrlKey: false, shiftKey: false, altKey: false, metaKey: false, isComposing: false, keyCode: 0, ...o });
  assert.equal(k("Escape", { isComposing: true }), "ime", "Esc cancels the composition, not the text");
  assert.equal(k("KeyA", { keyCode: 229 }), "ime");
  assert.equal(k("Escape"), "escape");
  assert.equal(k("Enter", { ctrlKey: true }), "commit");
  assert.equal(k("Enter"), "type", "Enter = a new line");
  for (const [code, shift] of [["KeyS", false], ["KeyS", true], ["KeyP", false], ["KeyW", false]]) assert.equal(k(code, { ctrlKey: true, shiftKey: shift }), "action", code);
  for (const c of [["F5", {}], ["KeyR", { ctrlKey: true }], ["KeyF", { ctrlKey: true }], ["ArrowLeft", { altKey: true }]]) assert.equal(k(c[0], c[1]), "browser", c[0]);
  for (const c of [["KeyZ", { ctrlKey: true }], ["KeyC", { ctrlKey: true }], ["KeyA", { ctrlKey: true }], ["KeyV", { ctrlKey: true }], ["KeyT", {}], ["Digit1", { shiftKey: true }]]) assert.equal(k(c[0], c[1]), "type", c[0]);
  // the field's "action": commit the text, then the same key at rest → the host's intent
  let s = run(fresh("text"), [down(10, 10), up(10, 10), { type: "text", value: "Label" }, { type: "textCommit" }]);
  s = run(s, [key("KeyS", { ctrl: true })]);
  assert.equal(s.$last.intent, "save");
  assert.equal(s.doc.objects[0].text, "Label");
});

test("tooltips slide back inside the window; popovers open where they fit", () => {
  assert.equal(I.tipShift(-23, 87, 1280), 31, "first tool: pushed right to the margin");
  assert.equal(I.tipShift(1180, 1300, 1280), -28, "last button: pushed left");
  assert.equal(I.tipShift(100, 200, 1280), 0, "fits: untouched");
  assert.equal(I.popSide({ top: 600, bottom: 640 }, 180, 720), "above", "no room below → above");
  assert.equal(I.popSide({ top: 60, bottom: 100 }, 180, 720), "below");
  assert.equal(I.popSide({ top: 100, bottom: 140 }, 180, 300), "below", "neither fits: the roomier side");
});

test("live gestures name the objects they reshape (the renderer previews a blur only for them)", () => {
  let s = run(fresh("redact"), [down(10, 10), move(100, 100)]);
  assert.deepEqual([...I.liveIds(s)], [s.doc.objects[0].id], "creating");
  s = run(s, [up(100, 100), { type: "tool", tool: "select" }, down(10, 50), move(40, 50)]);
  assert.deepEqual([...I.liveIds(s)], [s.doc.objects[0].id], "moving");
  s = run(s, [up(40, 50)]);
  assert.equal(I.liveIds(s), null, "at rest: the exact effect");
});
