// WebView2 browser accelerators every window swallows (W3-C in the shelf; shared module and the
// settings window / HUD — W4, FIX-15). Physical keys, so the Russian layout matches too.
import assert from "node:assert/strict";
import test from "node:test";
import { importTs } from "./helpers/bundle.mjs";

const { isBrowserShortcut } = await importTs("src/lib/browserKeys.ts");
const { resolveKey } = await importTs("src/windows/picker/keymap.ts");

const key = (code, mods = {}) => ({ code, key: "", ctrlKey: false, shiftKey: false, altKey: false, metaKey: false, ...mods });
const b = (code, mods, opts) => isBrowserShortcut(key(code, mods), opts);

test("browser accelerators are swallowed; the shelf's own chords and text editing untouched (W3)", () => {
  for (const code of ["F5", "F3", "F7"]) assert.ok(b(code), code);
  for (const code of ["KeyR", "KeyP", "KeyG", "KeyU", "KeyS", "KeyJ", "KeyH", "KeyN"]) assert.ok(b(code, { ctrlKey: true }), code);
  assert.ok(b("KeyR", { ctrlKey: true, shiftKey: true }), "hard reload");
  assert.ok(b("ArrowLeft", { altKey: true }), "Alt+← = back");
  // DevTools keys stay (no DevTools in release); AltGr (Ctrl+Alt) never matches.
  assert.ok(!b("KeyI", { ctrlKey: true, shiftKey: true }));
  assert.ok(!b("KeyJ", { ctrlKey: true, shiftKey: true }));
  assert.ok(!b("F12"));
  assert.ok(!b("KeyR", { ctrlKey: true, altKey: true }));
  // Text editing and the shelf's own keys are not browser shortcuts.
  for (const code of ["KeyA", "KeyC", "KeyV", "KeyX", "KeyZ", "KeyD", "KeyK", "KeyE", "KeyO", "KeyF"]) assert.ok(!b(code, { ctrlKey: true }), code);
  assert.ok(!b("ArrowLeft"));
  assert.ok(!b("ArrowLeft", { shiftKey: true }));
  assert.ok(!b("Backspace", { ctrlKey: true }));
  // Ctrl+Shift+S is the stack chord: not a browser key, and the key map resolves it first.
  assert.ok(!b("KeyS", { ctrlKey: true, shiftKey: true }));
  const empty = { inSearch: true, inOtherInput: false, onButton: false, searchEmpty: true, caretAtEnd: true, undoAvailable: false };
  assert.deepEqual(resolveKey(key("KeyS", { ctrlKey: true, shiftKey: true }), empty), { type: "stack" });
});

test("windows without their own Ctrl+F also swallow find-on-page (settings, HUD — W4)", () => {
  assert.ok(b("KeyF", { ctrlKey: true }, { find: true }));
  assert.ok(!b("KeyF", { ctrlKey: true }), "the shelf keeps Ctrl+F for its search");
  assert.ok(!b("KeyF", { ctrlKey: true, shiftKey: true }, { find: true }));
  assert.ok(!b("KeyF", { ctrlKey: true, altKey: true }, { find: true }), "AltGr+F types a character");
  assert.ok(!b("KeyF", {}, { find: true }), "plain F types");
  // The rest of the set is the same with the option.
  assert.ok(b("F5", {}, { find: true }));
  assert.ok(b("KeyP", { ctrlKey: true }, { find: true }));
  for (const code of ["KeyA", "KeyC", "KeyV", "KeyX", "KeyZ"]) assert.ok(!b(code, { ctrlKey: true }, { find: true }), code);
});
