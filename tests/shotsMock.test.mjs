// The browser mock of the screenshot commands (W5-A, ARCHITECTURE §18.10 "Мок"): the session and
// its fake desktop, the framed commit, the feed record and the events, the editor and the rest.
// Node has no DOM, so pictures are SVG data URLs here (canvas → blob: URL in a browser).
import assert from "node:assert/strict";
import test from "node:test";
import { importTs } from "./helpers/bundle.mjs";

const { mockInvoke, mockListen, frameBody, fakeDesktop, clampRect, currentSettings, afterCommit, shotDoc } = await importTs(
  "src/lib/ipc/mock/index.ts",
  "src/lib/ipc/mock/bus.ts",
  "src/lib/ipc/frame.ts",
  "src/lib/ipc/mock/shots.ts",
  "src/lib/ipc/mock/settings.ts",
);

const tick = (ms = 5) => new Promise((r) => setTimeout(r, ms));
const q = (over = {}) => ({ text: "", exclude: [], filter: {}, cursor: null, limit: 200, ...over });

function record(events, name) {
  const off = mockListen(name, (p) => events.push({ name, p }));
  return off;
}

const inside = (c, p) => c.x >= p.x && c.y >= p.y && c.x + c.w <= p.x + p.w && c.y + c.h <= p.y + p.h;

test("the fake desktop: 1920×1080 × scale, windows inside the frame, one with two children", () => {
  for (const scale of [1, 1.5]) {
    const d = fakeDesktop(scale);
    assert.deepEqual([d.width, d.height], [1920 * scale, 1080 * scale]);
    assert.ok(d.windows.length >= 4 && d.windows.length <= 6);
    for (const w of d.windows) {
      assert.ok(inside(w, { x: 0, y: 0, w: d.width, h: d.height }), "window inside the frame");
      for (const c of w.children) assert.ok(inside(c, w), "child inside its window");
    }
    assert.equal(d.windows.filter((w) => w.children.length === 2).length, 1);
  }
});

test("clampRect rounds and clamps to the frame, refuses an empty region", () => {
  assert.deepEqual(clampRect({ x: -10.4, y: 5.6, w: 30, h: 2000 }, 100, 50), { x: 0, y: 6, w: 20, h: 44 });
  assert.throws(() => clampRect({ x: 200, y: 0, w: 10, h: 10 }, 100, 50), /^invalid: /);
  assert.throws(() => clampRect({ x: 0, y: 0, w: Number.NaN, h: 1 }, 100, 50), /^invalid: /);
});

test("shot_session opens a session with the frame and the settings; shot_stage checks the id", async () => {
  const events = [];
  const off = record(events, "shot:session");
  const s = await mockInvoke("shot_session", {});
  await tick();
  off();
  assert.equal(s.mode, "region");
  assert.deepEqual([s.monitor.width, s.monitor.height, s.monitor.scale], [1920, 1080, 1]);
  assert.ok(s.monitor.frameUrl.startsWith("data:image/svg+xml"));
  assert.deepEqual(s.windows, fakeDesktop(1).windows);
  assert.equal(s.magnifier, currentSettings().screenshot.magnifier);
  assert.ok(events.some((e) => e.p.id === s.id), "shot:session emitted");
  assert.equal((await mockInvoke("shot_session", {})).id, s.id, "the same session until it ends");
  await mockInvoke("shot_stage", { sessionId: s.id, stage: "frame" });
  await assert.rejects(mockInvoke("shot_stage", { sessionId: s.id + 1, stage: "frame" }), /^not_found: /);
  await assert.rejects(mockInvoke("shot_stage", { sessionId: s.id, stage: "zoom" }), /^invalid: /);
  await mockInvoke("shot_cancel", { sessionId: s.id });
});

