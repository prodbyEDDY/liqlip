// QAO controller (W6-E, src/windows/shots/qao.ts): window height, timers and their pauses, the
// drag-out lifecycle (the meaning of Liquid Tools' `hudLifecycle` tests), the buttons, toasts
// inside the card, `clips:changed`. Commands are fakes; the clock is manual.
import assert from "node:assert/strict";
import test from "node:test";
import { importTs } from "./helpers/bundle.mjs";

const { createQao, initialState, errorCode, SHRINK_MS, TOAST_MS } = await importTs("src/windows/shots/qao.ts");
const { stackHeight } = await importTs("src/windows/shots/stack.ts");

const flush = () => new Promise((r) => setImmediate(r));
function deferred() {
  let resolve, reject;
  const promise = new Promise((a, b) => ((resolve = a), (reject = b)));
  return { promise, resolve, reject };
}

const clip = (id, over = {}) => ({ id, kind: "screenshot", thumbUrl: `thumb-${id}`, width: 800, height: 600, meta: {}, ...over });
const added = (id, over = {}) => ({ clip: clip(id), savedPath: null, tempPath: null, monitor: "D1", error: null, ...over });
const P = (x = 10, y = 10, extra = {}) => ({ pointerId: 1, x, y, button: 0, buttons: 1, ...extra });

/** A QAO with fake commands, a manual clock and a manual scheduler. */
function harness(over = {}, settings = {}) {
  let now = 0;
  const timers = [];
  const calls = [];
  const log = (name, ...args) => calls.push([name, ...args]);
  const drag = deferred();
  const api = {
    clipCopy: async (req) => (log("copy", req), { result: "copied" }),
    clipDetail: async (id) => (log("detail", id), { ...clip(id), thumbUrl: `thumb-${id}-v2`, imageUrl: `media-${id}` }),
    clipDragOut: (ids) => (log("drag", ids), drag.promise),
    clipOcr: async (id) => (log("ocr", id), "Hello"),
    clipsDelete: async (ids) => (log("delete", ids), { undoToken: "u" }),
    textCopy: async (text) => log("textCopy", text),
    shotEdit: async (id) => log("edit", id),
    shotPin: async (id) => log("pin", id),
    shotSave: async (id) => (log("save", id), `C:\\Pictures\\Liqlip\\${id}.png`),
    shotReveal: async (id) => log("reveal", id),
    shotsHudLayout: async (h) => log("layout", h),
    shotsHudHide: async () => log("hide"),
    settingsOpen: async (s) => log("settings", s),
    shutter: () => log("shutter"),
    now: () => now,
    later: (fn, ms) => {
      const t = { fn, at: now + ms, live: true };
      timers.push(t);
      return () => (t.live = false);
    },
    ...over,
  };
  const state = { ...initialState(), ...settings };
  const qao = createQao(state, api);
  qao.tick(); // start the clock
  return {
    qao,
    state,
    calls,
    drag,
    names: () => calls.map((c) => c[0]),
    last: (name) => calls.filter((c) => c[0] === name).at(-1),
    keys: () => state.items.map((i) => i.key),
    /** Advance the clock in 100 ms ticks, firing due timers. */
    advance(ms) {
      for (let t = 0; t < ms; t += 100) {
        now += 100;
        for (const x of timers) if (x.live && x.at <= now) (x.live = false), x.fn();
        qao.tick();
      }
    },
  };
}

test("a shot adds a card, grows the window at once and clicks the shutter (if on)", () => {
  const h = harness();
  h.qao.add(added(1));
  assert.deepEqual(h.keys(), ["clip:1"]);
  assert.deepEqual(h.last("layout"), ["layout", stackHeight(1)]);
  assert.ok(h.names().includes("shutter"));
  const quiet = harness({}, { sound: false });
  quiet.qao.add(added(1));
  assert.ok(!quiet.names().includes("shutter"));
  quiet.qao.add(added(0, { clip: null })); // no record and no temp file: nothing to show
  assert.equal(quiet.state.items.length, 1);
});

test("an expired card leaves; the window shrinks after the exit animation, hides when empty", () => {
  const h = harness({}, { timeout: 1 });
  h.qao.add(added(1));
  h.advance(900);
  assert.equal(h.state.items.length, 1);
  h.advance(200);
  assert.equal(h.state.items.length, 0, "gone after 1 s");
  assert.ok(!h.names().includes("hide"), "not before the animation");
  h.advance(SHRINK_MS);
  assert.deepEqual(h.calls.at(-1), ["hide"]);
});

