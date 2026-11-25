// The browser mock behaves like the real store / paste flow where the shelf depends on it
// (W1-E): AND-ed search terms with phrases and the short-term rule (ARCHITECTURE §4.4),
// id-less `clips:changed` for folder and snippet edits, `partial` for mixed pastes, the
// layout per position.
import assert from "node:assert/strict";
import test from "node:test";
import { importTs } from "./helpers/bundle.mjs";

const { mockInvoke, mockListen, searchTermsOf, mockLayout, MOCK_DEFAULTS } = await importTs(
  "src/lib/ipc/mock/index.ts",
  "src/lib/ipc/mock/bus.ts",
  "src/lib/ipc/mock/clips.ts",
  "src/lib/ipc/mock/system.ts",
  "src/lib/ipc/mock/settings.ts",
);

const q = (over = {}) => ({ text: "", exclude: [], filter: {}, cursor: null, limit: 200, ...over });
const tick = () => new Promise((r) => setTimeout(r, 5));
const all = async (over) => (await mockInvoke("clips_list", { q: q(over) })).items;

test("terms: whitespace splits, quotes keep a phrase, repeats dropped", () => {
  assert.deepEqual(searchTermsOf('meeting "next week" Meeting'), ["meeting", "next week"]);
  assert.deepEqual(searchTermsOf('"unclosed phrase'), ["unclosed phrase"]);
  assert.deepEqual(searchTermsOf("   "), []);
});

test("search ANDs the terms; a phrase must match as a whole", async () => {
  const both = await all({ text: "SELECT clips" });
  assert.ok(both.length > 0);
  for (const c of both) {
    const hay = [c.title, c.preview].join(" ").toLowerCase();
    assert.ok(hay.includes("select") && hay.includes("clips"));
  }
  assert.equal((await all({ text: '"clips SELECT"' })).length, 0, "the phrase is not in that order");
  assert.equal((await all({ text: "SELECT zzzunlikely" })).length, 0);
});

test("short terms (< 3 chars) look at the preview and title only", async () => {
  const items = await all({ text: "ok" });
  assert.ok(items.length > 0);
  assert.ok(items.every((c) => [c.preview, c.title].join(" ").toLowerCase().includes("ok")));
});

test("exclusions follow the same rules", async () => {
  const withGit = await all({ text: "git" });
  const noRebase = await all({ text: "git", exclude: ["rebase"] });
  assert.ok(noRebase.length < withGit.length);
  assert.ok(noRebase.every((c) => !c.preview.toLowerCase().includes("rebase")));
});

test("folder and snippet edits emit clips:changed without ids", async () => {
  const events = [];
  const off = mockListen("clips:changed", (p) => events.push(p));
  const f = await mockInvoke("folder_create", { name: "Mock events" });
  await mockInvoke("folder_rename", { id: f.id, name: "Mock events 2" });
  await mockInvoke("snippet_save", { input: { name: "x", body: "y" } });
  await tick();
  off();
  assert.ok(events.length >= 3);
  assert.ok(events.every((e) => e.origin === "user" && e.change === "updated" && e.ids === undefined));
});

test("a joined text paste that skips images reports partial", async () => {
  const image = (await all({ filter: { kinds: ["image"] } }))[0];
  const text = (await all({ filter: { kinds: ["text"] } }))[0];
  const mixed = await mockInvoke("clip_paste", { req: { ids: [text.id, image.id], mode: "auto", separator: "\n" } });
  assert.deepEqual(mixed, { result: "pasted", reason: "partial" });
  const plain = await mockInvoke("clip_paste", { req: { ids: [text.id], mode: "auto" } });
  assert.deepEqual(plain, { result: "pasted" });
});

test("layout per position (SPEC §3.1 sizes + BLEED 72)", () => {
  const at = (position, centerRow = false) =>
    mockLayout({ ...MOCK_DEFAULTS, picker: { ...MOCK_DEFAULTS.picker, position, centerRow } });
  assert.deepEqual(at("center"), { position: "center", centerRow: false, width: 760, height: 640, bleed: 72 });
  assert.deepEqual(at("center", true), { position: "center", centerRow: true, width: 760, height: 360, bleed: 72 });
  assert.equal(at("left").height, 826);
  assert.equal(at("bottom").width, 800);
});