test("shot_commit: a framed body becomes a screenshot record + clips:changed, shot:added, shot:end", async () => {
  const s = await mockInvoke("shot_session", {});
  const before = await mockInvoke("clips_counts", {});
  const events = [];
  const offs = ["clips:changed", "shot:added", "shot:end"].map((n) => record(events, n));
  const doc = { v: 1, w: 2, h: 1, unit: 1, crop: null, objects: [{ id: "a", kind: "line", color: "#ff0000", a: [0, 0], b: [1, 0], size: 2 }] };
  const rgba = new Uint8ClampedArray([255, 0, 0, 255, 0, 0, 255, 255]);
  const res = await mockInvoke("shot_commit", frameBody({ sessionId: s.id, rect: { x: 100, y: 100, w: 2, h: 1 }, action: "done", doc, width: 2, height: 1 }, rgba));
  await tick();
  offs.forEach((f) => f());
  assert.equal(typeof res.clipId, "number");
  assert.equal(res.text, null);
  assert.match(res.savedPath, /\\Pictures\\Liqlip\\Liqlip \d{4}-\d{2}-\d{2} \d{6}\.png$/, "saved by default ({time} = HHmmss, as files.rs)");
  const after = await mockInvoke("clips_counts", {});
  assert.equal(after.kinds.screenshot, before.kinds.screenshot + 1);
  const top = (await mockInvoke("clips_list", { q: q({ limit: 1 }) })).items[0];
  assert.equal(top.id, res.clipId);
  assert.equal(top.kind, "screenshot");
  assert.deepEqual([top.width, top.height], [2, 1]);
  assert.deepEqual(top.meta.shot, { savedPath: res.savedPath, edited: true });
  const added = events.find((e) => e.name === "shot:added").p;
  assert.equal(added.clip.id, res.clipId);
  assert.equal(added.error, null);
  assert.ok(events.some((e) => e.name === "clips:changed" && e.p.change === "added" && e.p.ids.includes(res.clipId)));
  assert.ok(events.some((e) => e.name === "shot:end" && e.p.sessionId === s.id));
  // The session is over: a late commit is refused; a new shot_session opens another one.
  await assert.rejects(mockInvoke("shot_commit", frameBody({ sessionId: s.id, rect: { x: 0, y: 0, w: 1, h: 1 }, action: "copy", doc: null, width: 0, height: 0 })), /^not_found: /);
  assert.notEqual((await mockInvoke("shot_session", {})).id, s.id);
});

test("shot_commit without pixels crops the frame; text → OCR text, no record; bad bodies", async () => {
  let s = await mockInvoke("shot_session", {});
  const res = await mockInvoke("shot_commit", frameBody({ sessionId: s.id, rect: { x: 1900, y: 1000, w: 100, h: 100 }, action: "copy", doc: null, width: 0, height: 0 }));
  const clip = (await mockInvoke("clips_list", { q: q({ limit: 1 }) })).items[0];
  assert.equal(clip.id, res.clipId);
  assert.deepEqual([clip.width, clip.height], [20, 80], "clamped to the frame");
  assert.equal(res.savedPath, null, "copy does not save");
  assert.equal(clip.meta.shot.edited, false);

  s = await mockInvoke("shot_session", {});
  const count = (await mockInvoke("clips_counts", {})).all;
  const text = await mockInvoke("shot_commit", frameBody({ sessionId: s.id, rect: { x: 0, y: 0, w: 50, h: 20 }, action: "text", doc: null, width: 0, height: 0 }));
  assert.equal(text.clipId, null);
  assert.equal(typeof text.text, "string");
  assert.equal((await mockInvoke("clips_counts", {})).all, count, "no record");

  s = await mockInvoke("shot_session", {});
  await assert.rejects(mockInvoke("shot_commit", { sessionId: s.id }), /^invalid: /, "named args instead of a body");
  await assert.rejects(mockInvoke("shot_commit", frameBody({ sessionId: s.id, rect: { x: 0, y: 0, w: 2, h: 1 }, action: "done", doc: null, width: 2, height: 1 }, new Uint8Array(4))), /^invalid: /);
  await assert.rejects(mockInvoke("shot_commit", frameBody({ sessionId: s.id, rect: { x: 0, y: 0, w: 2, h: 1 }, action: "boom", doc: null, width: 0, height: 0 })), /^invalid: /);
  await mockInvoke("shot_cancel", { sessionId: s.id });
});

