// Optimistic settings sync: src/windows/settings/sync.ts against the real mock
// settings_set (validation, taken hotkeys, settings:changed) and against a failing backend.
import assert from "node:assert/strict";
import test from "node:test";
import { importTs } from "./helpers/bundle.mjs";

const { SettingsSync, mockInvoke, mockListen } = await importTs(
  "src/windows/settings/sync.ts",
  "src/lib/ipc/mock/index.ts",
  "src/lib/ipc/mock/bus.ts",
);

const tick = () => new Promise((r) => setTimeout(r, 5));

function harness(send) {
  const snaps = [];
  const sync = new SettingsSync(send, (s) => snaps.push(s));
  return { sync, snaps, last: () => snaps.at(-1) };
}

const mockSend = (settings) => mockInvoke("settings_set", { settings });

test("edits apply optimistically, then settle on the backend result", async () => {
  const { sync, last } = harness(mockSend);
  sync.update("picker.hints", false);
  assert.equal(last(), undefined, "ignored before the first receive");
  sync.receive(await mockInvoke("settings_get", {}));
  sync.update("picker.hints", false);
  assert.equal(last().view.picker.hints, false, "optimistic");
  assert.equal(last().saving, true);
  await sync.idle();
  assert.equal(last().view.picker.hints, false);
  assert.equal(last().saving, false);
  assert.equal((await mockInvoke("settings_get", {})).picker.hints, false, "persisted");
});

test("a taken hotkey is reverted by the backend and reported at the field", async () => {
  const { sync, last } = harness(mockSend);
  sync.receive(await mockInvoke("settings_get", {}));
  sync.update("hotkeys.summon", "Alt + Tab");
  assert.equal(last().view.hotkeys.summon, "Alt + Tab");
  await sync.idle();
  assert.equal(last().view.hotkeys.summon, "Alt + V");
  assert.deepEqual(last().errors["hotkeys.summon"], { code: "taken" });
  // Editing the same field clears the error.
  sync.update("hotkeys.summon", "Ctrl + Alt + V");
  assert.equal(last().errors["hotkeys.summon"], undefined);
  await sync.idle();
  assert.equal(last().view.hotkeys.summon, "Ctrl + Alt + V");
  sync.update("hotkeys.summon", "Alt + V");
  await sync.idle();
});

test("rapid edits are serialized and batched; none is lost", async () => {
  const calls = [];
  const { sync, last } = harness(async (s) => {
    calls.push(structuredClone(s));
    await tick();
    return { settings: s, errors: [] };
  });
  sync.receive(await mockInvoke("settings_get", {}));
  sync.update("picker.hints", false);
  sync.update("picker.alwaysPlain", true);
  sync.update("storage.retention", "1y");
  await sync.idle();
  assert.equal(calls.length, 2, "first edit alone, the rest batched while it was in flight");
  assert.equal(calls[1].picker.hints, false, "the second request builds on the first");
  assert.equal(calls[1].picker.alwaysPlain, true);
  assert.equal(calls[1].storage.retention, "1y");
  assert.equal(last().view.storage.retention, "1y");
});

test("a rejected request rolls back only its own patches and marks them failed", async () => {
  let fail = true;
  const { sync, last } = harness(async (s) => {
    await tick();
    if (fail) throw "busy: disk";
    return { settings: s, errors: [] };
  });
  const base = await mockInvoke("settings_get", {});
  sync.receive(base);
  sync.update("general.theme", "dark");
  assert.equal(last().view.general.theme, "dark");
  await sync.idle();
  assert.equal(last().view.general.theme, base.general.theme, "rolled back");
  assert.equal(last().errors["general.theme"].code, "failed");
  assert.equal(last().errors["general.theme"].cause, "busy: disk");
  fail = false;
  sync.update("general.theme", "dark");
  await sync.idle();
  assert.equal(last().view.general.theme, "dark");
  assert.equal(last().errors["general.theme"], undefined);
});

