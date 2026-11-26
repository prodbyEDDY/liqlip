// Annotation document v1 (W6-C, ARCHITECTURE §18.6): serialization round trip, unknown versions,
// untrusted input, layer order, counter numbers, the frame crop the overlay commits; plus the
// undo history (structural sharing, 500 ms merging) and the tool defaults (per tool, persisted).
import assert from "node:assert/strict";
import test from "node:test";
import { importTs } from "./helpers/bundle.mjs";

const D = await importTs("src/lib/editor/doc.ts", "src/lib/editor/history.ts", "src/lib/editor/defaults.ts");

const sample = () => ({
  v: 1,
  w: 800,
  h: 600,
  unit: 1.5,
  crop: { x: 10, y: 20, w: 300, h: 200 },
  objects: [
    { id: "a", color: "#ff3b30", kind: "arrow", a: [1, 2], b: [100, 50], bend: 22, style: "curved", size: 3 },
    { id: "l", color: "#000000", kind: "line", a: [0, 0], b: [10, 10], size: 1 },
    { id: "r", color: "#007aff", kind: "rect", r: { x: 5, y: 5, w: 50, h: 40 }, fill: true, size: 2, round: true },
    { id: "e", color: "#34c759", kind: "ellipse", r: { x: 5, y: 5, w: 50, h: 40 }, fill: false, size: 4 },
    { id: "p", color: "#ff3b30", kind: "pen", points: [[1, 1], [2, 3], [5, 8]], size: 3 },
    { id: "t", color: "#ffffff", kind: "text", at: [40, 40], width: 120, text: "Привет\nworld", fontSize: 24, style: "badge" },
    { id: "s", kind: "spotlight", r: { x: 0, y: 0, w: 20, h: 20 }, shape: "ellipse" },
    { id: "i", kind: "image", r: { x: 1, y: 1, w: 4, h: 4 }, src: "data:image/png;base64,AAAA" },
    { id: "m", color: "#ffcc00", kind: "marker", points: [[0, 5], [50, 5]], size: 3 },
    { id: "x", color: "#000000", kind: "redact", r: { x: 60, y: 60, w: 30, h: 10 }, mode: "pixelate", size: 5 },
    { id: "c1", color: "#ff3b30", kind: "counter", at: [10, 10], n: null, size: 3 },
    { id: "c2", color: "#ff3b30", kind: "counter", at: [20, 10], n: 7, size: 3 },
    { id: "c3", color: "#ff3b30", kind: "counter", at: [30, 10], n: null, size: 3 },
  ],
});

test("serialization v1: save → load gives the same document (every kind)", () => {
  const { doc, unknownVersion, dropped } = D.loadDoc(JSON.parse(D.serializeDoc(D.loadDoc(sample(), 800, 600).doc)), 800, 600);
  assert.equal(unknownVersion, false);
  assert.equal(dropped, 0);
  const again = D.loadDoc(JSON.parse(D.serializeDoc(doc)), 800, 600).doc;
  assert.deepEqual(again, doc);
  assert.equal(doc.unit, 1.5);
  assert.deepEqual(doc.crop, { x: 10, y: 20, w: 300, h: 200 });
  assert.equal(doc.objects.length, 13);
  assert.equal(doc.objects.find((o) => o.id === "r").round, true, "the additive `round` survives");
  assert.equal(doc.objects.find((o) => o.id === "t").text, "Привет\nworld");
});

test("loading puts objects back in layer order: hide → spotlight → marker → shapes → counters", () => {
  const { doc } = D.loadDoc(sample(), 800, 600);
  const layers = doc.objects.map((o) => D.layerOf(o.kind));
  assert.deepEqual(layers, [...layers].sort((a, b) => a - b));
  assert.equal(doc.objects[0].kind, "redact");
  assert.equal(doc.objects[1].kind, "spotlight");
  assert.equal(doc.objects[2].kind, "marker");
  assert.equal(doc.objects.at(-1).kind, "counter");
  // order inside a layer (z-order) is kept
  assert.deepEqual(
    doc.objects.filter((o) => D.layerOf(o.kind) === 3).map((o) => o.id),
    ["a", "l", "r", "e", "p", "t", "i"],
  );
});

