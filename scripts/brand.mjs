#!/usr/bin/env node
// Liqlip brand assets — ONE generator for every native place the mark appears
// (ARCHITECTURE §16). Run after changing the geometry below:
//
//   node scripts/brand.mjs
//
// Writes:
//   design/logo.svg · design/app-icon.svg       sources (the mark / the squircle app tile)
//   design/logo-1024.png · design/app-icon-1024.png
//   public/brand/logo-light.svg · logo-dark.svg  the mark for the UI (light / dark theme)
//   src-tauri/icons/{icon.ico,icon.png,32x32.png,64x64.png,128x128.png,128x128@2x.png}
//                                                via `tauri icon` (v2.tauri.app/develop/icons)
//   src-tauri/icons/tray/<px>.png · <px>-paused.png   tray glyphs, black on transparent,
//                                                one per tray size (100 %…300 % scaling)
//   src-tauri/icons/nsis-sidebar.bmp             NSIS Welcome/Finish bitmap (164 × 314)
//
// Rasterising uses the project's own Tauri CLI (`tauri icon` renders SVG with resvg),
// so there are no extra dependencies. Everything is monochrome: no gradients, no glow.

import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { inflateSync } from "node:zlib";

const root = resolve(import.meta.dirname, "..");
const tauriCli = join(root, "node_modules", "@tauri-apps", "cli", "tauri.js");

// Colours: the design-system ink of each theme (src/app.css) and the dark paper for the
// app tile — the same pairing as the Liquid Tools app icon.
const INK_LIGHT = "#252525";
const INK_DARK = "#F4F5F7";
const TILE = "#171717";

// ── Path building ──────────────────────────────────────────────────────────────
// Corners: the UI's `squircle` is CSS `corner-shape: squircle` = superellipse(2), i.e. the
// quartic |x|^4 + |y|^4 = 1 inside the border-radius box. One cubic per corner whose
// handles sit at 9.09 % of the radius from the corner puts the midpoint exactly on that
// curve (B(½) = (4 + 3k) / 8 = 2^-¼ → k = 0.9091). Capsules use circles (design rule:
// capsules are plain `border-radius: 999px`, never squircle) → k = 0.5523.
const K_SQUIRCLE = 0.9091;
const K_CIRCLE = 0.5523;

const num = (v) => String(Math.round(v * 1000) / 1000);
const pt = ([x, y]) => `${num(x)} ${num(y)}`;

/** A closed contour: a start point and segments `{ to }` (line) or `{ c1, c2, to }`. */
function contourToPath({ start, segs }, reverse = false) {
  if (!reverse) {
    return `M${pt(start)}` + segs.map((s) => (s.c1 ? `C${pt(s.c1)} ${pt(s.c2)} ${pt(s.to)}` : `L${pt(s.to)}`)).join("") + "Z";
  }
  const points = [start, ...segs.map((s) => s.to)];
  let d = `M${pt(points[points.length - 1])}`;
  for (let i = segs.length - 1; i >= 0; i--) {
    const s = segs[i];
    d += s.c1 ? `C${pt(s.c2)} ${pt(s.c1)} ${pt(points[i])}` : `L${pt(points[i])}`;
  }
  return d + "Z";
}

/** Rounded rectangle, clockwise on screen (y down); `k` picks squircle or circle corners. */
function roundRect(x0, y0, x1, y1, r, k) {
  r = Math.max(0, Math.min(r, (x1 - x0) / 2, (y1 - y0) / 2));
  const h = r * (1 - k); // handle distance from the corner
  return {
    start: [x0 + r, y0],
    segs: [
      { to: [x1 - r, y0] },
      { c1: [x1 - h, y0], c2: [x1, y0 + h], to: [x1, y0 + r] },
      { to: [x1, y1 - r] },
      { c1: [x1, y1 - h], c2: [x1 - h, y1], to: [x1 - r, y1] },
      { to: [x0 + r, y1] },
      { c1: [x0 + h, y1], c2: [x0, y1 - h], to: [x0, y1 - r] },
      { to: [x0, y0 + r] },
      { c1: [x0, y0 + h], c2: [x0 + h, y0], to: [x0 + r, y0] },
    ],
  };
}

