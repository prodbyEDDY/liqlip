// Text layout (W6-C). The 8 tests are Liquid Tools `tests/annotationText.test.mjs` ported as they
// were — the LT "bubble" is Liqlip's "badge" style, and the padding (0.1 em / 0.7 × 0.45 em) gives
// exactly LT's 2 px / 14 × 9 px at 20 px, so every number stays. Plus the cache and the v1 adapters.
import assert from "node:assert/strict";
import test from "node:test";
import { importTs } from "./helpers/bundle.mjs";

const T = await importTs("src/lib/editor/textLayout.ts");
const { layoutText, fitText, resizeText, paintText, TEXT_MIN, TEXT_MAX } = T;
const measure = (text, size) => (Array.from(text).length * size) / 2;
const shape = (extra = {}) =>
  fitText(
    {
      type: "text",
      text: "Alpha beta gamma",
      fontSize: 20,
      font: "Arial",
      x0: 100,
      y0: 150,
      x1: 100,
      y1: 150,
      ...extra,
    },
    measure,
  );

test("manual line breaks and blank lines survive layout without width clipping", () => {
  const s = shape({ text: "One\r\n\r\nThree" });
  const layout = layoutText(s, measure);
  assert.deepEqual(Array.from(layout.lines), ["One", "", "Three"]);
  assert.equal(layout.w, 54);
  assert.equal(layout.h, 79);
  assert.equal(s.y1 - s.y0, layout.h);
});

test("fixed text width wraps whole words, never creates a whitespace-only line", () => {
  const layout = layoutText(shape({ text: "Hello world", textWidth: 54 }), measure);
  assert.deepEqual(Array.from(layout.lines), ["Hello ", "world"]);
  assert.equal(layout.w, 54);
  assert.equal(layout.h, 54);
});

test("long tokens wrap without splitting emoji or combining characters", () => {
  const text = "á👨‍👩‍👧‍👦abcdef";
  const layout = layoutText(shape({ text, textWidth: 44 }), measure);
  assert.equal(layout.lines.join(""), text);
  assert.ok(layout.lines.includes("👨‍👩‍👧‍👦"));
  assert.ok(layout.lines[0].startsWith("á"));
});

test("right handle changes width and height through reflow, not font size", () => {
  const start = shape();
  const next = resizeText(start, 3, -100, 80, measure);
  assert.equal(next.fontSize, start.fontSize);
  assert.equal(next.x0, start.x0);
  assert.equal(next.y0, start.y0);
  assert.equal(next.x1 - next.x0, start.x1 - start.x0 - 100);
  assert.ok(next.y1 > start.y1);
  assert.equal(layoutText(next, measure).w, next.x1 - next.x0);
});

test("left handle pins the opposite edge and clamps crossing to a usable width", () => {
  const start = shape({ type: "badge" });
  const next = resizeText(start, 7, 10000, 0, measure);
  assert.equal(next.x1, start.x1);
  assert.equal(next.textWidth, 48);
  assert.equal(next.fontSize, 20);
  assert.ok(next.y1 > next.y0);
});

test("each corner scales text and keeps its opposite corner pinned", () => {
  const start = shape({ textWidth: 134 });
  for (const node of [0, 2, 4, 6]) {
    const left = node === 0 || node === 6;
    const top = node === 0 || node === 2;
    const next = resizeText(start, node, left ? -80 : 80, top ? -40 : 40, measure);
    assert.ok(next.fontSize > start.fontSize);
    assert.equal(left ? next.x1 : next.x0, left ? start.x1 : start.x0);
    assert.equal(top ? next.y1 : next.y0, top ? start.y1 : start.y0);
    assert.equal(next.y1 - next.y0, layoutText(next, measure).h);
  }
});

test("corner drag across the anchor or far outside the screen never inverts text", () => {
  const start = shape();
  const small = resizeText(start, 4, -10000, -10000, measure);
  const large = resizeText(start, 4, 10000, 10000, measure);
  assert.equal(small.fontSize, TEXT_MIN);
  assert.equal(large.fontSize, TEXT_MAX);
  assert.ok(small.x1 > small.x0 && small.y1 > small.y0);
});

test("canvas preview/export use only filled glyphs and the same wrapped line layout", () => {
  const s = shape({ textWidth: 64 });
  const calls = [];
  const ctx = {
    measureText: () => ({ fontBoundingBoxAscent: 15, fontBoundingBoxDescent: 5 }),
    fillText: (text, x, y) => calls.push({ text, x, y }),
    strokeText: () => assert.fail("annotation text must never acquire an outline"),
  };
  paintText(ctx, s, "#15171c", measure);
  assert.equal(ctx.fillStyle, "#15171c");
  assert.deepEqual(
    calls.map((c) => c.text),
    Array.from(layoutText(s, measure).lines),
  );
  assert.equal(calls[1].y - calls[0].y, 25);
  assert.equal(calls[0].x, s.x0 + 2);
});

// ── Liqlip additions ─────────────────────────────────────────────────────────

test("layout is cached by (text, width, size): unchanged text is never re-measured (П-20)", () => {
  let n = 0;
  const counting = (text, size) => {
    n++;
    return (Array.from(text).length * size) / 2;
  };
  const s = { type: "text", text: "cache me please", fontSize: 16, font: "Arial", x0: 0, y0: 0, x1: 0, y1: 0 };
  layoutText(s, counting);
  const first = n;
  layoutText({ ...s, x0: 40 }, counting); // moving doesn't change the layout
  assert.equal(n, first);
  layoutText({ ...s, text: "changed" }, counting);
  assert.ok(n > first);
});

test("v1 adapter: font = fontSize × unit, the box at `at`, back again after a resize", () => {
  const o = { id: "t", color: "#ff0000", kind: "text", at: [10, 20], width: null, text: "Hi", fontSize: 24, style: "plain" };
  const s = T.textShape(o, 2, measure);
  assert.equal(s.fontSize, 48);
  assert.equal(s.x0, 10);
  assert.equal(s.y0, 20);
  const back = T.fromShape(o, resizeText(s, 3, 50, 0, measure, 2), 2);
  assert.equal(back.fontSize, 24, "a side handle keeps the font");
  assert.ok(back.width > 0, "and fixes the width (wrapping on)");
});

test("badge text is black or white by contrast to its plate", () => {
  assert.equal(T.contrastInk("#ffcc00"), "#000000");
  assert.equal(T.contrastInk("#ffffff"), "#000000");
  assert.equal(T.contrastInk("#007aff"), "#ffffff");
  assert.equal(T.contrastInk("#000000"), "#ffffff");
});