test("an unknown version opens the picture without objects (the caller shows a toast)", () => {
  const r = D.loadDoc({ ...sample(), v: 2 }, 640, 480);
  assert.equal(r.unknownVersion, true);
  assert.equal(r.doc.objects.length, 0);
  assert.deepEqual([r.doc.w, r.doc.h], [640, 480]);
  const n = D.loadDoc(null, 10, 20, 2);
  assert.equal(n.unknownVersion, false);
  assert.deepEqual([n.doc.w, n.doc.h, n.doc.unit, n.doc.objects.length], [10, 20, 2, 0]);
});

test("untrusted input: extra fields ignored, broken objects dropped, ids made unique", () => {
  const raw = {
    v: 1,
    w: 1,
    h: 1,
    unit: "x",
    crop: { x: -50, y: 0, w: 9999, h: 10 },
    objects: [
      { id: "a", kind: "rect", color: "red", r: { x: 0, y: 0, w: -10, h: 5 }, fill: 1, size: 9, extra: { deep: true } },
      { id: "a", kind: "line", a: [0, 0], b: [1, 1], size: 2, color: "#ABCDEF" },
      { kind: "pen", points: [] },
      { kind: "image", r: { x: 0, y: 0, w: 1, h: 1 }, src: "javascript:alert(1)" },
      { kind: "wat" },
      "nope",
    ],
  };
  const { doc, dropped } = D.loadDoc(raw, 100, 100);
  assert.equal(dropped, 4);
  assert.equal(doc.unit, 1, "bad unit → default");
  assert.deepEqual(doc.crop, { x: 0, y: 0, w: 100, h: 10 }, "crop clamped into the picture");
  const [r, l] = doc.objects;
  assert.deepEqual(r.r, { x: -10, y: 0, w: 10, h: 5 }, "negative size normalised");
  assert.equal(r.color, "#ff3b30", "bad colour → default");
  assert.equal(r.fill, false);
  assert.equal(r.size, 3, "bad size → 3");
  assert.equal("extra" in r, false);
  assert.notEqual(l.id, r.id, "duplicate id renamed");
  assert.equal(l.color, "#abcdef");
});

test("counters: explicit n, else previous + 1; numbers follow the array, so deleting renumbers", () => {
  const { doc } = D.loadDoc(sample(), 800, 600);
  assert.deepEqual([...D.counterNumbers(doc.objects).values()], [1, 7, 8]);
  const without = doc.objects.filter((o) => o.id !== "c1");
  assert.deepEqual([...D.counterNumbers(without).values()], [7, 8]);
  const plain = [1, 2, 3].map((i) => ({ id: `k${i}`, kind: "counter", color: "#000000", at: [0, 0], n: null, size: 3 }));
  assert.deepEqual([...D.counterNumbers(plain.filter((o) => o.id !== "k2")).values()], [1, 2], "1,2,3 → delete 2 → 1,2");
});

test("cropDoc: shifted to the frame, objects entirely outside dropped, size = frame", () => {
  const doc = {
    v: 1,
    w: 1000,
    h: 1000,
    unit: 2,
    crop: null,
    objects: [
      { id: "in", kind: "rect", color: "#000000", r: { x: 110, y: 120, w: 10, h: 10 }, fill: false, size: 3 },
      { id: "edge", kind: "line", color: "#000000", a: [90, 150], b: [130, 150], size: 3 },
      { id: "out", kind: "counter", color: "#000000", at: [900, 900], n: null, size: 3 },
    ],
  };
  const box = (o) => (o.kind === "rect" ? o.r : o.kind === "line" ? { x: 90, y: 150, w: 40, h: 0 } : { x: 880, y: 880, w: 40, h: 40 });
  const out = D.cropDoc(doc, { x: 100, y: 100, w: 200, h: 100 }, box);
  assert.deepEqual([out.w, out.h, out.unit], [200, 100, 2]);
  assert.deepEqual(
    out.objects.map((o) => o.id),
    ["in", "edge"],
  );
  assert.deepEqual(out.objects[0].r, { x: 10, y: 20, w: 10, h: 10 });
  assert.deepEqual(out.objects[1].a, [-10, 50]);
  assert.equal(doc.objects[0].r.x, 110, "input untouched");
});