const capsule = (x0, y0, x1, y1) => roundRect(x0, y0, x1, y1, Math.min(x1 - x0, y1 - y0) / 2, K_CIRCLE);

/**
 * Liquid: the board's inner area (inset by the gap) under a wave of `halves` half-waves
 * (2 = one S, 3 = a ripple), clockwise. A half-wave is one cubic whose handles sit ±H off
 * the level at ⅓ and ⅔ of its width; its peak is ¾·H.
 */
function liquid(x0, x1, y1, r, { level, amp, halves }) {
  r = Math.max(0, r);
  const h = r * (1 - K_SQUIRCLE);
  const step = (x1 - x0) / halves;
  const H = amp / 0.75;
  const wave = [];
  for (let i = 0; i < halves; i++) {
    const a = x0 + i * step;
    const dy = i % 2 === 0 ? -H : H; // the first half-wave is a crest
    wave.push({ c1: [a + step / 3, level + dy], c2: [a + (2 * step) / 3, level + dy], to: [a + step, level] });
  }
  return {
    start: [x0, level],
    segs: [
      ...wave,
      { to: [x1, y1 - r] },
      { c1: [x1, y1 - h], c2: [x1 - h, y1], to: [x1 - r, y1] },
      { to: [x0 + r, y1] },
      { c1: [x0 + h, y1], c2: [x0, y1 - h], to: [x0, y1 - r] },
      { to: [x0, level] },
    ],
  };
}

/**
 * The mark: board ring (outer clockwise, inner reversed — nonzero fill) + liquid or the pause
 * pair as ONE path, so coincident edges cancel in the rasteriser instead of leaving seams;
 * the clip capsule straddles the top edge. With `clipGap` > 0 a capsule `clipGap` wider than
 * the clip is knocked out of the board, so the clip reads as a separate part (large sizes);
 * at tray sizes the clip merges into the board.
 *
 * g: { board: [x0, y0, x1, y1], r, stroke, gap, clip: [x0, y0, x1, y1], clipGap,
 *      wave: { level, amp, halves }, pause: { w, h, gap, cy } }
 */
function markParts(g, paused = false) {
  const [bx0, by0, bx1, by1] = g.board;
  const s = g.stroke;
  const body = [
    contourToPath(roundRect(bx0, by0, bx1, by1, g.r, K_SQUIRCLE)),
    contourToPath(roundRect(bx0 + s, by0 + s, bx1 - s, by1 - s, g.r - s, K_SQUIRCLE), true),
  ];
  if (!paused) {
    const inset = s + g.gap;
    body.push(contourToPath(liquid(bx0 + inset, bx1 - inset, by1 - inset, g.r - inset, g.wave)));
  } else {
    const { w, h, gap, cy } = g.pause;
    const cx = (bx0 + bx1) / 2;
    body.push(contourToPath(capsule(cx - gap / 2 - w, cy - h / 2, cx - gap / 2, cy + h / 2)));
    body.push(contourToPath(capsule(cx + gap / 2, cy - h / 2, cx + gap / 2 + w, cy + h / 2)));
  }
  const [cx0, cy0, cx1, cy1] = g.clip;
  const k = g.clipGap;
  return {
    body: body.join(""),
    clip: contourToPath(capsule(cx0, cy0, cx1, cy1)),
    knock: k > 0 ? contourToPath(capsule(cx0 - k, cy0 - k, cx1 + k, cy1 + k)) : null,
  };
}

