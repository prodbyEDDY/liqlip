// QAO stack model (W6-E, src/windows/shots/stack.ts): order, eviction, removal, timers, geometry.
import assert from "node:assert/strict";
import test from "node:test";
import { importTs } from "./helpers/bundle.mjs";

const s = await importTs("src/windows/shots/stack.ts");

const clip = (id, over = {}) => ({ id, kind: "screenshot", thumbUrl: `http://liq.localhost/thumb/${id}?v=1`, width: 800, height: 600, meta: {}, ...over });
const added = (id, over = {}) => ({ clip: clip(id), savedPath: null, tempPath: null, monitor: "\\\\.\\DISPLAY1", error: null, ...over });
const card = (id, timeout = 10, over = {}) => s.makeShot(added(id, over), timeout);
const keys = (items) => items.map((i) => i.key);

test("new cards go to the bottom; a fifth pushes the oldest out", () => {
  let items = [];
  for (let n = 1; n <= 4; n++) items = s.addShot(items, card(n));
  assert.deepEqual(keys(items), ["clip:1", "clip:2", "clip:3", "clip:4"]);
  items = s.addShot(items, card(5));
  assert.equal(items.length, s.MAX_SHOTS);
  assert.deepEqual(keys(items), ["clip:2", "clip:3", "clip:4", "clip:5"]);
});

test("the same record again moves to the bottom as a fresh card (no duplicate)", () => {
  let items = [card(1), card(2), card(3)];
  items[0] = { ...items[0], remaining: 1, toast: { key: "shots.toast.copied" } };
  items = s.addShot(items, card(1));
  assert.deepEqual(keys(items), ["clip:2", "clip:3", "clip:1"]);
  assert.equal(items[2].remaining, 10);
  assert.equal(items[2].toast, null);
});

test("removeShot removes only that card and keeps the order", () => {
  const items = [card(1), card(2), card(3)];
  assert.deepEqual(keys(s.removeShot(items, "clip:2")), ["clip:1", "clip:3"]);
  assert.deepEqual(keys(s.removeShot(items, "clip:9")), keys(items));
});

test("keys and cards: a record, a temporary file (clip: null), neither", () => {
  assert.equal(s.shotKey(added(7)), "clip:7");
  assert.equal(s.shotKey(added(0, { clip: null, tempPath: "C:\\t\\1.png" })), "tmp:C:\\t\\1.png");
  assert.equal(s.shotKey(added(0, { clip: null })), null);
  assert.equal(s.makeShot(added(0, { clip: null }), 10), null);
  // INT-5: a card without a record previews its temp file by bare name (`liq://temp/<name>`)
  assert.equal(s.tempName("C:\\Users\\me\\AppData\\Local\\Temp\\Liqlip\\shots\\shot-9-1.png"), "shot-9-1.png");
  assert.equal(s.makeShot(added(0, { clip: null, tempPath: "C:\\t\\shot 1.png" }), 10).preview, "http://liq.localhost/temp/shot%201.png");
  const c = s.makeShot(added(3, { savedPath: "C:\\a.png", error: "save_failed" }), 5);
  assert.deepEqual(
    { preview: c.preview, savedPath: c.savedPath, error: c.error, timeout: c.timeout, remaining: c.remaining },
    { preview: "http://liq.localhost/thumb/3?v=1", savedPath: "C:\\a.png", error: "save_failed", timeout: 5, remaining: 5 },
  );
  assert.equal(s.makeShot(added(4, { clip: clip(4, { meta: { shot: { savedPath: "C:\\b.png", edited: false } } }) }), 10).savedPath, "C:\\b.png");
  assert.equal(s.makeShot(added(5), -3).timeout, 0, "a bad timeout means never");
});

test("timers: count down, expire, pause; 0 never hides; a failed save waits", () => {
  const items = [card(1, 2), card(2, 0), card(3, 2, { error: "save_failed" })];
  assert.equal(s.tickShots(items, 1, true), items, "paused: untouched");
  let next = s.tickShots(items, 1.5, false);
  assert.deepEqual(next.map((i) => i.remaining), [0.5, 0, 2]);
  next = s.tickShots(next, 0.6, false);
  assert.deepEqual(keys(next), ["clip:2", "clip:3"], "expired card left; never-hide and failed-save stay");
  assert.equal(s.tickShots(next, 100, false), next, "nothing counts → same array");
});

test("window geometry: width fixed, height from the card count, 0 = hide", () => {
  assert.equal(s.WINDOW_W, s.CARD_W + 2 * s.BLEED);
  assert.equal(s.stackHeight(0), 0);
  assert.equal(s.stackHeight(1), 2 * s.BLEED + s.CARD_H);
  assert.equal(s.stackHeight(4), 2 * s.BLEED + 4 * s.CARD_H + 3 * s.GAP);
  assert.equal(s.progress({ timeout: 10, remaining: 2.5 }), 0.25);
  assert.equal(s.progress({ timeout: 0, remaining: 0 }), 1);
  assert.equal(s.progress({ timeout: 10, remaining: -1 }), 0);
});
