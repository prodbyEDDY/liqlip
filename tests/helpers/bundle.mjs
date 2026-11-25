// Test helper (coordinator, W0): bundle TypeScript modules with esbuild and import them as
// ONE ES module, so node:test can exercise real `src/` code (no Svelte runes inside).
import { build } from "esbuild";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../..", import.meta.url));

/** Bundle one or more TS modules (paths relative to the repo root) into one module and
 *  import it: every export of every entry, sharing module state (one mock bus, etc.). */
export async function importTs(...entries) {
  const contents = entries.map((e) => `export * from ${JSON.stringify("./" + e)};`).join("\n");
  const out = await build({
    stdin: { contents, resolveDir: root, loader: "ts", sourcefile: "entry.ts" },
    absWorkingDir: root,
    bundle: true,
    write: false,
    format: "esm",
    platform: "neutral",
    target: "es2022",
    alias: { $lib: "./src/lib" },
    logLevel: "silent",
  });
  const code = out.outputFiles[0].text;
  return import("data:text/javascript;base64," + Buffer.from(code).toString("base64"));
}

export { root };