test("hover, a drag, a capture session and a hidden page pause every timer; 0 never hides", () => {
  const h = harness({}, { timeout: 1 });
  h.qao.add(added(1));
  h.qao.add(added(2));
  h.qao.enter();
  h.advance(3000);
  assert.equal(h.state.items.length, 2, "hovered: paused");
  h.qao.leave();
  h.qao.sessionStarted();
  h.advance(3000);
  assert.equal(h.state.items.length, 2, "capturing: paused until shot:end");
  h.qao.sessionEnded();
  h.qao.setHidden(true);
  h.advance(3000);
  assert.equal(h.state.items.length, 2, "hidden page: paused");
  h.qao.setHidden(false);
  h.advance(1100);
  assert.equal(h.state.items.length, 0, "resumed");

  const never = harness({}, { timeout: 0 });
  never.qao.add(added(1));
  never.advance(60_000);
  assert.equal(never.state.items.length, 1);
});

test("a new shot ends a capture pause even if shot:end never came", () => {
  const h = harness({}, { timeout: 1 });
  h.qao.sessionStarted();
  h.qao.add(added(1));
  assert.equal(h.state.capturing, false);
});

test("drag-out: waits for the drop and closes only the dragged card", async () => {
  const h = harness({}, { timeout: 1 });
  h.qao.add(added(1));
  assert.equal(h.qao.press("clip:1", P()), true);
  assert.equal(h.qao.move("clip:1", P(13, 10)), false, "≤ 6 px is still a click");
  assert.equal(h.qao.move("clip:1", P(18, 10)), true, "> 6 px starts the drag");
  assert.deepEqual(h.last("drag"), ["drag", [1]]);
  h.advance(5000);
  assert.equal(h.state.items.length, 1, "the timer waits while dragging");
  assert.equal(h.state.dragging, "clip:1");
  h.drag.resolve("dropped");
  await h.qao.settled();
  assert.equal(h.state.items.length, 0);
  assert.equal(h.state.dragging, null);
  h.advance(SHRINK_MS);
  assert.ok(h.names().includes("hide"));
  h.qao.release("clip:1", P(18, 10));
  assert.ok(!h.names().includes("edit"), "the drag's release is not a click");
});

for (const outcome of ["cancelled", "error"]) {
  test(`a ${outcome} drag resumes the full timer and clears the stuck hover`, async () => {
    const h = harness({}, { timeout: 2 });
    h.qao.add(added(1));
    h.advance(1500);
    h.qao.enter();
    h.qao.press("clip:1", P());
    h.qao.move("clip:1", P(30, 10));
    if (outcome === "error") h.drag.reject("failed: DoDragDrop");
    else h.drag.resolve("cancelled");
    await h.qao.settled();
    const s = h.state.items[0];
    assert.equal(h.state.dragging, null);
    assert.equal(h.state.hovered, false, "pointerleave was swallowed by the drag loop");
    assert.equal(s.remaining, 2, "full time again");
    assert.equal(Boolean(s.toast?.error), outcome === "error");
    if (outcome === "error") assert.equal(s.toast.key, "shots.toast.dragFailed");
    h.advance(2100);
    assert.equal(h.state.items.length, 0, "and then it runs out");
  });
}

test("an old drag finishing never touches a newer card", async () => {
  const h = harness();
  h.qao.add(added(1));
  h.qao.press("clip:1", P());
  h.qao.move("clip:1", P(40, 10));
  h.qao.add(added(2));
  h.drag.resolve("dropped");
  await h.qao.settled();
  assert.deepEqual(h.keys(), ["clip:2"]);
  h.advance(SHRINK_MS);
  assert.deepEqual(h.last("layout"), ["layout", stackHeight(1)]);
});

test("pointercancel never opens the editor; a click does; a released button is no drag", async () => {
  const h = harness();
  h.qao.add(added(1));
  h.qao.press("clip:1", P());
  h.qao.cancel();
  h.qao.release("clip:1", P());
  assert.ok(!h.names().includes("edit"));
  h.qao.press("clip:1", P());
  h.qao.release("clip:1", P());
  await h.qao.settled();
  assert.deepEqual(h.last("edit"), ["edit", 1]);
  h.qao.press("clip:1", P());
  assert.equal(h.qao.move("clip:1", P(50, 50, { buttons: 0 })), false, "button already up: no drag");
  assert.equal(h.qao.press("clip:1", P(10, 10, { button: 2 })), false, "right button: nothing");
  h.qao.release("clip:1", { pointerId: 2, x: 0, y: 0 });
  assert.equal(h.names().filter((n) => n === "edit").length, 1, "another pointer's release is not a click");
});

