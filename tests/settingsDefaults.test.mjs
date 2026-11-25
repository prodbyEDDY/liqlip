// The browser mock's defaults must equal the Rust defaults. Both sides are tested against
// tests/fixtures/settings.default.json (Rust: settings::tests::defaults_match_fixture).
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { importTs } from "./helpers/bundle.mjs";

test("mock settings defaults ≡ fixture ≡ Rust defaults", async () => {
  const fixture = JSON.parse(await readFile(new URL("./fixtures/settings.default.json", import.meta.url), "utf8"));
  const { MOCK_DEFAULTS } = await importTs("src/lib/ipc/mock/settings.ts");
  assert.deepEqual(JSON.parse(JSON.stringify(MOCK_DEFAULTS)), fixture);
});