test("settings:changed from elsewhere keeps pending edits on top", async () => {
  let release;
  const { sync, last } = harness(
    (s) =>
      new Promise((r) => {
        release = () => r({ settings: s, errors: [] });
      }),
  );
  const base = await mockInvoke("settings_get", {});
  sync.receive(base);
  sync.update("picker.hints", false);
  // Another window changes the theme while our request is in flight.
  sync.receive({ ...base, general: { ...base.general, theme: "light" } });
  assert.equal(last().view.general.theme, "light");
  assert.equal(last().view.picker.hints, false, "the in-flight edit still shows");
  release();
  await sync.idle();
});

test("recheck re-sends the confirmed state and reports conflicts", async () => {
  const { sync, last } = harness(async (s) => ({ settings: s, errors: [{ field: "hotkeys.summon", code: "taken" }] }));
  sync.receive(await mockInvoke("settings_get", {}));
  const errors = await sync.recheck();
  assert.deepEqual(errors, [{ field: "hotkeys.summon", code: "taken" }]);
  assert.deepEqual(last().errors["hotkeys.summon"], { code: "taken" });
  sync.clear("hotkeys");
  assert.equal(last().errors["hotkeys.summon"], undefined);
});

test("the mock emits settings:changed once per effective change", async () => {
  const events = [];
  const off = mockListen("settings:changed", (p) => events.push(p));
  const { sync } = harness(mockSend);
  sync.receive(await mockInvoke("settings_get", {}));
  sync.update("stack.newlineAfterPaste", true);
  await sync.idle();
  sync.update("stack.newlineAfterPaste", true); // no-op
  await sync.idle();
  await tick();
  off();
  assert.equal(events.length, 1);
  assert.equal(events[0].stack.newlineAfterPaste, true);
});

test("each answer's errors replace earlier taken/invalid ones; failed errors stay", async () => {
  let answer = [{ field: "hotkeys.summon", code: "taken" }];
  let fail = false;
  const { sync, last } = harness(async (s) => {
    if (fail) throw "busy: x";
    return { settings: s, errors: answer };
  });
  sync.receive(await mockInvoke("settings_get", {}));
  await sync.recheck();
  assert.deepEqual(last().errors["hotkeys.summon"], { code: "taken" });
  // The conflict persists: reported again with an unrelated edit.
  sync.update("picker.hints", true);
  await sync.idle();
  assert.deepEqual(last().errors["hotkeys.summon"], { code: "taken" });
  // An edit that fails keeps its own "failed" error...
  fail = true;
  sync.update("general.theme", "dark");
  await sync.idle();
  assert.equal(last().errors["general.theme"].code, "failed");
  // ...and once the other app lets go, the next answer clears the conflict only.
  fail = false;
  answer = [];
  await sync.recheck();
  assert.equal(last().errors["hotkeys.summon"], undefined);
  assert.equal(last().errors["general.theme"].code, "failed");
});

// W3-D: the refresh on "window shown" must not roll back an edit made while it was in flight.
test("a background refresh is dropped once anything happened after it started", async () => {
  const { sync, last } = harness(mockSend);
  const initial = await mockInvoke("settings_get", {});
  sync.receive(initial);

  // Nothing happened meanwhile → applied.
  let rev = sync.revision;
  assert.equal(sync.receiveIfCurrent({ ...initial, stack: { newlineAfterPaste: true } }, rev), true);
  assert.equal(last().view.stack.newlineAfterPaste, true);
  sync.receive(initial);

  // An edit after the refresh started → its stale answer is ignored.
  rev = sync.revision;
  const stale = structuredClone(initial);
  sync.update("picker.favoritesFirst", !initial.picker.favoritesFirst);
  await sync.idle();
  assert.equal(sync.receiveIfCurrent(stale, rev), false);
  assert.equal(last().view.picker.favoritesFirst, !initial.picker.favoritesFirst, "the edit survives");

  // settings:changed after the refresh started → ignored too.
  rev = sync.revision;
  const pushed = await mockInvoke("settings_get", {});
  sync.receive(pushed);
  assert.equal(sync.receiveIfCurrent(stale, rev), false);

  // While a request is in flight → ignored (its answer is newer).
  sync.update("picker.favoritesFirst", initial.picker.favoritesFirst);
  assert.equal(sync.receiveIfCurrent(stale, sync.revision), false);
  await sync.idle();
});