// ── history (rule 11) ────────────────────────────────────────────────────────

test("history: one step per operation; undo/redo restore snapshots; redo cleared by a new step", () => {
  const s0 = { objects: [], crop: null },
    s1 = { objects: [{ id: "1" }], crop: null },
    s2 = { objects: [{ id: "1" }, { id: "2" }], crop: null };
  let h = D.record(D.emptyHistory(), s0, null, 0);
  h = D.record(h, s1, null, 10);
  assert.equal(h.past.length, 2);
  const u = D.undo(h, s2);
  assert.equal(u.snap, s1);
  const r = D.redo(u.history, s1);
  assert.equal(r.snap, s2);
  const n = D.record(u.history, s1, null, 20);
  assert.equal(n.future.length, 0);
  assert.equal(D.undo(D.emptyHistory(), s0), null);
});

test("history: a series with the same key faster than 500 ms is ONE step; slower or another key is not", () => {
  const s = (i) => ({ objects: [{ id: String(i) }], crop: null });
  let h = D.emptyHistory();
  h = D.record(h, s(0), "nudge:a", 1000);
  h = D.record(h, s(1), "nudge:a", 1200);
  h = D.record(h, s(2), "nudge:a", 1650);
  assert.equal(h.past.length, 1, "three nudges, each < 500 ms after the previous");
  assert.deepEqual(h.past[0], s(0), "undo goes back to before the series");
  h = D.record(h, s(3), "nudge:a", 2300);
  assert.equal(h.past.length, 2, "a pause > 500 ms starts a new step");
  h = D.record(h, s(4), "prop:size:a", 2400);
  assert.equal(h.past.length, 3, "another key starts a new step");
  h = D.record(h, s(5), null, 2410);
  h = D.record(h, s(6), null, 2420);
  assert.equal(h.past.length, 5, "no key → never merged");
});

test("history keeps at most 100 steps", () => {
  let h = D.emptyHistory();
  for (let i = 0; i < 130; i++) h = D.record(h, { objects: [], crop: null }, null, i);
  assert.equal(h.past.length, D.HISTORY_MAX);
});

// ── defaults (rule 4: per tool, between launches) ────────────────────────────

test("defaults: per tool, persisted and read back field by field; broken values fall back", () => {
  const mem = new Map();
  const storage = { getItem: (k) => mem.get(k) ?? null, setItem: (k, v) => mem.set(k, v), removeItem: (k) => mem.delete(k) };
  const d = D.factoryDefaults();
  assert.equal(d.tools.arrow.color, "#ff3b30", "red by default");
  assert.equal(d.tools.marker.color, "#ffcc00");
  d.tools.arrow = { ...d.tools.arrow, size: 5, arrowStyle: "double" };
  d.recent = ["#123456"];
  D.saveDefaults(d, storage);
  const back = D.loadDefaults(storage);
  assert.equal(back.tools.arrow.size, 5);
  assert.equal(back.tools.arrow.arrowStyle, "double");
  assert.equal(back.tools.rect.size, 3, "other tools untouched");
  assert.deepEqual(back.recent, ["#123456"]);
  mem.set(D.STORAGE_KEY, JSON.stringify({ tools: { pen: { size: 99, color: "nope", fill: true } } }));
  const fb = D.loadDefaults(storage);
  assert.equal(fb.tools.pen.size, 3);
  assert.equal(fb.tools.pen.color, "#ff3b30");
  assert.equal(fb.tools.pen.fill, true);
  mem.set(D.STORAGE_KEY, "{broken");
  assert.deepEqual(D.loadDefaults(storage), D.factoryDefaults());
  D.resetDefaults(storage);
  assert.equal(mem.has(D.STORAGE_KEY), false);
});

test("recent colours: custom only, newest first, 3 at most", () => {
  let r = [];
  r = D.pushRecent(r, "#ff3b30");
  assert.deepEqual(r, [], "a preset is not 'recent'");
  for (const c of ["#111111", "#222222", "#333333", "#444444", "#222222"]) r = D.pushRecent(r, c);
  assert.deepEqual(r, ["#222222", "#444444", "#333333"]);
});
