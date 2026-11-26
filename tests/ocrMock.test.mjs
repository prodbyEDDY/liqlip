// The browser mock of the OCR commands behaves like src-tauri/src/ocr (W2-A, ARCHITECTURE
// §5a): status shape, clip_ocr answers + stores + emits, reindex progress to done == total
// without doubling, OCR turned off → `invalid`.
import assert from "node:assert/strict";
import test from "node:test";
import { importTs } from "./helpers/bundle.mjs";

const { mockInvoke, mockListen } = await importTs("src/lib/ipc/mock/index.ts", "src/lib/ipc/mock/bus.ts");

const q = (over = {}) => ({ text: "", exclude: [], filter: {}, cursor: null, limit: 200, ...over });
const tick = (ms = 5) => new Promise((r) => setTimeout(r, ms));

async function images() {
  // Screenshots are pictures too (W5, §18.5): the OCR queue covers both kinds.
  return (await mockInvoke("clips_list", { q: q({ filter: { kinds: ["image", "screenshot"] } }) })).items;
}

async function setOcr(over) {
  const s = await mockInvoke("settings_get", {});
  s.ocr = { ...s.ocr, ...over };
  const res = await mockInvoke("settings_set", { settings: s });
  assert.deepEqual(res.errors, []);
}

test("ocr_status reports two languages and the queue length", async () => {
  const s = await mockInvoke("ocr_status", {});
  assert.equal(s.available, true);
  assert.deepEqual(s.languages, ["en-US", "ru-RU"]);
  assert.equal(typeof s.pending, "number");
});

test("clip_ocr returns text, stores it (done) and emits clips:changed; errors by code", async () => {
  const events = [];
  const off = mockListen("clips:changed", (p) => events.push(p));
  const img = (await images()).find((c) => c.ocr !== "done");
  assert.ok(img, "seed has an image that is not recognised yet");
  const text = await mockInvoke("clip_ocr", { id: img.id });
  assert.equal(typeof text, "string");
  const detail = await mockInvoke("clip_detail", { id: img.id });
  assert.equal(detail.ocr, "done");
  assert.equal(detail.ocrText, text);
  assert.equal(await mockInvoke("clip_ocr", { id: img.id }), text, "done → same text from the store");
  await tick();
  assert.ok(events.some((e) => e.origin === "user" && e.change === "updated" && e.ids?.includes(img.id)));
  off();

  const txt = (await mockInvoke("clips_list", { q: q({ filter: { kinds: ["text"] }, limit: 1 }) })).items[0];
  await assert.rejects(mockInvoke("clip_ocr", { id: txt.id }), /^invalid: /);
  await assert.rejects(mockInvoke("clip_ocr", { id: 987654 }), /^not_found: /);
});

test("search finds recognised text", async () => {
  const img = (await images()).find((c) => c.ocr === "done");
  const text = await mockInvoke("clip_ocr", { id: img.id });
  const word = text.split(/\s+/).find((w) => w.length >= 5);
  if (!word) return; // an image without text — nothing to search
  const hits = await mockInvoke("clips_list", { q: q({ text: word }) });
  assert.ok(hits.items.some((c) => c.id === img.id));
});

test("ocr_reindex: progress {0,total} … done == total, not doubled, every image done", async () => {
  const progress = [];
  const off = mockListen("ocr:progress", (p) => progress.push(p));
  const total = (await images()).length;
  await mockInvoke("ocr_reindex", {});
  await mockInvoke("ocr_reindex", {}); // second click during the run
  const deadline = Date.now() + 10_000;
  while (!progress.some((p) => p.done === p.total && p.total > 0)) {
    assert.ok(Date.now() < deadline, `no final progress: ${JSON.stringify(progress.slice(-3))}`);
    await tick(10);
  }
  off();
  assert.deepEqual(progress[0], { done: 0, total });
  assert.ok(progress.every((p) => p.total === total), "the second call did not restart or grow the run");
  for (let i = 1; i < progress.length; i++) assert.ok(progress[i - 1].done <= progress[i].done);
  assert.ok((await images()).every((c) => c.ocr === "done"));
  assert.equal((await mockInvoke("ocr_status", {})).pending, 0);
});

test("OCR turned off: reindex and recognition are refused, recognised text stays", async () => {
  const done = (await images()).find((c) => c.ocr === "done");
  const text = await mockInvoke("clip_ocr", { id: done.id });
  await setOcr({ enabled: false });
  await assert.rejects(mockInvoke("ocr_reindex", {}), /^invalid: /);
  assert.equal(await mockInvoke("clip_ocr", { id: done.id }), text, "already recognised text is still served");
  await setOcr({ enabled: true });
});
