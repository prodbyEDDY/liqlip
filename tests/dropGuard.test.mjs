// "Nothing can be dropped here" (W7-R4, src/lib/dropGuard.ts): dragenter / dragover are cancelled
// with dropEffect "none" (a refused drop, not a "successful" one), drop is cancelled (no navigation).
import assert from "node:assert/strict";
import test from "node:test";
import { importTs } from "./helpers/bundle.mjs";

const { refuseDrop, refuseFileDrop } = await importTs("src/lib/dropGuard.ts");

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

// W8-A: the settings window refuses only a FILE nobody on the page took — the folder / snippet
// reorder (its rows accept `dragover` themselves, `text/plain`) and text dropped into a field stay.
const drag = (type, types, prevented = false) => {
  const e = {
    type,
    defaultPrevented: prevented,
    dataTransfer: { dropEffect: "copy", types },
    prevented: false,
    preventDefault: () => (e.prevented = true),
  };
  return e;
};

test("settings: a file from Explorer is refused (dragenter / dragover: dropEffect none; drop: cancelled)", () => {
  for (const type of ["dragenter", "dragover", "drop"]) {
    const e = drag(type, ["Files"]);
    refuseFileDrop(e);
    assert.equal(e.prevented, true, type);
    assert.equal(e.dataTransfer.dropEffect, type === "drop" ? "copy" : "none", type);
  }
});

test("settings: the reorder drag and text drags are left alone", () => {
  const row = drag("dragover", ["text/plain"], true); // a folder row took it (reorder.over)
  row.dataTransfer.dropEffect = "move";
  refuseFileDrop(row);
  assert.deepEqual([row.prevented, row.dataTransfer.dropEffect], [false, "move"]);
  const gap = drag("dragover", ["text/plain"]); // the same drag between rows: no file, not touched
  refuseFileDrop(gap);
  assert.equal(gap.prevented, false);
  const text = drag("drop", ["text/plain", "text/html"]); // text into a field
  refuseFileDrop(text);
  assert.equal(text.prevented, false);
  const taken = drag("drop", ["Files"], true); // something on the page handled the file itself
  refuseFileDrop(taken);
  assert.equal(taken.prevented, false);
  const bare = { type: "dragover", defaultPrevented: false, dataTransfer: null, preventDefault: () => assert.fail() };
  refuseFileDrop(bare);
});
