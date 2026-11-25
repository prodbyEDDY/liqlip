// The browser mock behaves like the contract says (coordinator, W0): shapes, filters,
// cursor paging, mutations + events with origin 'user', undo, settings validation.
import assert from "node:assert/strict";
import test from "node:test";
import { importTs } from "./helpers/bundle.mjs";

const { mockInvoke, mockListen } = await importTs("src/lib/ipc/mock/index.ts", "src/lib/ipc/mock/bus.ts");

const q = (over = {}) => ({ text: "", exclude: [], filter: {}, cursor: null, limit: 40, ...over });
const tick = () => new Promise((r) => setTimeout(r, 5));

test("seed covers all seven kinds and pages with an opaque cursor", async () => {
  const counts = await mockInvoke("clips_counts", {});
  assert.equal(counts.all, 300);
  for (const k of ["text", "link", "code", "color", "contact", "image", "file"]) {
    assert.ok(counts.kinds[k] > 0, `kind ${k} seeded`);
  }
  const seen = new Set();
  let cursor = null;
  let pages = 0;
  do {
    const page = await mockInvoke("clips_list", { q: q({ cursor }) });
    for (const c of page.items) {
      assert.ok(!seen.has(c.id), "no duplicates across pages");
      seen.add(c.id);
    }
    cursor = page.nextCursor;
    pages++;
  } while (cursor);
  assert.equal(seen.size, 300);
  assert.equal(pages, Math.ceil(300 / 40));
});

test("feed is chronological and filters work", async () => {
  const { items } = await mockInvoke("clips_list", { q: q({ limit: 200 }) });
  for (let i = 1; i < items.length; i++) assert.ok(items[i - 1].copiedAt >= items[i].copiedAt);
  const links = await mockInvoke("clips_list", { q: q({ filter: { kinds: ["link"] }, limit: 200 }) });
  assert.ok(links.items.length > 0 && links.items.every((c) => c.kind === "link" && c.meta.host !== undefined));
  const images = await mockInvoke("clips_list", { q: q({ filter: { kinds: ["image"] } }) });
  assert.ok(images.items.every((c) => c.thumbUrl && c.width && c.formats.includes("png")));
  const hit = await mockInvoke("clips_list", { q: q({ text: "github" }) });
  assert.ok(hit.items.length > 0);
  const none = await mockInvoke("clips_list", { q: q({ text: "github", exclude: ["github"] }) });
  assert.equal(none.items.length, 0);
});

test("mutations emit clips:changed with origin 'user'; delete is undoable", async () => {
  const events = [];
  const off = mockListen("clips:changed", (p) => events.push(p));
  const { items } = await mockInvoke("clips_list", { q: q({ limit: 3 }) });
  const ids = items.map((c) => c.id);

  await mockInvoke("clip_set_favorite", { ids: [ids[0]], value: true });
  const favs = await mockInvoke("clips_list", { q: q({ filter: { favorites: true }, limit: 200 }) });
  assert.ok(favs.items.some((c) => c.id === ids[0]));

  const before = (await mockInvoke("clips_counts", {})).all;
  const { undoToken } = await mockInvoke("clips_delete", { ids });
  assert.equal((await mockInvoke("clips_counts", {})).all, before - 3);
  await assert.rejects(mockInvoke("clip_detail", { id: ids[1] }), /^not_found: /);
  assert.equal(await mockInvoke("clips_undo", { token: undoToken }), true);
  assert.equal(await mockInvoke("clips_undo", { token: undoToken }), false, "a token works once");
  assert.equal((await mockInvoke("clips_counts", {})).all, before);

  await tick();
  off();
  assert.ok(events.length >= 3);
  assert.ok(events.every((e) => e.origin === "user"));
  assert.deepEqual(events.find((e) => e.change === "deleted").ids, ids);
});

test("folders: create, duplicate name, move clips, delete with undo", async () => {
  const f = await mockInvoke("folder_create", { name: "Inbox" });
  assert.equal(f.count, 0);
  await assert.rejects(mockInvoke("folder_create", { name: "inbox" }), /^invalid: exists/);
  const { items } = await mockInvoke("clips_list", { q: q({ limit: 2 }) });
  await mockInvoke("clip_set_folder", { ids: items.map((c) => c.id), folderId: f.id });
  const counts = await mockInvoke("clips_counts", {});
  assert.equal(counts.folders[f.id], 2);
  const { undoToken } = await mockInvoke("folder_delete", { id: f.id, withClips: false });
  assert.ok(!(await mockInvoke("folders_list", {})).some((x) => x.id === f.id));
  assert.equal(await mockInvoke("clips_undo", { token: undoToken }), true);
  assert.ok((await mockInvoke("folders_list", {})).some((x) => x.id === f.id));
});

test("paste outcome follows the mock target; stack works", async () => {
  const { items } = await mockInvoke("clips_list", { q: q({ limit: 2 }) });
  const outcome = await mockInvoke("clip_paste", { req: { ids: [items[0].id], mode: "auto" } });
  assert.deepEqual(outcome, { result: "pasted" });
  const missing = await mockInvoke("clip_paste", { req: { ids: [999999], mode: { format: "html" } } });
  assert.deepEqual(missing, { result: "failed", reason: "not_found" });
  const stack = await mockInvoke("stack_add", { ids: items.map((c) => c.id) });
  assert.equal(stack.items.length, 2);
  const reversed = await mockInvoke("stack_reverse", {});
  assert.equal(reversed.items[0].clipId, items[1].id);
  await mockInvoke("stack_paste_next", {});
  assert.equal((await mockInvoke("stack_get", {})).items.length, 1);
  assert.equal((await mockInvoke("stack_clear", {})).items.length, 0);
});

test("settings_set validates, reports taken hotkeys and keeps the rest", async () => {
  const s = await mockInvoke("settings_get", {});
  s.picker.centerRow = true;
  s.hotkeys.summon = "Alt + Tab"; // simulated "taken"
  s.storage.maxEntries = -1;
  const events = [];
  const off = mockListen("settings:changed", (p) => events.push(p));
  const res = await mockInvoke("settings_set", { settings: s });
  assert.equal(res.settings.picker.centerRow, true);
  assert.equal(res.settings.hotkeys.summon, "Alt + V");
  assert.equal(res.settings.storage.maxEntries, 5000);
  assert.deepEqual(
    res.errors.map((e) => `${e.field}:${e.code}`).sort(),
    ["hotkeys.summon:taken", "storage.maxEntries:invalid"],
  );
  await tick();
  off();
  assert.equal(events.length, 1);
  assert.equal(events[0].picker.centerRow, true);
});

test("unknown command rejects as unimplemented", async () => {
  await assert.rejects(mockInvoke("nope", {}), /^unimplemented: nope$/);
});