/** SVG markup of the mark in its own units (`extent` × `extent`), optionally transformed. */
function markSvg(g, fill, { paused = false, extent, transform = "" } = {}) {
  const { body, clip, knock } = markParts(g, paused);
  const t = transform ? ` transform="${transform}"` : "";
  if (!knock) return `  <path fill="${fill}"${t} d="${body}${clip}"/>`;
  return (
    `  <g${t}>\n` +
    `    <mask id="liqlip-clip-gap" maskUnits="userSpaceOnUse" x="0" y="0" width="${extent}" height="${extent}">\n` +
    `      <rect width="${extent}" height="${extent}" fill="#fff"/>\n` +
    `      <path fill="#000" d="${knock}"/>\n` +
    `    </mask>\n` +
    `    <path fill="${fill}" mask="url(#liqlip-clip-gap)" d="${body}"/>\n` +
    `    <path fill="${fill}" d="${clip}"/>\n` +
    `  </g>`
  );
}

// ── Geometry ───────────────────────────────────────────────────────────────────
// The idea: liquid + clip. A clipboard — portrait squircle board, capsule clip — holding
// liquid with a ripple: the sibling of the Liquid Tools flask. Paused: the liquid gives way
// to a pause pair inside the same board.

/** Master mark on a 24-unit grid (glyph 15 × 20.5, centred). */
const MASTER = {
  board: [4.5, 3.75, 19.5, 22.25],
  r: 4,
  stroke: 2,
  gap: 1.5,
  clip: [8.25, 1.75, 15.75, 5.75],
  clipGap: 1,
  wave: { level: 13.25, amp: 0.45, halves: 4 },
  pause: { w: 2.5, h: 7, gap: 2.5, cy: 14.5 },
};

/**
 * Tray glyphs, hinted per pixel size: board edges and stroke land on whole (or half) pixels,
 * so the 100 % / 125 % / 150 % sizes stay crisp. Windows asks for SM_CXSMICON at the taskbar
 * DPI: 16 (100 %), 20 (125 %), 24 (150 %), 28 (175 %), 32 (200 %), 40 (250 %), 48 (300 %).
 * Small sizes: no gap between wall and liquid, a flat clip merged into the top edge, one
 * pronounced S-wave (a ripple would vanish at 16 px).
 */
const S_WAVE = (level, amp) => ({ level, amp, halves: 2 });
const TRAY = {
  16: { board: [3, 3, 13, 15], r: 2.5, stroke: 1, gap: 0, clip: [5, 2, 11, 5], clipGap: 0, wave: S_WAVE(9.5, 1), pause: { w: 2, h: 5, gap: 2, cy: 9.5 } },
  20: { board: [4, 4, 16, 19], r: 3, stroke: 1.5, gap: 0, clip: [6, 2.5, 14, 6], clipGap: 0, wave: S_WAVE(12, 1.2), pause: { w: 2, h: 6, gap: 2, cy: 12 } },
  24: { board: [4, 4, 20, 23], r: 3.75, stroke: 1.5, gap: 0, clip: [8, 2.5, 16, 7], clipGap: 0, wave: S_WAVE(14, 1.4), pause: { w: 3, h: 7, gap: 2, cy: 14.5 } },
  // 175 % and up: enough pixels for the full mark (gap, knocked-out clip, ripple).
  28: scaled(MASTER, 28 / 24),
  32: scaled(MASTER, 32 / 24),
  40: scaled(MASTER, 40 / 24),
  48: scaled(MASTER, 48 / 24),
};

/** The geometry `g` with every length multiplied by `k`. */
function scaled(g, k) {
  const m = (v) => (typeof v === "number" ? v * k : Array.isArray(v) ? v.map(m) : Object.fromEntries(Object.entries(v).map(([n, x]) => [n, n === "halves" ? x : m(x)])));
  return m(g);
}

// App tile: squircle over the whole canvas; the 24-unit mark box spans 68 % of it.
const TILE_RADIUS = 0.36; // of the tile side (squircle corners)
const MARK_BOX = 0.68;

// ── SVG documents ──────────────────────────────────────────────────────────────

const svgDoc = (size, viewBox, body) =>
  `<svg xmlns="http://www.w3.org/2000/svg" width="${size}" height="${size}" viewBox="${viewBox}">\n${body}\n</svg>\n`;

function logoSvg(fill, size = 1024) {
  return svgDoc(size, "0 0 24 24", markSvg(MASTER, fill, { extent: 24 }));
}