test("an edit failure keeps the timer running and says so inside the card", async () => {
  const h = harness({ shotEdit: async () => Promise.reject("failed: window") }, { timeout: 1 });
  h.qao.add(added(1));
  await h.qao.act("clip:1", "edit");
  assert.equal(h.state.items[0].toast.key, "shots.toast.editFailed");
  h.advance(1100);
  assert.equal(h.state.items.length, 0);
});

test("buttons: copy, save → show in folder, text, pin, delete, close", async () => {
  const h = harness();
  h.qao.add(added(1));
  await h.qao.act("clip:1", "copy");
  assert.deepEqual(h.last("copy"), ["copy", { ids: [1], mode: "auto" }]);
  assert.equal(h.state.items[0].toast.key, "shots.toast.copied");
  h.advance(TOAST_MS + 100);
  assert.equal(h.state.items[0].toast, null, "toasts expire");

  await h.qao.act("clip:1", "save");
  assert.equal(h.state.items[0].savedPath, "C:\\Pictures\\Liqlip\\1.png");
  assert.equal(h.state.items[0].toast.key, "shots.toast.saved");
  await h.qao.act("clip:1", "reveal");
  assert.deepEqual(h.last("reveal"), ["reveal", 1]);

  await h.qao.act("clip:1", "text");
  assert.deepEqual(h.last("textCopy"), ["textCopy", "Hello"]);
  assert.equal(h.state.items[0].toast.key, "shots.toast.textCopied");

  await h.qao.act("clip:1", "pin");
  assert.deepEqual(h.last("pin"), ["pin", 1]);

  await h.qao.act("clip:1", "folder");
  assert.deepEqual(h.last("settings"), ["settings", "screenshot"]);

  h.qao.add(added(2));
  await h.qao.act("clip:2", "delete");
  assert.deepEqual(h.last("delete"), ["delete", [2]]);
  await h.qao.act("clip:1", "close");
  assert.deepEqual(h.keys(), []);
  assert.equal(h.state.items.length, 0);
});

test("text recognition: no text, off, gated, busy", async () => {
  const empty = harness({ clipOcr: async () => "  \n" });
  empty.qao.add(added(1));
  await empty.qao.act("clip:1", "text");
  assert.equal(empty.state.items[0].toast.key, "shots.toast.textEmpty");
  assert.ok(!empty.names().includes("textCopy"));

  const off = harness({ clipOcr: async () => Promise.reject("invalid: text recognition is turned off") });
  off.qao.add(added(1));
  await off.qao.act("clip:1", "text");
  assert.equal(off.state.items[0].toast.key, "shots.textOff");

  const gated = harness({}, { ocr: "nolang" });
  gated.qao.add(added(1));
  await gated.qao.act("clip:1", "text");
  assert.ok(!gated.names().includes("ocr"), "the disabled button does nothing");

  const slow = deferred();
  const busy = harness({ clipOcr: () => slow.promise });
  busy.qao.add(added(1));
  const first = busy.qao.act("clip:1", "text");
  assert.equal(busy.state.items[0].busy, "text");
  assert.equal(busy.state.items[0].toast.key, "shots.toast.textRunning");
  await busy.qao.act("clip:1", "copy");
  assert.ok(!busy.names().includes("copy"), "one command at a time per card");
  slow.resolve("x");
  await first;
  assert.equal(busy.state.items[0].busy, null);
});

test("save errors: the persistent notice (the card waits); a moved file brings Save back", async () => {
  const h = harness(
    { shotSave: async () => Promise.reject("failed: access denied"), shotReveal: async () => Promise.reject("not_found: file") },
    { timeout: 1 },
  );
  h.qao.add(added(1, { savedPath: "C:\\x.png" }));
  await h.qao.act("clip:1", "reveal");
  assert.equal(h.state.items[0].savedPath, null);
  assert.equal(h.state.items[0].toast.key, "shots.toast.fileMissing");
  await h.qao.act("clip:1", "save");
  assert.equal(h.state.items[0].error, "save_failed");
  h.advance(5000);
  assert.equal(h.state.items.length, 1, "a failed save does not vanish unseen");

  const fromEvent = harness({}, { timeout: 1 });
  fromEvent.qao.add(added(3, { error: "save_failed" }));
  fromEvent.advance(5000);
  assert.equal(fromEvent.state.items[0].error, "save_failed");
});

