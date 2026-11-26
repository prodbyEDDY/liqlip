// "Nothing can be dropped here" (W7-R4, src/lib/dropGuard.ts): dragenter / dragover are cancelled
// with dropEffect "none" (a refused drop, not a "successful" one), drop is cancelled (no navigation).
import assert from "node:assert/strict";
import test from "node:test";
import { importTs } from "./helpers/bundle.mjs";

const { refuseDrop } = await importTs("src/lib/dropGuard.ts");

const ev = (type, dt = { dropEffect: "copy" }) => {
  const e = { type, dataTransfer: dt, prevented: false, preventDefault: () => (e.prevented = true) };
  return e;
};

test("dragenter / dragover: cancelled with dropEffect none — the source sees a refused drop", () => {
  for (const type of ["dragenter", "dragover"]) {
    const e = ev(type);
    refuseDrop(e);
    assert.equal(e.prevented, true, type);
    assert.equal(e.dataTransfer.dropEffect, "none", type);
  }
});

test("drop: cancelled (the browser never navigates to a dropped file); no dataTransfer is fine", () => {
  const e = ev("drop");
  refuseDrop(e);
  assert.equal(e.prevented, true);
  const bare = ev("dragover", null);
  refuseDrop(bare);
  assert.equal(bare.prevented, true);
});