test("what follows a commit is Rust's plan (W7-R4): only Done shows the QAO / editor", async () => {
  // commit::plan + step (6): done → the setting; copy/save/save_as → nothing; pin → the pin
  for (const setting of ["overlay", "editor", "none"]) {
    for (const a of ["copy", "save", "save_as"]) assert.equal(afterCommit(a, setting, true), "nothing", `${a} / ${setting}`);
    assert.equal(afterCommit("pin", setting, true), "pin");
    assert.equal(afterCommit("pin", setting, false), "qao", "no record to pin — the QAO card (W7-R1 / W8-A)");
  }
  assert.equal(afterCommit("done", "overlay", true), "qao");
  assert.equal(afterCommit("done", "editor", true), "editor");
  assert.equal(afterCommit("done", "editor", false), "qao", "no record to edit — the QAO instead");
  assert.equal(afterCommit("done", "none", true), "nothing");
  // end to end: Ctrl+C in the overlay copies and closes — no thumbnail
  const events = [];
  const off = record(events, "shot:added");
  const s = await mockInvoke("shot_session", {});
  await mockInvoke("shot_commit", frameBody({ sessionId: s.id, rect: { x: 0, y: 0, w: 10, h: 10 }, action: "copy", doc: null, width: 0, height: 0 }));
  await tick();
  off();
  assert.equal(events.length, 0);
});

test("every shot record gets a document with the monitor scale as its unit (W8-A, as Rust)", async () => {
  // the rule (commit.rs::sized_doc + store::shots::empty_doc)
  assert.deepEqual(shotDoc(null, 30, 20, 1.5, true), { v: 1, w: 30, h: 20, unit: 1.5, crop: null, objects: [] });
  const sent = { v: 1, w: 31, h: 21, unit: 2, crop: null, objects: [] };
  assert.deepEqual(shotDoc(sent, 30, 20, 1, true), { ...sent, w: 30, h: 20 }, "the crop's size, the sent unit");
  assert.deepEqual(shotDoc(sent, 31, 21, 1, false), sent, "flattened pixels: as sent");
  // end to end: the overlay sends the empty document; the editor opens with its unit
  const s = await mockInvoke("shot_session", {});
  const res = await mockInvoke("shot_commit", frameBody({ sessionId: s.id, rect: { x: 10, y: 10, w: 40, h: 30 }, action: "copy", doc: { ...sent, w: 40, h: 30 }, width: 0, height: 0 }));
  await mockInvoke("shot_edit", { id: res.clipId });
  const p = await mockInvoke("shot_editor_get", {});
  assert.equal(p.clipId, res.clipId);
  assert.deepEqual([p.doc.unit, p.doc.w, p.doc.h, p.doc.objects.length], [2, 40, 30, 0]);
  // a capture without overlay: the empty document with the monitor's scale (1 in node)
  await mockInvoke("shot_capture", { mode: "screen", delay: null });
  const top = (await mockInvoke("clips_list", { q: q({ limit: 1 }) })).items[0];
  await mockInvoke("shot_edit", { id: top.id });
  assert.deepEqual((await mockInvoke("shot_editor_get", {})).doc, { v: 1, w: 1920, h: 1080, unit: 1, crop: null, objects: [] });
});

test("shot_editor_commit: a cancelled Save as… answers null (W8-A)", async () => {
  const shot = (await mockInvoke("clips_list", { q: q({ filter: { kinds: ["screenshot"] }, limit: 1 }) })).items[0];
  const doc = { v: 1, w: 1, h: 1, unit: 1, crop: null, objects: [] };
  const body = () => frameBody({ clipId: shot.id, action: "save_as", doc, width: 0, height: 0 });
  globalThis.location = { search: "?mock-shot-dialog=cancel" };
  try {
    assert.equal(await mockInvoke("shot_editor_commit", body()), null);
  } finally {
    delete globalThis.location;
  }
  const saved = await mockInvoke("shot_editor_commit", body());
  assert.equal(saved.id, shot.id);
  assert.match(saved.meta.shot.savedPath, /\\Documents\\/);
});

