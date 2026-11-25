// {clipboard} in snippets (W3, FIX-09; coordinator's semantics): `clipboard_text` at paste
// time — a string is the answer (even ""), null (no text: image, files, empty) → "", an error →
// the history (newest record, unless the shelf wrote the clipboard after it); an unreadable
// history rejects (the shelf then offers "Paste anyway"). Plus the mock's own clipboard.
import assert from "node:assert/strict";
import test from "node:test";
import { importTs } from "./helpers/bundle.mjs";

const { resolveClipboardText, historyClipboardText, noteWrite, resetWrites } = await importTs("src/windows/picker/overlays/clipboardSource.ts");
const { mockInvoke } = await importTs("src/lib/ipc/mock/index.ts");

const rec = (id, kind, copiedAt, text) => ({ id, kind, copiedAt, preview: text ?? "", text: text ?? null });

/** An IO double: `live` answer, history records, and a log of what was asked. */
function io({ live = null, items = [], listFails = false } = {}) {
  const calls = { live: 0, list: 0, detail: [] };
  return {
    calls,
    live: async () => {
      calls.live++;
      if (live instanceof Error) throw live;
      return live;
    },
    list: async () => {
      calls.list++;
      if (listFails) throw "busy: database is locked";
      return { items: items.map(({ text: _t, ...c }) => c), nextCursor: null };
    },
    detail: async (id) => {
      calls.detail.push(id);
      const r = items.find((x) => x.id === id);
      if (!r) throw `not_found: clip ${id}`;
      return r;
    },
  };
}

test("the live clipboard wins — the history isn't even read", async () => {
  resetWrites();
  const x = io({ live: "secret from KeePass", items: [rec(1, "text", 100, "older history")] });
  assert.equal(await resolveClipboardText(x), "secret from KeePass");
  assert.equal(x.calls.list, 0);
});

test("an empty clipboard text is still the answer (not a fallback)", async () => {
  resetWrites();
  const x = io({ live: "", items: [rec(1, "text", 100, "history")] });
  assert.equal(await resolveClipboardText(x), "");
  assert.equal(x.calls.list, 0);
});

test("null (an image / files / nothing on the clipboard) → empty, never an older record", async () => {
  resetWrites();
  noteWrite({ text: "written by the shelf earlier" }, 50);
  const x = io({ live: null, items: [rec(2, "text", 300, "newest copy")] });
  assert.equal(await resolveClipboardText(x), "");
  assert.equal(x.calls.list, 0, "the history isn't consulted");
});

test("a failing clipboard_text (busy) → the newest history record", async () => {
  resetWrites();
  // "favorites first": the newest copy is not the first row.
  const x = io({ live: new Error("busy: the clipboard owner does not respond"), items: [rec(1, "text", 100, "pinned favourite"), rec(2, "text", 300, "newest copy"), rec(3, "code", 200, "x")] });
  assert.equal(await resolveClipboardText(x), "newest copy");
  assert.equal(x.calls.live, 1);
});

test("history: a later write by the shelf is what the clipboard holds", async () => {
  resetWrites();
  noteWrite({ text: "transform result" }, 500);
  assert.equal(await historyClipboardText(io({ items: [rec(1, "text", 400, "captured earlier")] })), "transform result");
  // A capture after the shelf's write wins again.
  assert.equal(await historyClipboardText(io({ items: [rec(1, "text", 600, "captured later")] })), "captured later");
  // Records pasted together, joined by their separator; images skipped.
  noteWrite({ ids: [1, 2, 3], separator: " | " }, 900);
  const many = io({ items: [rec(1, "text", 100, "a"), rec(2, "image", 100), rec(3, "code", 100, "c")] });
  assert.equal(await historyClipboardText(many), "a | c");
});

test("history: newest is an image / files / nothing → empty text", async () => {
  resetWrites();
  assert.equal(await historyClipboardText(io({ items: [rec(1, "image", 100)] })), "");
  assert.equal(await historyClipboardText(io({ items: [rec(1, "file", 100, "C:\\a.txt")] })), "");
  assert.equal(await historyClipboardText(io({ items: [] })), "");
});

test("neither clipboard nor history readable → rejects (the shelf asks 'Paste anyway')", async () => {
  resetWrites();
  await assert.rejects(resolveClipboardText(io({ live: new Error("busy: locked"), listFails: true })));
});

test("mock clipboard_text: null until something is written, then the last write", async () => {
  assert.equal(await mockInvoke("clipboard_text", {}), null);
  await mockInvoke("text_copy", { text: "copied from the shelf" });
  assert.equal(await mockInvoke("clipboard_text", {}), "copied from the shelf");
  await mockInvoke("text_paste", { req: { text: "pasted text", plain: true } });
  assert.equal(await mockInvoke("clipboard_text", {}), "pasted text");
});
