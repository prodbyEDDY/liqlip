#!/usr/bin/env node
// Third-party notices for "About Liqlip" (ARCHITECTURE §16):
//
//   node scripts/licenses.mjs        → public/licenses/THIRD-PARTY.txt
//
// Run it after any dependency change (Cargo.lock / pnpm-lock.yaml). It lists only what
// ships in the installer:
//   • Rust — `cargo metadata` for the Windows target, walking NORMAL dependencies from the
//     `liqlip` crate (build/dev dependencies and proc-macro crates run at compile time only
//     and are skipped, with everything reached only through them). `cargo metadata` unifies
//     features, so a few crates only `tauri-build` needs may still be listed — harmless;
//   • npm — `pnpm licenses list --json --prod` (dependencies, including peers such as the
//     Svelte runtime that bits-ui pulls in), plus build tools whose output is copied into the
//     bundle (`EMITTED_BY_BUILD`).
// Each package's own license file(s) are quoted; identical texts are printed once with the
// list of packages using them. The output is deterministic (sorted, no timestamps).

import { execFileSync, execSync } from "node:child_process";
import { readdirSync, readFileSync, realpathSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const OUT = join(root, "public", "licenses", "THIRD-PARTY.txt");
const TARGET = "x86_64-pc-windows-msvc";

/** Dev dependencies whose generated output ships: Tailwind's preflight is copied into the CSS. */
const EMITTED_BY_BUILD = ["tailwindcss"];

const LICENSE_FILE = /^(licen[cs]e|copying|notice|copyright|unlicense)/i;
/** For "A OR B" pick one alternative — the first of these that has its own file. */
const PREFERRED = ["mit", "zlib", "isc", "bsd", "apache", "unlicense", "boost", "bsl"];

function run(cmd, args) {
  return execFileSync(cmd, args, { cwd: root, encoding: "utf8", maxBuffer: 256 * 1024 * 1024 });
}

function licenseFiles(dir) {
  let names;
  try {
    names = readdirSync(dir);
  } catch {
    return [];
  }
  return names.filter((n) => LICENSE_FILE.test(n) && statSync(join(dir, n)).isFile()).sort();
}

/** The files to quote for an SPDX expression: one alternative of an OR, else all. */
function pickFiles(expr, files) {
  const notices = files.filter((f) => /^notice/i.test(f));
  const licenses = files.filter((f) => !/^notice/i.test(f));
  // Old crates write "MIT/Apache-2.0" for OR.
  if (/\bOR\b|\//.test(expr ?? "") && !/\bAND\b/.test(expr) && licenses.length > 1) {
    for (const p of PREFERRED) {
      const hit = licenses.find((f) => f.toLowerCase().includes(p));
      if (hit) return [hit, ...notices];
    }
  }
  return files;
}

// ── Rust ──────────────────────────────────────────────────────────────────────

function rustPackages() {
  const meta = JSON.parse(
    run("cargo", ["metadata", "--format-version", "1", "--locked", "--offline", "--filter-platform", TARGET, "--manifest-path", join(root, "src-tauri", "Cargo.toml")]),
  );
  const pkgs = new Map(meta.packages.map((p) => [p.id, p]));
  const nodes = new Map(meta.resolve.nodes.map((n) => [n.id, n]));
  const rootId = meta.resolve.root;
  const isProcMacro = (p) => p.targets.every((t) => t.kind.includes("proc-macro"));
  const seen = new Set();
  const stack = [rootId];
  while (stack.length) {
    const id = stack.pop();
    for (const dep of nodes.get(id).deps) {
      if (!dep.dep_kinds.some((k) => k.kind === null)) continue; // build/dev only
      if (seen.has(dep.pkg) || isProcMacro(pkgs.get(dep.pkg))) continue;
      seen.add(dep.pkg);
      stack.push(dep.pkg);
    }
  }
  return [...seen].map((id) => {
    const p = pkgs.get(id);
    const dir = dirname(p.manifest_path);
    const files = p.license_file ? [p.license_file] : pickFiles(p.license, licenseFiles(dir));
    return { eco: "rust", name: p.name, version: p.version, license: p.license ?? "(see file)", url: p.repository ?? p.homepage ?? "", dir, files };
  });
}

// ── npm ───────────────────────────────────────────────────────────────────────

function npmPackages() {
  // pnpm is a .cmd shim on Windows, which only a shell can start.
  const report = JSON.parse(execSync("pnpm licenses list --json --prod", { cwd: root, encoding: "utf8", maxBuffer: 256 * 1024 * 1024 }));
  const out = [];
  for (const entries of Object.values(report)) {
    for (const e of entries) {
      e.versions.forEach((version, i) => {
        const dir = e.paths[i];
        out.push({ eco: "npm", name: e.name, version, license: e.license, url: e.homepage ?? "", dir, files: pickFiles(e.license, licenseFiles(dir)) });
      });
    }
  }
  for (const name of EMITTED_BY_BUILD) {
    const dir = realpathSync(join(root, "node_modules", name));
    const pj = JSON.parse(readFileSync(join(dir, "package.json"), "utf8"));
    out.push({ eco: "npm", name, version: pj.version, license: pj.license, url: pj.homepage ?? "", dir, files: pickFiles(pj.license, licenseFiles(dir)) });
  }
  return out;
}

// ── Output ────────────────────────────────────────────────────────────────────

const normalize = (s) => s.replace(/\r\n?/g, "\n").replace(/[ \t]+\n/g, "\n").trim();

function main() {
  const all = [...rustPackages(), ...npmPackages()].sort(
    (a, b) => a.eco.localeCompare(b.eco) || a.name.localeCompare(b.name) || a.version.localeCompare(b.version, undefined, { numeric: true }),
  );
  const unique = all.filter((p, i) => i === 0 || p.eco !== all[i - 1].eco || p.name !== all[i - 1].name || p.version !== all[i - 1].version);

  // Identical texts are printed once.
  const texts = new Map(); // text → { n, users[] }
  const missing = [];
  for (const p of unique) {
    const label = `${p.name} ${p.version}`;
    if (p.files.length === 0) missing.push(p);
    for (const f of p.files) {
      const text = normalize(readFileSync(resolve(p.dir, f), "utf8"));
      if (!texts.has(text)) texts.set(text, { n: texts.size + 1, users: [] });
      texts.get(text).users.push(label);
    }
    p.refs = p.files.map((f) => texts.get(normalize(readFileSync(resolve(p.dir, f), "utf8"))).n);
  }

  const rust = unique.filter((p) => p.eco === "rust");
  const npm = unique.filter((p) => p.eco === "npm");
  const line = (p) => `  ${p.name} ${p.version} — ${p.license}${p.url ? ` — ${p.url}` : ""}${p.refs.length ? ` [text ${p.refs.join(", ")}]` : ""}`;
  const rule = "=".repeat(78);
  let s = "";
  s += "Liqlip — third-party software notices\n";
  s += "Generated by scripts/licenses.mjs from Cargo.lock and pnpm-lock.yaml. Do not edit by hand.\n\n";
  s += "Liqlip includes the open-source software listed below. Each component is used under\n";
  s += "its own license; for dual-licensed components the license quoted is the one Liqlip\n";
  s += "uses. Interface icons: Heroicons (MIT, see heroicons.txt).\n\n";
  s += `${rule}\nRust crates (${rust.length})\n${rule}\n${rust.map(line).join("\n")}\n\n`;
  s += `${rule}\nJavaScript packages (${npm.length})\n${rule}\n${npm.map(line).join("\n")}\n\n`;
  if (missing.length) {
    s += `${rule}\nPublished without a license file (license per package metadata)\n${rule}\n`;
    s += missing.map((p) => `  ${p.name} ${p.version} — ${p.license}${p.url ? ` — ${p.url}` : ""}`).join("\n") + "\n\n";
  }
  s += `${rule}\nLicense texts\n${rule}\n`;
  for (const [text, { n, users }] of texts) {
    s += `\n--- [text ${n}] used by: ${users.join(", ")}\n\n${text}\n`;
  }
  writeFileSync(OUT, s);
  console.log(`licenses: ${rust.length} crates + ${npm.length} npm packages, ${texts.size} distinct texts, ${missing.length} without a file → ${(s.length / 1024).toFixed(0)} KB`);
}

main();
