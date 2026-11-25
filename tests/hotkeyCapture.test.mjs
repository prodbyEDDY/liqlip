// Ported from Liquid Tools tests/hotkeyCapture.test.mjs (prelude re-wired to the Liqlip IPC).
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

import { transform } from "esbuild";

async function loadRecorder() {
  const source = await readFile(
    new URL("../src/lib/hotkeyCapture.ts", import.meta.url),
    "utf8",
  );
  const body = source.slice(source.indexOf("export interface CaptureSpec"));
  const nativeCallbacks = [];
  const listeners = new Map();
  const window = {
    __TAURI_INTERNALS__: {},
    addEventListener(type, listener) {
      if (!listeners.has(type)) listeners.set(type, new Set());
      listeners.get(type).add(listener);
    },
    removeEventListener(type, listener) {
      listeners.get(type)?.delete(listener);
    },
  };
  const prelude = `
    const keyToken = (event) =>
      event.code.startsWith("Key") ? event.code.slice(3) : null;
    const activeModifiers = (event) => [
      ...(event.ctrlKey ? ["Ctrl"] : []),
      ...(event.altKey ? ["Alt"] : []),
      ...(event.shiftKey ? ["Shift"] : []),
      ...(event.metaKey ? ["Super"] : []),
    ];
    const isModifierCode = (code) =>
      /^(Control|Alt|Shift|Meta)(Left|Right)$/.test(code);
    const modifierToken = (code) => ({
      ControlLeft: "LCtrl",
      ControlRight: "RCtrl",
      AltLeft: "LAlt",
      AltRight: "RAlt",
      ShiftLeft: "LShift",
      ShiftRight: "RShift",
      MetaLeft: "LWin",
      MetaRight: "RWin",
    })[code] ?? null;
    const mouseButtonToken = () => null;
    const isTauri = () => true;
    const hotkeyRecordStart = async () => {};
    const hotkeyRecordStop = async () => {};
    const onHotkeysCaptured = async (callback) => {
      nativeCallbacks.push(callback);
      return () => {};
    };
  `;
  const compiled = await transform(`${prelude}\n${body}`, {
    format: "cjs",
    loader: "ts",
    target: "es2022",
  });
  const module = { exports: {} };
  vm.runInNewContext(compiled.code, {
    console,
    module,
    exports: module.exports,
    nativeCallbacks,
    window,
  });
  return {
    startHotkeyCapture: module.exports.startHotkeyCapture,
    listeners,
    nativeCallbacks,
  };
}

test("Tauri recorder falls back to focused-window keyboard capture", async () => {
  const { startHotkeyCapture, listeners, nativeCallbacks } = await loadRecorder();
  const commits = [];
  const session = startHotkeyCapture({
    onCommit: (combo) => commits.push(combo),
    onCancel: () => {},
  });

  assert.equal(listeners.get("keydown")?.size, 1);

  const event = {
    key: "V",
    code: "KeyV",
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    metaKey: false,
    preventDefault() {},
    stopPropagation() {},
  };
  listeners.get("keydown").values().next().value(event);
  await Promise.resolve();
  nativeCallbacks[0]?.({ combo: "V" });

  assert.deepEqual(commits, ["V"]);

  session.stop();
  assert.equal(listeners.get("keydown")?.size, 0);
});

test("records a chord whose final key is a sided modifier", async () => {
  const { startHotkeyCapture, listeners } = await loadRecorder();
  const commits = [];
  startHotkeyCapture({
    bareModifier: true,
    onCommit: (combo) => commits.push(combo),
    onCancel: () => {},
  });

  const dispatch = (type, event) =>
    listeners.get(type).values().next().value({
      altKey: false,
      shiftKey: false,
      preventDefault() {},
      stopPropagation() {},
      ...event,
    });

  dispatch("keydown", {
    code: "ControlLeft",
    ctrlKey: true,
    metaKey: false,
  });
  dispatch("keydown", {
    code: "MetaLeft",
    ctrlKey: true,
    metaKey: true,
  });
  dispatch("keyup", {
    code: "MetaLeft",
    ctrlKey: true,
    metaKey: false,
  });

  assert.deepEqual(commits, ["Ctrl + LWin"]);
});

test("native recorder: an empty combo means cancelled", async () => {
  const { startHotkeyCapture, nativeCallbacks } = await loadRecorder();
  const events = [];
  startHotkeyCapture({
    onCommit: (combo) => events.push(["commit", combo]),
    onCancel: () => events.push(["cancel"]),
  });
  await Promise.resolve();
  nativeCallbacks[0]?.({ combo: "" });
  nativeCallbacks[0]?.({ combo: "Alt + V" }); // after settling, later events are ignored
  assert.deepEqual(events, [["cancel"]]);
});
