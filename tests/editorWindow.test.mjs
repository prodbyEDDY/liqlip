// The editor window's load ordering (W7-R3): the newest load wins; a `shot:editor` that arrives
// while `shot_editor_get` is in flight makes its answer stale.
import assert from "node:assert/strict";
import test from "node:test";
import { importTs } from "./helpers/bundle.mjs";

const { LoadOrder } = await importTs("src/windows/editor/loadOrder.ts");

test("two loads in flight: only the newest may install its picture", () => {
  const o = new LoadOrder();
  const a = o.begin(); // the first shot_editor_get → record A decoding
  const b = o.begin(); // shot:editor → record B
  assert.equal(o.current(a), false, "A finishing late is dropped (its pixels never land under B's id)");
  assert.equal(o.current(b), true);
});

test("a shot:editor during shot_editor_get makes the answer stale; one after it doesn't", () => {
  const o = new LoadOrder();
  let m = o.mark();
  o.switched();
  assert.equal(o.superseded(m), true, "event first: the get answer is older");
  m = o.mark();
  assert.equal(o.superseded(m), false, "no event meanwhile: load the answer");
});
