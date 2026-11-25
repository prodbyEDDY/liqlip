// i18n catalogues and interpolation (W3-D): src/lib/i18n/{en,ru,format}.ts.
// ru.ts is typed Record<TranslationKey, string>, so a missing key is a compile error; these
// tests add what the compiler can't see: block order, the same {slots} in both languages,
// one-pass interpolation, and no dead keys outside the shelf's own `picker.*` block.
import assert from "node:assert/strict";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";
import { importTs, root } from "./helpers/bundle.mjs";

const { en, ru, interpolate } = await importTs("src/lib/i18n/en.ts", "src/lib/i18n/ru.ts", "src/lib/i18n/format.ts");

const slots = (s) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort().join(",");

test("en and ru have the same keys in the same order", () => {
  assert.deepEqual(Object.keys(ru), Object.keys(en));
});

test("every translation keeps the same {slots} and is not empty", () => {
  for (const k of Object.keys(en)) {
    assert.ok(en[k].trim() && ru[k].trim(), `${k} is empty`);
    assert.equal(slots(ru[k]), slots(en[k]), `${k}: {slots} differ`);
  }
});

test("interpolate fills slots in one pass, values verbatim", () => {
  assert.equal(interpolate("Delete “{name}”?", { name: "Work" }), "Delete “Work”?");
  assert.equal(interpolate("{a} and {b}", { a: "{b}", b: "x" }), "{b} and x", "a value is never re-scanned");
  assert.equal(interpolate("Folder {name}", { name: "$& $1 $$" }), "Folder $& $1 $$", "no replacement patterns");
  assert.equal(interpolate("{n} of {total}", { n: 3, total: 10 }), "3 of 10");
  assert.equal(interpolate("Copied — press {keys}", { other: 1 }), "Copied — press {keys}", "unknown slots stay");
  assert.equal(interpolate("Plain {keys}"), "Plain {keys}", "no vars → as written");
  assert.equal(interpolate("{toString}", {}), "{toString}", "only own properties count");
});

// ── dead keys ────────────────────────────────────────────────────────────────
// A key is alive when a source file names it literally, or builds it from a template
// prefix (`settings.section.${key}`) or suffix (`${label}.desc`). `picker.*` belongs to the
// shelf agent and is left out here.
function walk(dir, out = []) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) {
      if (name !== "heroicons" && name !== "i18n") walk(p, out);
    } else if (/\.(ts|svelte)$/.test(name)) out.push(p);
  }
  return out;
}

test("no dead keys outside picker.*", () => {
  const src = walk(join(root, "src")).map((f) => readFileSync(f, "utf8")).join("\n");
  const prefixes = [...src.matchAll(/[`"']([a-z][\w.]*\.)\$\{/g)].map((m) => m[1]).filter((p) => p.split(".").length > 2);
  const suffixes = [...src.matchAll(/\$\{[^}]+\}(\.[\w.]+)`/g)].map((m) => m[1]);
  const dead = Object.keys(en).filter(
    (k) =>
      !k.startsWith("picker.") &&
      !src.includes(`"${k}"`) &&
      !src.includes(`'${k}'`) &&
      !src.includes(`\`${k}\``) &&
      !prefixes.some((p) => k.startsWith(p)) &&
      !suffixes.some((s) => k.endsWith(s)),
  );
  assert.deepEqual(dead, []);
});