function appIconSvg() {
  const S = 1024;
  const tile = contourToPath(roundRect(0, 0, S, S, S * TILE_RADIUS, K_SQUIRCLE));
  const unit = (S * MARK_BOX) / 24;
  const off = (S - 24 * unit) / 2;
  return svgDoc(
    S,
    `0 0 ${S} ${S}`,
    `  <path fill="${TILE}" d="${tile}"/>\n` +
      markSvg(MASTER, INK_DARK, { extent: 24, transform: `translate(${num(off)} ${num(off)}) scale(${num(unit)})` }),
  );
}

function traySvg(px, paused) {
  return svgDoc(px, `0 0 ${px} ${px}`, markSvg(TRAY[px], "#000000", { paused, extent: px }));
}

/** NSIS Welcome/Finish sidebar: the app tile colour with the mark, laid out on a square
 *  canvas (tauri icon renders squares) and cropped to 164 × 314 afterwards. */
function sidebarSvg() {
  const W = 164;
  const H = 314;
  const unit = 72 / 24; // 72 px mark box
  const ox = (W - 24 * unit) / 2;
  const oy = (H - 24 * unit) / 2 - 24; // optically above centre
  return svgDoc(
    H,
    `0 0 ${H} ${H}`,
    `  <rect width="${W}" height="${H}" fill="${TILE}"/>\n` +
      markSvg(MASTER, INK_DARK, { extent: 24, transform: `translate(${num(ox)} ${num(oy)}) scale(${num(unit)})` }),
  );
}

// ── Rasterising (Tauri CLI → resvg) ────────────────────────────────────────────

function tauriIcon(input, args) {
  execFileSync(process.execPath, [tauriCli, "icon", input, ...args], { cwd: root, stdio: "pipe" });
}

/** Render `svg` to a `px` × `px` PNG and return the file path. */
function renderPng(tmp, name, svg, px) {
  const src = join(tmp, `${name}.svg`);
  const out = join(tmp, `${name}-out`);
  writeFileSync(src, svg);
  tauriIcon(src, ["--png", String(px), "-o", out]);
  return join(out, `${px}x${px}.png`);
}

// ── PNG decode / BMP encode (for the NSIS bitmap) ──────────────────────────────

/** Minimal PNG decoder: 8-bit RGBA, non-interlaced (what `tauri icon` writes). */
function decodePng(buf) {
  let pos = 8;
  let width = 0;
  let height = 0;
  const idat = [];
  while (pos < buf.length) {
    const len = buf.readUInt32BE(pos);
    const type = buf.toString("latin1", pos + 4, pos + 8);
    const data = buf.subarray(pos + 8, pos + 8 + len);
    if (type === "IHDR") {
      width = data.readUInt32BE(0);
      height = data.readUInt32BE(4);
      if (data[8] !== 8 || data[9] !== 6 || data[12] !== 0) throw new Error("decodePng: expected 8-bit RGBA, non-interlaced");
    } else if (type === "IDAT") idat.push(data);
    pos += 12 + len;
  }
  const raw = inflateSync(Buffer.concat(idat));
  const stride = width * 4;
  const px = Buffer.alloc(stride * height);
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (stride + 1)];
    const line = raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1));
    for (let x = 0; x < stride; x++) {
      const a = x >= 4 ? px[y * stride + x - 4] : 0;
      const b = y > 0 ? px[(y - 1) * stride + x] : 0;
      const c = x >= 4 && y > 0 ? px[(y - 1) * stride + x - 4] : 0;
      let v = line[x];
      if (filter === 1) v += a;
      else if (filter === 2) v += b;
      else if (filter === 3) v += (a + b) >> 1;
      else if (filter === 4) {
        const p = a + b - c;
        const pa = Math.abs(p - a);
        const pb = Math.abs(p - b);
        const pc = Math.abs(p - c);
        v += pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
      }
      px[y * stride + x] = v & 0xff;
    }
  }
  return { width, height, px };
}

