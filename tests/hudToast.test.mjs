// HUD toast logic (W1-D): template segments, "same toast extends", browser preview query,
// and that every HudKind of the contract has a key + icon.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { importTs } from "./helpers/bundle.mjs";

const hud = await importTs("src/windows/hud/toast.ts");

test("segments split on {keys}", () => {
  assert.deepEqual(hud.segments("Copied — press {keys}"), [
    { kind: "text", text: "Copied — press " },
    { kind: "keys" },
  ]);
  assert.deepEqual(hud.segments("Paste stack is empty"), [{ kind: "text", text: "Paste stack is empty" }]);
  assert.deepEqual(hud.segments("{keys} then {keys}"), [
    { kind: "keys" },
    { kind: "text", text: " then " },
    { kind: "keys" },
  ]);
});

test("a repeated show of the visible toast only extends it", () => {
  const copied = { kind: "copied" };
  assert.equal(hud.isSameToast(copied, true, { kind: "copied" }), true);
  assert.equal(hud.isSameToast(copied, false, { kind: "copied" }), false, "hidden → plays again");
  assert.equal(hud.isSameToast(copied, true, { kind: "paused" }), false);
  assert.equal(hud.isSameToast(copied, true, { kind: "copied", text: "x" }), false);
  assert.equal(hud.isSameToast(null, true, copied), false);
});

test("browser preview query", () => {
  assert.deepEqual(hud.previewFromQuery("?kind=copied"), { toast: { kind: "copied" }, demo: false });
  assert.deepEqual(hud.previewFromQuery("?kind=elevated&text=Hi"), { toast: { kind: "elevated", text: "Hi" }, demo: false });
  assert.deepEqual(hud.previewFromQuery("?kind=bogus"), { toast: null, demo: false });
  assert.deepEqual(hud.previewFromQuery("?demo=1"), { toast: null, demo: true });
});

test("every contract HudKind has an i18n key and an icon", async () => {
  const types = await readFile(new URL("../src/lib/ipc/types.ts", import.meta.url), "utf8");
  const union = types.match(/export type HudKind = ([^;]+);/)[1];
  const kinds = [...union.matchAll(/"([a-z_]+)"/g)].map((m) => m[1]);
  assert.deepEqual([...kinds].sort(), [...hud.HUD_KINDS].sort());
  const en = await readFile(new URL("../src/lib/i18n/en.ts", import.meta.url), "utf8");
  const icons = await readFile(new URL("../src/lib/icons.ts", import.meta.url), "utf8");
  for (const k of kinds) {
    assert.ok(en.includes(`"${hud.KIND_KEY[k]}":`), `i18n key for ${k}`);
    assert.ok(icons.includes(`  ${hud.KIND_ICON[k]}: _`), `icon for ${k}`);
  }
  assert.ok(hud.VISIBLE_MS < 1600, "exit finishes before Rust hides the window");
});