test("copy outcomes: a deleted record, a busy clipboard", async () => {
  const gone = harness({ clipCopy: async () => ({ result: "failed", reason: "not_found" }) });
  gone.qao.add(added(1));
  await gone.qao.act("clip:1", "copy");
  assert.equal(gone.state.items[0].toast.key, "shots.toast.gone");
  const busy = harness({ clipCopy: async () => ({ result: "failed", reason: "clipboard_busy" }) });
  busy.qao.add(added(1));
  await busy.qao.act("clip:1", "copy");
  assert.equal(busy.state.items[0].toast.key, "shots.toast.busy");
});

test("a shot kept out of history (clip: null): Copy and Save as… by its temp file name, nothing else", async () => {
  const h = harness({
    shotTempCopy: async (name) => h.calls.push(["tempCopy", name]),
    shotTempSaveAs: async (name) => (h.calls.push(["tempSaveAs", name]), `C:\\Pictures\\${name}`),
  });
  h.qao.add(added(0, { clip: null, tempPath: "C:\\Temp\\Liqlip\\shots\\shot-1-0.png" }));
  const key = h.keys()[0];
  assert.equal(key, "tmp:C:\\Temp\\Liqlip\\shots\\shot-1-0.png");
  assert.equal(h.state.items[0].preview, "http://liq.localhost/temp/shot-1-0.png", "the preview is the temp route");
  for (const cmd of ["edit", "pin", "save", "text", "delete", "reveal"]) await h.qao.act(key, cmd);
  assert.equal(h.qao.press(key, P()), false, "no drag-out");
  assert.deepEqual(h.names().filter((n) => !["layout", "shutter"].includes(n)), [], "record commands never run");
  await h.qao.act(key, "copy");
  assert.deepEqual(h.last("tempCopy"), ["tempCopy", "shot-1-0.png"]);
  assert.equal(h.state.items[0].toast.key, "shots.toast.copied");
  await h.qao.act(key, "saveAs");
  assert.deepEqual(h.last("tempSaveAs"), ["tempSaveAs", "shot-1-0.png"]);
  assert.equal(h.state.items[0].toast.key, "shots.toast.saved");
  await h.qao.act(key, "close");
  assert.equal(h.state.items.length, 0);
});

test("a temp card whose file is gone says so; a cancelled Save as… stays quiet", async () => {
  const h = harness({ shotTempCopy: async () => Promise.reject("not_found: the temporary picture is gone"), shotTempSaveAs: async () => null });
  h.qao.add(added(0, { clip: null, tempPath: "C:\\Temp\\Liqlip\\shots\\shot-2-0.png" }));
  const key = h.keys()[0];
  await h.qao.act(key, "saveAs");
  assert.equal(h.state.items[0].toast, null, "null = the dialog was cancelled");
  await h.qao.act(key, "copy");
  assert.equal(h.state.items[0].toast.key, "shots.toast.tempGone");
  // a card with a record has no Save as… of its own
  h.qao.add(added(5));
  await h.qao.act("clip:5", "saveAs");
  assert.equal(h.state.items.find((i) => i.key === "clip:5").busy, null);
});

test("no thumbnail in the payload: the full picture from clip_detail", async () => {
  const h = harness();
  h.qao.add(added(4, { clip: clip(4, { thumbUrl: null }) }));
  await h.qao.settled();
  assert.equal(h.state.items[0].preview, "thumb-4-v2");
});

test("clips:changed: deleted cards leave, updated ones refresh, reset re-checks", async () => {
  const h = harness({
    clipDetail: async (id) => {
      if (id === 3) throw "not_found: clip 3";
      return { ...clip(id), thumbUrl: `thumb-${id}-v2`, imageUrl: null, meta: { shot: { savedPath: `C:\\${id}.png`, edited: true } } };
    },
  });
  for (const id of [1, 2, 3]) h.qao.add(added(id));
  await h.qao.clipsChanged({ origin: "user", change: "deleted", ids: [2] });
  assert.deepEqual(h.keys(), ["clip:1", "clip:3"]);
  await h.qao.clipsChanged({ origin: "user", change: "updated", ids: [1] });
  assert.equal(h.state.items[0].preview, "thumb-1-v2");
  assert.equal(h.state.items[0].savedPath, "C:\\1.png");
  await h.qao.clipsChanged({ origin: "user", change: "reset" });
  assert.deepEqual(h.keys(), ["clip:1"], "a record that no longer exists leaves");
  await h.qao.clipsChanged({ origin: "capture", change: "added", ids: [9] });
  assert.deepEqual(h.keys(), ["clip:1"]);
});

test("errorCode reads the machine code of a rejection", () => {
  assert.equal(errorCode("not_found: clip 1"), "not_found");
  assert.equal(errorCode(new Error("busy: db")), "busy");
  assert.equal(errorCode("weird"), "unknown");
  assert.equal(errorCode(undefined), "unknown");
});