/** 24-bit BMP of the top-left `w` × `h` of an RGBA image, flattened onto `bg`. */
function encodeBmp({ width, px }, w, h, bg) {
  const row = Math.ceil((w * 3) / 4) * 4;
  const out = Buffer.alloc(54 + row * h);
  out.write("BM", 0, "latin1");
  out.writeUInt32LE(out.length, 2);
  out.writeUInt32LE(54, 10);
  out.writeUInt32LE(40, 14); // BITMAPINFOHEADER
  out.writeInt32LE(w, 18);
  out.writeInt32LE(h, 22); // positive = bottom-up rows
  out.writeUInt16LE(1, 26);
  out.writeUInt16LE(24, 28);
  out.writeUInt32LE(row * h, 34);
  out.writeInt32LE(2835, 38); // 72 dpi
  out.writeInt32LE(2835, 42);
  for (let y = 0; y < h; y++) {
    const dst = 54 + (h - 1 - y) * row;
    for (let x = 0; x < w; x++) {
      const i = (y * width + x) * 4;
      const a = px[i + 3] / 255;
      const mix = (c, k) => Math.round(c * a + bg[k] * (1 - a));
      out[dst + x * 3] = mix(px[i + 2], 2);
      out[dst + x * 3 + 1] = mix(px[i + 1], 1);
      out[dst + x * 3 + 2] = mix(px[i], 0);
    }
  }
  return out;
}

const hexRgb = (hex) => [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));

// ── Main ───────────────────────────────────────────────────────────────────────

function main() {
  const tmp = mkdtempSync(join(tmpdir(), "liqlip-brand-"));
  const icons = join(root, "src-tauri", "icons");
  try {
    // Sources + UI exports.
    mkdirSync(join(root, "design"), { recursive: true });
    mkdirSync(join(root, "public", "brand"), { recursive: true });
    writeFileSync(join(root, "design", "logo.svg"), logoSvg("#000000"));
    writeFileSync(join(root, "design", "app-icon.svg"), appIconSvg());
    writeFileSync(join(root, "public", "brand", "logo-light.svg"), logoSvg(INK_LIGHT, 24));
    writeFileSync(join(root, "public", "brand", "logo-dark.svg"), logoSvg(INK_DARK, 24));
    copyFileSync(renderPng(tmp, "logo", logoSvg("#000000"), 1024), join(root, "design", "logo-1024.png"));
    copyFileSync(renderPng(tmp, "app", appIconSvg(), 1024), join(root, "design", "app-icon-1024.png"));

    // App icon set. `tauri icon` also emits macOS/AppX/mobile files — Liqlip is
    // Windows-only, so only the desktop PNGs and the ICO are kept.
    const set = join(tmp, "set");
    tauriIcon(join(root, "design", "app-icon.svg"), ["-o", set]);
    const keep = ["icon.ico", "icon.png", "32x32.png", "64x64.png", "128x128.png", "128x128@2x.png"];
    for (const f of keep) copyFileSync(join(set, f), join(icons, f));

    // Tray glyphs (black; tray.rs recolours them white for a dark taskbar).
    const trayDir = join(icons, "tray");
    rmSync(trayDir, { recursive: true, force: true });
    mkdirSync(trayDir);
    for (const px of Object.keys(TRAY).map(Number)) {
      copyFileSync(renderPng(tmp, `tray-${px}`, traySvg(px, false), px), join(trayDir, `${px}.png`));
      copyFileSync(renderPng(tmp, `tray-${px}-p`, traySvg(px, true), px), join(trayDir, `${px}-paused.png`));
    }

    // NSIS sidebar bitmap.
    const side = decodePng(readFileSync(renderPng(tmp, "sidebar", sidebarSvg(), 314)));
    writeFileSync(join(icons, "nsis-sidebar.bmp"), encodeBmp(side, 164, 314, hexRgb(TILE)));

    console.log(`brand: wrote design/, public/brand/, src-tauri/icons/ (${readdirSync(trayDir).length} tray glyphs)`);
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
}

main();