test("shot_capture: screen commits at once; a trigger during a session is ignored; bad modes", async () => {
  const before = (await mockInvoke("clips_counts", {})).kinds.screenshot;
  await mockInvoke("shot_capture", { mode: "screen", delay: null });
  const top = (await mockInvoke("clips_list", { q: q({ limit: 1 }) })).items[0];
  assert.equal(top.kind, "screenshot");
  assert.deepEqual([top.width, top.height], [1920, 1080]);
  assert.equal((await mockInvoke("clips_counts", {})).kinds.screenshot, before + 1);
  const s = await mockInvoke("shot_session", {});
  await mockInvoke("shot_capture", { mode: "screen" });
  assert.equal((await mockInvoke("clips_counts", {})).kinds.screenshot, before + 1, "ignored while the overlay is open");
  await mockInvoke("shot_cancel", { sessionId: s.id });
  await assert.rejects(mockInvoke("shot_capture", { mode: "video" }), /^invalid: /);
  await assert.rejects(mockInvoke("shot_capture", { mode: "screen", delay: -1 }), /^invalid: /);
});

test("screenshots off in storage.types: no record, the QAO gets a temp file", async () => {
  const prev = currentSettings();
  await mockInvoke("settings_set", { settings: { ...prev, storage: { ...prev.storage, types: { ...prev.storage.types, screenshot: false } } } });
  const events = [];
  const off = record(events, "shot:added");
  const count = (await mockInvoke("clips_counts", {})).all;
  await mockInvoke("shot_capture", { mode: "screen" });
  await tick();
  off();
  assert.equal((await mockInvoke("clips_counts", {})).all, count);
  const p = events.at(-1).p;
  assert.equal(p.clip, null);
  assert.match(p.tempPath, /\\Temp\\Liqlip\\shots\\/);
  await mockInvoke("settings_set", { settings: prev });
});

test("editor: payload by record, Done stores the document and the flattened picture", async () => {
  const shot = (await mockInvoke("clips_list", { q: q({ filter: { kinds: ["screenshot"] }, limit: 1 }) })).items[0];
  await mockInvoke("shot_edit", { id: shot.id });
  const p = await mockInvoke("shot_editor_get", {});
  assert.equal(p.clipId, shot.id);
  assert.equal(p.width, shot.width);
  assert.ok(p.baseUrl);
  const doc = { v: 1, w: 3, h: 1, unit: 1, crop: null, objects: [] };
  const events = [];
  const off = record(events, "clips:changed");
  const clip = await mockInvoke("shot_editor_commit", frameBody({ clipId: shot.id, action: "done", doc, width: 3, height: 1 }, new Uint8Array(12)));
  await tick();
  off();
  assert.deepEqual([clip.width, clip.height], [3, 1]);
  assert.equal(clip.meta.shot.edited, false, "no objects → not marked as annotated");
  assert.deepEqual((await mockInvoke("shot_editor_get", {})).doc, doc);
  assert.ok(events.some((e) => e.p.change === "updated" && e.p.ids.includes(shot.id)));
  const text = (await mockInvoke("clips_list", { q: q({ filter: { kinds: ["text"] }, limit: 1 }) })).items[0];
  await assert.rejects(mockInvoke("shot_edit", { id: text.id }), /^invalid: /);
  await assert.rejects(mockInvoke("shot_editor_commit", frameBody({ clipId: shot.id, action: "explode", doc, width: 0, height: 0 })), /^invalid: /);
});

test("pins, files, QAO layout, status, Windows settings", async () => {
  const shot = (await mockInvoke("clips_list", { q: q({ filter: { kinds: ["screenshot"] }, limit: 1 }) })).items[0];
  await mockInvoke("shot_pin", { id: shot.id });
  const pin = await mockInvoke("shot_pin_get", {});
  assert.equal(typeof pin.clipId, "number");
  await mockInvoke("shot_pin_resize", { scale: 1.25 });
  await assert.rejects(mockInvoke("shot_pin_resize", { scale: 0 }), /^invalid: /);
  const path = await mockInvoke("shot_save", { id: shot.id });
  assert.match(path, /\.png$/);
  await mockInvoke("shot_reveal", { id: shot.id });
  assert.match(await mockInvoke("shot_save_as", { id: shot.id }), /\.png$/);
  await mockInvoke("shots_hud_layout", { height: 340 });
  await assert.rejects(mockInvoke("shots_hud_layout", { height: -1 }), /^invalid: /);
  await mockInvoke("shots_hud_hide", {});
  assert.deepEqual(await mockInvoke("shot_status", {}), { snippingKey: false, hotkeyErrors: [] });
  await mockInvoke("open_windows_settings", { page: "keyboard" });
  await assert.rejects(mockInvoke("open_windows_settings", { page: "display" }), /^invalid: /);
});
