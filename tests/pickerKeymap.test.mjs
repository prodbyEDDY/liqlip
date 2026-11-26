// Shelf key map (W1-E): physical keys (Russian layout), focus context, every SPEC §3.5 chord.
import assert from "node:assert/strict";
import test from "node:test";
import { importTs } from "./helpers/bundle.mjs";

const { resolveKey, isTypingKey, isSwallowedRepeat, clickIntent } = await importTs("src/windows/picker/keymap.ts");

const key = (code, mods = {}, keyChar = "") => ({ code, key: keyChar, ctrlKey: false, shiftKey: false, altKey: false, metaKey: false, ...mods });
const empty = { inSearch: true, inOtherInput: false, onButton: false, searchEmpty: true, caretAtEnd: true, undoAvailable: false };
const typed = { ...empty, searchEmpty: false, caretAtEnd: false };
const r = (code, mods, ctx = empty, k) => resolveKey(key(code, mods, k), ctx);

test("shortcuts match physical keys (Russian layout)", () => {
  assert.deepEqual(r("KeyD", { ctrlKey: true }, empty, "в"), { type: "favorite" });
  assert.deepEqual(r("KeyK", { ctrlKey: true }, empty, "л"), { type: "actions" });
  assert.deepEqual(r("KeyE", { ctrlKey: true }, empty, "у"), { type: "edit" }, "W2-C: Ctrl+E edits (Quick Look)");
  assert.equal(r("KeyE", { ctrlKey: true, shiftKey: true }), null);
  assert.deepEqual(r("KeyO", { ctrlKey: true, shiftKey: true }, empty, "Щ"), { type: "reveal" });
  assert.deepEqual(r("Comma", { ctrlKey: true }, empty, "б"), { type: "settings" });
});

test("Enter family", () => {
  assert.deepEqual(r("Enter"), { type: "paste" });
  assert.deepEqual(r("NumpadEnter"), { type: "paste" });
  assert.deepEqual(r("Enter", { shiftKey: true }), { type: "pastePlain" });
  assert.deepEqual(r("Enter", { ctrlKey: true }), { type: "copy" });
  assert.deepEqual(r("Enter", { altKey: true }), { type: "pasteKeepOpen" });
  assert.equal(r("Enter", {}, { ...empty, onButton: true }), null, "a focused button owns Enter");
});

test("digits: Ctrl paste N, Ctrl+Shift plain, Alt pill N", () => {
  assert.deepEqual(r("Digit3", { ctrlKey: true }), { type: "pasteNth", n: 3, plain: false });
  assert.deepEqual(r("Numpad9", { ctrlKey: true, shiftKey: true }), { type: "pasteNth", n: 9, plain: true });
  assert.deepEqual(r("Digit2", { altKey: true }), { type: "pill", n: 2 });
  assert.equal(r("Digit0", { ctrlKey: true }), null);
  assert.equal(r("Digit1", { ctrlKey: true, altKey: true }), null, "AltGr is not a shelf chord");
});

test("arrows: grid nav; ←/→/Home/End stay with the caret while the search has text", () => {
  assert.deepEqual(r("ArrowDown"), { type: "nav", dir: "down", extend: false });
  assert.deepEqual(r("ArrowUp", {}, typed), { type: "nav", dir: "up", extend: false });
  assert.deepEqual(r("ArrowLeft"), { type: "nav", dir: "left", extend: false });
  assert.equal(r("ArrowLeft", {}, typed), null);
  assert.equal(r("Home", {}, typed), null);
  assert.deepEqual(r("End"), { type: "nav", dir: "end", extend: false });
  assert.deepEqual(r("ArrowRight", { shiftKey: true }), { type: "nav", dir: "right", extend: true });
  assert.deepEqual(r("PageDown", {}, typed), { type: "nav", dir: "pageDown", extend: false });
});

test("Space previews only with an empty search; Ctrl+Y always", () => {
  assert.deepEqual(r("Space"), { type: "preview" });
  assert.equal(r("Space", {}, typed), null);
  assert.deepEqual(r("KeyY", { ctrlKey: true }, typed), { type: "preview" });
});

