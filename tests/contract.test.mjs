// Contract guard (coordinator, W0): the IPC contract lives in four places that must agree —
// docs/ARCHITECTURE.md §9.6, src-tauri/src/lib.rs (registration), src/lib/ipc/commands.ts
// (wrappers) and the browser mock — and model.rs must match types.ts field by field.
import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import { join } from "node:path";
import test from "node:test";
import { importTs, root } from "./helpers/bundle.mjs";

const read = (p) => readFile(join(root, p), "utf8");

async function rustFiles(dir) {
  const out = [];
  for (const e of await readdir(join(root, dir), { withFileTypes: true })) {
    const p = `${dir}/${e.name}`;
    if (e.isDirectory()) out.push(...(await rustFiles(p)));
    else if (e.name.endsWith(".rs")) out.push(p);
  }
  return out;
}

const sorted = (xs) => [...new Set(xs)].sort();

test("every §9.6 command is documented, registered, wrapped and mocked", async () => {
  const arch = await read("docs/ARCHITECTURE.md");
  const table = arch.slice(arch.indexOf("### 9.6."), arch.indexOf("### 9.7."));
  const documented = table
    .split("\n")
    .filter((l) => l.startsWith("| `"))
    .flatMap((l) => [...l.split("|")[1].matchAll(/`([a-z_]+)`/g)].map((m) => m[1]));

  const lib = await read("src-tauri/src/lib.rs");
  const handler = lib.slice(lib.indexOf("generate_handler!["), lib.indexOf("])", lib.indexOf("generate_handler![")));
  const registered = [...handler.matchAll(/(?:\w+::)+(\w+),/g)].map((m) => m[1]);

  const ts = await read("src/lib/ipc/commands.ts");
  const wrapped = [...ts.matchAll(/call<[^>]*>\("([a-z_]+)"/g)].map((m) => m[1]);

  const { MOCK_COMMANDS } = await importTs("src/lib/ipc/mock/index.ts");

  assert.equal(documented.length, 65, "ARCHITECTURE §9.6 lists 65 commands");
  assert.deepEqual(sorted(registered), sorted(documented), "lib.rs generate_handler! ≡ §9.6");
  assert.deepEqual(sorted(wrapped), sorted(documented), "commands.ts ≡ §9.6");
  assert.deepEqual(sorted(MOCK_COMMANDS), sorted(documented), "mock router ≡ §9.6");
  assert.equal(registered.length, new Set(registered).size, "no duplicate registration");
});

test("stub commands report their own name", async () => {
  for (const file of await rustFiles("src-tauri/src")) {
    if (!file.endsWith("commands.rs")) continue;
    const src = await read(file);
    for (const m of src.matchAll(/pub async fn (\w+)\([^)]*\)[^{]*\{\s*Err\("unimplemented: (\w+)"/g)) {
      assert.equal(m[2], m[1], `${file}: ${m[1]} returns "unimplemented: ${m[2]}"`);
    }
  }
});

// ── model.rs ≡ types.ts ─────────────────────────────────────────────────────

const camel = (s) => s.replace(/_([a-z])/g, (_, c) => c.toUpperCase());

function tsInterfaces(src) {
  const map = new Map();
  for (const m of src.matchAll(/export interface (\w+)(?: extends (\w+))? \{([\s\S]*?)\n\}/g)) {
    const fields = new Map();
    for (const line of m[3].split("\n")) {
      const f = line.match(/^\s*(\w+)(\?)?:/);
      if (f) fields.set(f[1], Boolean(f[2]));
    }
    map.set(m[1], { base: m[2] ?? null, fields });
  }
  for (const v of map.values()) {
    if (v.base) for (const [k, o] of map.get(v.base).fields) v.fields.set(k, o);
  }
  return map;
}

function tsUnions(src) {
  const map = new Map();
  for (const m of src.matchAll(/export type (\w+) = ((?:"[^"]+"\s*\|?\s*)+);/g)) {
    map.set(m[1], [...m[2].matchAll(/"([^"]+)"/g)].map((x) => x[1]));
  }
  return map;
}

function rustStructs(src) {
  const map = new Map();
  for (const m of src.matchAll(/pub struct (\w+)(?:<T>)? \{(\}|[\s\S]*?\n\})/g)) {
    const fields = new Map();
    const flatten = [];
    let attrs = "";
    for (const line of m[2].split("\n")) {
      const t = line.trim();
      if (t.startsWith("#[")) {
        attrs += t;
        continue;
      }
      const f = t.match(/^pub (\w+): /);
      if (f) {
        if (attrs.includes("flatten")) flatten.push(t.match(/: (\w+),/)[1]);
        else fields.set(camel(f[1]), attrs.includes("skip_serializing_if"));
        attrs = "";
      } else if (!t.startsWith("///")) {
        attrs = "";
      }
    }
    map.set(m[1], { fields, flatten });
  }
  for (const v of map.values()) {
    for (const base of v.flatten) for (const [k, o] of map.get(base).fields) v.fields.set(k, o);
  }
  return map;
}

function rustEnums(src) {
  const map = new Map();
  for (const m of src.matchAll(/((?:#\[[^\]]+\]\s*)*)pub enum (\w+) \{([\s\S]*?)\n\}/g)) {
    const rule = m[1].match(/rename_all = "(\w+)"/)?.[1];
    const variants = [];
    let rename = null;
    for (const line of m[3].split("\n")) {
      const t = line.trim();
      const r = t.match(/#\[serde\(rename = "([^"]+)"\)\]/);
      if (r) {
        rename = r[1];
        continue;
      }
      const v = t.match(/^([A-Z]\w*)(\(.*\))?,$/);
      if (!v) continue;
      if (v[2]) variants.push(null); // newtype variant (PasteMode::Format) — not a plain string
      else if (rename) variants.push(rename);
      else if (rule === "lowercase") variants.push(v[1].toLowerCase());
      else if (rule === "snake_case") variants.push(v[1].replace(/([a-z])([A-Z])/g, "$1_$2").toLowerCase());
      else variants.push(v[1]);
      rename = null;
    }
    map.set(m[2], variants);
  }
  return map;
}

test("model.rs structs ≡ types.ts interfaces (field names and optionality)", async () => {
  const ts = tsInterfaces(await read("src/lib/ipc/types.ts"));
  const rs = rustStructs(await read("src-tauri/src/model.rs"));
  const rustOnly = new Set(["PerKind", "PickerClose"]); // Record<Kind,T> and `{}` on the TS side
  let compared = 0;
  for (const [name, r] of rs) {
    if (rustOnly.has(name)) continue;
    compared++;
    const t = ts.get(name);
    assert.ok(t, `types.ts lacks interface ${name}`);
    assert.deepEqual(
      [...r.fields].sort(),
      [...t.fields].sort(),
      `${name}: [field, optional] pairs differ between model.rs and types.ts`,
    );
  }
  for (const name of ts.keys()) assert.ok(rs.has(name), `model.rs lacks struct ${name}`);
  assert.ok(compared >= 45, `compared only ${compared} structs`);
});

test("model.rs enums ≡ types.ts string unions", async () => {
  const ts = tsUnions(await read("src/lib/ipc/types.ts"));
  const rs = rustEnums(await read("src-tauri/src/model.rs"));
  let compared = 0;
  for (const [name, variants] of rs) {
    if (variants.includes(null) || !ts.has(name)) continue; // PasteMode / inline TS unions
    assert.deepEqual([...variants].sort(), [...ts.get(name)].sort(), `${name} variants differ`);
    compared++;
  }
  assert.ok(compared >= 20, `compared only ${compared} enums`);
});