test("Delete / Ctrl+A / Ctrl+Z respect text editing", () => {
  assert.deepEqual(r("Delete"), { type: "delete" });
  assert.equal(r("Delete", {}, typed), null);
  assert.deepEqual(r("Delete", {}, { ...typed, caretAtEnd: true }), { type: "delete" });
  assert.deepEqual(r("KeyA", { ctrlKey: true }), { type: "selectAll" });
  assert.equal(r("KeyA", { ctrlKey: true }, typed), null);
  assert.equal(r("KeyZ", { ctrlKey: true }, typed), null, "text undo while typing");
  assert.deepEqual(r("KeyZ", { ctrlKey: true }, { ...typed, undoAvailable: true }), { type: "undo" });
});

test("pills, stack, rename, search focus, Esc; other inputs own the keyboard", () => {
  assert.deepEqual(r("Tab", { ctrlKey: true }), { type: "pillCycle", delta: 1 });
  assert.deepEqual(r("Tab", { ctrlKey: true, shiftKey: true }), { type: "pillCycle", delta: -1 });
  assert.deepEqual(r("KeyS", { ctrlKey: true, shiftKey: true }), { type: "stack" });
  assert.equal(r("KeyS", { ctrlKey: true }), null);
  assert.deepEqual(r("F2"), { type: "rename" });
  assert.deepEqual(r("KeyF", { ctrlKey: true }), { type: "focusSearch" });
  assert.deepEqual(r("Escape"), { type: "escape" });
  const other = { ...empty, inSearch: false, inOtherInput: true };
  assert.equal(r("Enter", {}, other), null);
  assert.equal(r("KeyD", { ctrlKey: true }, other), null);
  assert.deepEqual(r("Escape", {}, other), { type: "escape" });
  assert.equal(r("KeyD", { metaKey: true }), null);
});

test("auto-repeat never pastes twice; navigation keeps repeating", () => {
  const rep = (code, mods = {}) => resolveKey(key(code, { repeat: true, ...mods }), empty);
  assert.equal(rep("Enter"), null);
  assert.equal(rep("Digit1", { ctrlKey: true }), null);
  assert.equal(rep("Delete"), null);
  assert.equal(rep("KeyD", { ctrlKey: true }), null);
  assert.deepEqual(rep("ArrowDown"), { type: "nav", dir: "down", extend: false });
  assert.equal(isSwallowedRepeat(key("Enter", { repeat: true }), empty), true);
  assert.equal(isSwallowedRepeat(key("Enter"), empty), false);
  assert.equal(isSwallowedRepeat(key("ArrowDown", { repeat: true }), empty), false);
});

test("W6-F: Ctrl+P pins a picture; Ctrl+E is Edit for every kind; a bare E still types", () => {
  assert.deepEqual(r("KeyP", { ctrlKey: true }, empty, "з"), { type: "pin" }, "Russian layout");
  assert.deepEqual(r("KeyP", { ctrlKey: true }, typed), { type: "pin" }, "the search has no use for Ctrl+P (print otherwise)");
  assert.equal(r("KeyP", { ctrlKey: true, shiftKey: true }), null);
  assert.equal(resolveKey(key("KeyP", { ctrlKey: true, repeat: true }), empty), null, "a held Ctrl+P pins once");
  assert.equal(r("KeyE", {}, empty, "e"), null, "E goes to the search");
  assert.equal(isTypingKey(key("KeyE", {}, "у")), true);
  assert.deepEqual(r("KeyE", { ctrlKey: true }), { type: "edit" });
});

test("typing keys go to the search", () => {
  assert.equal(isTypingKey(key("KeyA", {}, "a")), true);
  assert.equal(isTypingKey(key("KeyF", {}, "а")), true);
  assert.equal(isTypingKey(key("KeyA", { ctrlKey: true }, "a")), false);
  assert.equal(isTypingKey(key("Space", {}, " ")), false);
  assert.equal(isTypingKey(key("ArrowUp", {}, "ArrowUp")), false);
});

test("click intent: one click per settings; a double click pastes only when a click copies (W3)", () => {
  assert.equal(clickIntent(1, true), "paste");
  assert.equal(clickIntent(1, false), "copy");
  // The pair's first click already pasted — the second one must not paste again.
  assert.equal(clickIntent(2, true), null);
  assert.equal(clickIntent(2, false), "paste");
  assert.equal(clickIntent(3, false), "paste");
});
