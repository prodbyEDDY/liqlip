// Mock of the screenshot commands (W5-A; ARCHITECTURE §18.10 "Мок") — the browser stand-in for
// W6-A (capture, commit, files) and W6-B (windows, status), so the capture overlay, the editor,
// the QAO and the pins can be built in a plain browser (`pnpm dev`):
//   /capture.html            the overlay: `shot_session` always has a session (a new one after
//                            the previous ended) with a fake desktop frame
//   /editor.html?clip=<id>   the editor on a record (no `clip` — the last edited, else newest picture)
//   /shots.html              the QAO: `shotCapture("screen")` adds a shot (`shot:added`)
//   /pin.html?clip=<id>      a pinned picture
//
// The frame is drawn in the page (canvas → PNG → `blob:` URL, same origin, so a canvas that
// draws it is never tainted): a 1920×1080 desktop (× `?mock-scale=`) with five windows — title
// bars, text lines, one window with two child panes — and `session.windows` lists exactly what
// is drawn, top-most first, in physical px of the frame. `shot_commit` parses the body with the
// same framing as Rust (`../frame.ts`), adds a `screenshot` record to the mock feed (picture:
// the flattened RGBA, or the frame cropped to `rect` when there are no pixels) and emits
// `clips:changed` + `shot:added` + `shot:end`. Without a DOM (node tests) pictures are SVG.
//
// URL flags (also listed in bus.ts): ?mock-shot=text (OCR session) · ?mock-shot-quick=1 (skip
// the frame stage) · ?mock-scale=1.5 (monitor scale) · ?mock-snipping=1 (Windows gives PrtSc
// to the Snipping Tool) · ?mock-shot-dialog=cancel (Save as… is cancelled).
//
// INT-5: `shots_pending` answers `[]` (the mock emits `shot:added` only to live listeners);
// `shot_temp_copy` / `shot_temp_save_as` accept the bare file names the mock hands out in
// `tempPath`; the editor commit follows Rust (`done` without pixels = nothing changed, `pin`
// with pixels applies the edit first).
import type {
  AnnotDoc,
  Clip,
  ClipsChanged,
  HudShow,
  MonitorFrame,
  PinPayload,
  Rect,
  ShotAction,
  ShotAdded,
  ShotCommit,
  ShotCommitResult,
  ShotEditorAction,
  ShotEditorCommit,
  ShotEditorPayload,
  ShotEnd,
  ShotMode,
  ShotSession,
  ShotSessionMode,
  ShotStage,
  ShotStatus,
  WinRect,
} from "../types";
import { isImageKind } from "../types";
import { parseFrameBody } from "../frame";
import { fail, flags, mockEmit, now, rng } from "./bus";
import { db, toClip, type Args } from "./clips";
import { makeClip, type MockClip } from "./seed";
import { currentSettings } from "./settings";

function param(name: string): string | null {
  if (typeof location === "undefined") return null;
  return new URLSearchParams(location.search).get(name);
}

/** Read per call, so one page can be reloaded with other flags. */
function shotFlags() {
  const scale = Number(param("mock-scale") ?? 1);
  return {
    text: param("mock-shot") === "text",
    quick: param("mock-shot-quick") === "1",
    scale: Number.isFinite(scale) ? Math.min(4, Math.max(1, scale)) : 1,
    snipping: param("mock-snipping") === "1",
    dialogCancel: param("mock-shot-dialog") === "cancel",
  };
}

const MONITOR_ID = "\\\\.\\DISPLAY1";
/** Seconds of the delayed capture are short outside a browser (node tests). */
const SECOND_MS = flags.latency ? 1000 : 1;

// ── the fake desktop ─────────────────────────────────────────────────────────

/** One drawn window: its rectangle (logical px of a 1920×1080 desktop), title, panes. */
interface FakeWindow {
  title: string;
  r: Rect;
  /** child panes (rectangles of the same desktop) — the browser window's sidebar + page */
  panes?: Rect[];
}

/** Top-most first (the order of `session.windows`, like `EnumWindows`). */
const FAKE_WINDOWS: FakeWindow[] = [
  { title: "Notes", r: { x: 1290, y: 110, w: 520, h: 380 } },
  {
    title: "Browser — liqlip.example",
    r: { x: 250, y: 170, w: 1120, h: 730 },
    panes: [
      { x: 250, y: 210, w: 270, h: 690 },
      { x: 520, y: 210, w: 850, h: 690 },
    ],
  },
  { title: "Terminal", r: { x: 110, y: 610, w: 760, h: 380 } },
  { title: "Files", r: { x: 990, y: 560, w: 780, h: 420 } },
  { title: "Mail", r: { x: 60, y: 50, w: 660, h: 430 } },
];

/** Desktop size and its windows in physical px of the frame at `scale` (pure; node-tested). */
export function fakeDesktop(scale: number): { width: number; height: number; windows: WinRect[] } {
  const px = (r: Rect): WinRect => ({
    x: Math.round(r.x * scale),
    y: Math.round(r.y * scale),
    w: Math.round(r.w * scale),
    h: Math.round(r.h * scale),
    children: [],
  });
  return {
    width: Math.round(1920 * scale),
    height: Math.round(1080 * scale),
    windows: FAKE_WINDOWS.map((w) => ({ ...px(w.r), children: (w.panes ?? []).map(px) })),
  };
}

type Canvas = HTMLCanvasElement;
const hasDom = () => typeof document !== "undefined" && typeof document.createElement === "function";

function canvas(width: number, height: number): Canvas {
  const c = document.createElement("canvas");
  c.width = width;
  c.height = height;
  return c;
}

function blobUrl(c: Canvas): Promise<string> {
  return new Promise((resolve, reject) =>
    c.toBlob((b) => (b ? resolve(URL.createObjectURL(b)) : reject(fail("failed", "canvas.toBlob"))), "image/png"),
  );
}

/** Draw the desktop at `scale` (bottom window first). */
function drawDesktop(scale: number): Canvas {
  const { width, height } = fakeDesktop(scale);
  const c = canvas(width, height);
  const g = c.getContext("2d")!;
  g.scale(scale, scale);
  g.fillStyle = "#2f3237";
  g.fillRect(0, 0, 1920, 1080);
  g.fillStyle = "#1f2125";
  g.fillRect(0, 1032, 1920, 48); // taskbar (not a window of the list)
  const text = (s: string, x: number, y: number, size: number, color: string) => {
    g.fillStyle = color;
    g.font = `${size}px Inter, "Segoe UI", sans-serif`;
    g.fillText(s, x, y);
  };
  const lines = (r: Rect, n: number, seed: number) => {
    const rand = rng(seed);
    for (let i = 0; i < n; i++) {
      g.fillStyle = "#c9ccd1";
      g.fillRect(r.x + 20, r.y + 24 + i * 26, Math.max(40, (r.w - 40) * (0.45 + rand() * 0.5)), 10);
    }
  };
  [...FAKE_WINDOWS].reverse().forEach((w, i) => {
    const { r } = w;
    g.fillStyle = "#f4f5f7";
    g.fillRect(r.x, r.y, r.w, r.h);
    g.fillStyle = "#e1e3e7";
    g.fillRect(r.x, r.y, r.w, 40);
    text(w.title, r.x + 16, r.y + 26, 16, "#26282c");
    if (w.panes) {
      w.panes.forEach((p, j) => {
        g.fillStyle = j === 0 ? "#e9ebef" : "#ffffff";
        g.fillRect(p.x, p.y, p.w, p.h);
        lines(p, Math.floor((p.h - 40) / 26), 31 + j);
      });
      text("Liqlip — a keyboard-first clipboard manager", w.panes[1].x + 20, w.panes[1].y + 380, 22, "#26282c");
    } else {
      lines({ x: r.x, y: r.y + 40, w: r.w, h: r.h - 40 }, Math.floor((r.h - 80) / 26), 11 + i);
    }
  });
  return c;
}

/** The same desktop as an SVG data URL (no DOM: node tests). */
function desktopSvg(scale: number): string {
  const { width, height } = fakeDesktop(scale);
  const wins = [...FAKE_WINDOWS]
    .reverse()
    .map(({ r }) => `<rect x="${r.x}" y="${r.y}" width="${r.w}" height="${r.h}" fill="#f4f5f7"/><rect x="${r.x}" y="${r.y}" width="${r.w}" height="40" fill="#e1e3e7"/>`)
    .join("");
  return svgUrl(`<g transform="scale(${scale})"><rect width="1920" height="1080" fill="#2f3237"/>${wins}</g>`, width, height);
}

const svgUrl = (body: string, w: number, h: number) =>
  "data:image/svg+xml;charset=utf-8," +
  encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}">${body}</svg>`);

/** A neutral picture of a given size when there is no DOM. */
const placeholder = (w: number, h: number) => svgUrl(`<rect width="${w}" height="${h}" fill="#8a8f98"/>`, w, h);

/** The frozen frame of this page: drawn once per scale. */
let frame: { scale: number; url: string; canvas: Canvas | null } | null = null;

async function desktopFrame(scale: number): Promise<{ url: string; canvas: Canvas | null }> {
  if (frame?.scale === scale) return frame;
  if (frame?.canvas && frame.url.startsWith("blob:")) URL.revokeObjectURL(frame.url);
  if (!hasDom()) {
    frame = { scale, url: desktopSvg(scale), canvas: null };
  } else {
    const c = drawDesktop(scale);
    frame = { scale, url: await blobUrl(c), canvas: c };
  }
  return frame;
}

async function pictureFromRgba(rgba: Uint8Array, w: number, h: number): Promise<string> {
  if (!hasDom()) return placeholder(w, h);
  const c = canvas(w, h);
  c.getContext("2d")!.putImageData(new ImageData(new Uint8ClampedArray(rgba), w, h), 0, 0);
  return blobUrl(c);
}

async function pictureFromFrame(r: Rect): Promise<string> {
  const f = frame;
  if (!f?.canvas) return placeholder(r.w, r.h);
  const c = canvas(r.w, r.h);
  c.getContext("2d")!.drawImage(f.canvas, r.x, r.y, r.w, r.h, 0, 0, r.w, r.h);
  return blobUrl(c);
}

// ── session ──────────────────────────────────────────────────────────────────

const MODES: readonly ShotMode[] = ["region", "screen", "window", "repeat", "text"];
const ACTIONS: readonly ShotAction[] = ["done", "copy", "save", "save_as", "pin", "text"];
const EDITOR_ACTIONS: readonly ShotEditorAction[] = ["done", "copy", "save_as", "pin"];

let session: ShotSession | null = null;
let sessionSeq = 0;
let stage: ShotStage = "select";
/** `{ monitorId, rect }` of the last shot — `repeat` and `R` in the overlay. */
let lastRegion: Rect | null = null;
let shotSeq = 0;

function monitorOf(scale: number, url: string): MonitorFrame {
  const { width, height } = fakeDesktop(scale);
  return { index: 0, id: MONITOR_ID, x: 0, y: 0, width, height, scale, frameUrl: url };
}

async function openSession(mode: ShotSessionMode): Promise<ShotSession> {
  const f = shotFlags();
  const { url } = await desktopFrame(f.scale);
  const s = currentSettings().screenshot;
  session = {
    id: ++sessionSeq,
    mode,
    monitor: monitorOf(f.scale, url),
    windows: fakeDesktop(f.scale).windows,
    lastRegion: lastRegion ? { ...lastRegion } : null,
    quick: f.quick || s.quick,
    magnifier: s.magnifier,
    dimensions: s.dimensions,
  };
  stage = "select";
  mockEmit<ShotSession>("shot:session", session);
  return structuredClone(session);
}

function endSession(): void {
  if (!session) return;
  const sessionId = session.id;
  session = null;
  mockEmit<ShotEnd>("shot:end", { sessionId });
}

/** Round and clamp a rectangle to the frame (W6-A's `clamp_crop`); empty → invalid. */
export function clampRect(r: Rect, width: number, height: number): Rect {
  const ok = [r?.x, r?.y, r?.w, r?.h].every((v) => typeof v === "number" && Number.isFinite(v));
  if (!ok) throw fail("invalid", "rect");
  const x0 = Math.max(0, Math.min(width, Math.round(r.x)));
  const y0 = Math.max(0, Math.min(height, Math.round(r.y)));
  const x1 = Math.max(0, Math.min(width, Math.round(r.x + r.w)));
  const y1 = Math.max(0, Math.min(height, Math.round(r.y + r.h)));
  if (x1 - x0 < 1 || y1 - y0 < 1) throw fail("invalid", "empty region");
  return { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
}

const pad = (n: number) => String(n).padStart(2, "0");

/** The file name template (`settings.screenshot.fileName`) for a shot taken now. */
function fileName(): string {
  const d = new Date(now());
  const s = currentSettings().screenshot;
  const vars: Record<string, string> = {
    date: `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`,
    time: `${pad(d.getHours())}${pad(d.getMinutes())}${pad(d.getSeconds())}`, // HHmmss, as files.rs
    app: "Google Chrome",
    n: String(shotSeq),
  };
  return s.fileName.replace(/\{(date|time|app|n)\}/g, (_, k: string) => vars[k]);
}

function savedPath(name: string): string {
  const s = currentSettings().screenshot;
  const folder = s.folder || "C:\\Users\\me\\Pictures\\Liqlip";
  return `${folder}\\${name}.${s.format}`;
}

function emitChanged(change: ClipsChanged["change"], ids: number[], origin: ClipsChanged["origin"] = "user"): void {
  mockEmit<ClipsChanged>("clips:changed", { origin, change, ids });
}

function hud(p: HudShow): void {
  mockEmit<HudShow>("hud:show", p);
}

/** Recognised text of a region (`action: 'text'`). */
const OCR_REGION_TEXT = "Liqlip — a keyboard-first clipboard manager";

/** What Rust's commit does (§18.4), on the mock feed. */
async function finishShot(region: Rect, rgba: Uint8Array | null, size: { w: number; h: number }, action: ShotAction, doc: AnnotDoc | null): Promise<ShotCommitResult> {
  const f = shotFlags();
  const { width, height } = fakeDesktop(f.scale);
  const r = clampRect(region, width, height);
  lastRegion = r;
  if (action === "text") {
    endSession();
    hud({ kind: "text_copied" });
    return { clipId: null, savedPath: null, text: OCR_REGION_TEXT };
  }
  shotSeq++;
  const picture = rgba ? await pictureFromRgba(rgba, size.w, size.h) : await pictureFromFrame(r);
  const [w, h] = rgba ? [size.w, size.h] : [r.w, r.h];
  const st = currentSettings();
  const name = fileName();
  const save = action === "save" || action === "save_as" || (action === "done" && st.screenshot.save);
  const path = save ? savedPath(name) : null;
  let clip: Clip | null = null;
  if (st.storage.types.screenshot) {
    const d = db();
    const c: MockClip = makeClip(d.nextId++, "screenshot", rng(shotSeq), now(), d.apps[0]?.id ?? null);
    Object.assign(c, { width: w, height: h, thumbUrl: picture, imageUrl: picture, size: Math.round((w * h * 4) / 6) });
    c.meta = { shot: { savedPath: path, edited: !!doc && Array.isArray(doc.objects) && doc.objects.length > 0 } };
    if (doc) docs.set(c.id, structuredClone(doc));
    d.clips.unshift(c);
    clip = toClip(c);
    emitChanged("added", [c.id], "capture");
  }
  endSession();
  if (action === "pin") console.info("[mock] shot pinned", clip?.id ?? null);
  const after = st.screenshot.afterCapture;
  if (after === "overlay") {
    mockEmit<ShotAdded>("shot:added", {
      clip,
      savedPath: path,
      tempPath: clip ? null : `C:\\Users\\me\\AppData\\Local\\Temp\\Liqlip\\shots\\shot-${now()}-${shotSeq}.png`,
      monitor: MONITOR_ID,
      error: null,
    });
  } else if (after === "editor" && clip) {
    lastEdited = clip.id;
    console.info("[mock] open the editor on", clip.id);
  }
  return { clipId: clip?.id ?? null, savedPath: path, text: null };
}

async function capture({ mode, delay }: Args): Promise<void> {
  if (!MODES.includes(mode as ShotMode)) throw fail("invalid", `mode ${String(mode)}`);
  const secs = delay === undefined || delay === null ? 0 : Number(delay);
  if (!Number.isInteger(secs) || secs < 0 || secs > 60) throw fail("invalid", `delay ${String(delay)}`);
  const run = async () => {
    const m = mode as ShotMode;
    if (session) return; // a trigger while a session is open is ignored (§18.3)
    const f = shotFlags();
    const { width, height, windows } = fakeDesktop(f.scale);
    // INT-5: "copy text from screen" without OCR language packs → the HUD, no overlay (as Rust)
    if (m === "text" && param("mock-ocr") === "nolang") return hud({ kind: "text_unavailable" });
    if (m === "region" || m === "text" || (m === "repeat" && !lastRegion)) {
      await openSession(m === "text" ? "text" : "region");
      return;
    }
    await desktopFrame(f.scale);
    const r = m === "screen" ? { x: 0, y: 0, w: width, h: height } : m === "window" ? windows[0] : lastRegion!;
    await finishShot(r, null, { w: 0, h: 0 }, "done", null);
  };
  if (secs === 0) return run();
  for (let s = secs; s > 0; s--) setTimeout(() => hud({ kind: "countdown", text: String(s) }), (secs - s) * SECOND_MS);
  setTimeout(() => void run(), secs * SECOND_MS);
}

function body(args: Args | Uint8Array): Uint8Array {
  if (!(args instanceof Uint8Array)) throw fail("invalid", "expected a raw body (frameBody)");
  return args;
}

async function commit(args: Args): Promise<ShotCommitResult> {
  const { head, rgba } = parseFrameBody<ShotCommit>(body(args as Args | Uint8Array));
  if (!session || head.sessionId !== session.id) throw fail("not_found", `session ${String(head.sessionId)}`);
  if (!ACTIONS.includes(head.action)) throw fail("invalid", `action ${String(head.action)}`);
  if (session.mode === "text" && head.action !== "text") throw fail("invalid", "a text session commits `text`");
  return finishShot(head.rect, rgba, { w: head.width, h: head.height }, head.action, head.doc ?? null);
}

// ── editor, pins, files ──────────────────────────────────────────────────────

/** Annotation documents and base pictures of edited records (Rust: `shot_docs`, `media/base`). */
const docs = new Map<number, AnnotDoc>();
const bases = new Map<number, string>();
let lastEdited: number | null = null;

function picture(id: unknown): MockClip {
  const c = db().clips.find((x) => x.id === id && x.deletedAt === null);
  if (!c) throw fail("not_found", `clip ${String(id)}`);
  if (!isImageKind(c.kind)) throw fail("invalid", `clip ${String(id)} is not a picture`);
  return c;
}

/** `?clip=<id>` of this page, else `fallback`, else the newest picture. */
function pageClip(fallback: number | null): MockClip | null {
  const live = db().clips.filter((c) => c.deletedAt === null && isImageKind(c.kind));
  const want = Number(param("clip") ?? fallback ?? NaN);
  return live.find((c) => c.id === want) ?? (param("clip") ? null : (live.sort((a, b) => b.copiedAt - a.copiedAt)[0] ?? null));
}

function nameOf(c: MockClip): string {
  const saved = c.meta.shot?.savedPath;
  if (saved) return saved.split("\\").pop()!.replace(/\.(png|jpg)$/i, "");
  return c.title ?? `${c.kind === "screenshot" ? "Screenshot" : "Image"} ${c.width ?? 0}×${c.height ?? 0}`;
}

function editorGet(): ShotEditorPayload | null {
  const c = pageClip(lastEdited);
  if (!c) return null;
  return {
    clipId: c.id,
    name: nameOf(c),
    baseUrl: bases.get(c.id) ?? c.imageUrl ?? c.thumbUrl ?? placeholder(c.width ?? 1, c.height ?? 1),
    width: c.width ?? 0,
    height: c.height ?? 0,
    doc: docs.has(c.id) ? structuredClone(docs.get(c.id)!) : null,
    dirtyGuard: false,
  };
}

/** Rust's `editor::commit` (§18.9): `done` / `pin` WITH pixels apply the edit (pin then pins);
 *  `done` without pixels changes nothing; `copy` / `save_as` never touch the record. */
async function editorCommit(args: Args): Promise<Clip> {
  const { head, rgba } = parseFrameBody<ShotEditorCommit>(body(args as Args | Uint8Array));
  if (!EDITOR_ACTIONS.includes(head.action)) throw fail("invalid", `action ${String(head.action)}`);
  if (!head.doc || typeof head.doc !== "object") throw fail("invalid", "doc");
  const c = picture(head.clipId);
  if ((head.action === "done" || head.action === "pin") && rgba) {
    if (!bases.has(c.id) && c.imageUrl) bases.set(c.id, c.imageUrl); // first edit keeps the original
    const url = await pictureFromRgba(rgba, head.width, head.height);
    Object.assign(c, { imageUrl: url, thumbUrl: url, width: head.width, height: head.height });
    docs.set(c.id, structuredClone(head.doc));
    const edited = head.doc.objects.length > 0 || head.doc.crop !== null;
    c.meta = { ...c.meta, shot: { savedPath: c.meta.shot?.savedPath ?? null, edited } };
    emitChanged("updated", [c.id]);
  }
  if (head.action === "pin") console.info("[mock] shot pinned", c.id);
  if (head.action === "save_as" && !shotFlags().dialogCancel) savePicture(c, `C:\\Users\\me\\Documents\\${nameOf(c)}.png`);
  return toClip(c);
}

/** A temp file name the mock handed out (`shot-<ms>-<n>.png`); anything else is refused like Rust. */
function tempName(name: unknown): string {
  if (typeof name !== "string" || !/^[A-Za-z0-9_-][A-Za-z0-9._-]{0,127}$/.test(name)) throw fail("invalid", "temp file name");
  return name;
}

function savePicture(c: MockClip, path: string): string {
  c.meta = { ...c.meta, shot: { savedPath: path, edited: c.meta.shot?.edited ?? false } };
  emitChanged("updated", [c.id]);
  return path;
}

function finite(v: unknown, what: string, min: number): number {
  const n = Number(v);
  if (typeof v !== "number" || !Number.isFinite(n) || n < min) throw fail("invalid", what);
  return n;
}

// ── router table ─────────────────────────────────────────────────────────────

export const shotsHandlers: Record<string, (args: Args) => unknown> = {
  shot_capture: capture,
  shot_session: async () => (session ? structuredClone(session) : openSession(shotFlags().text ? "text" : "region")),
  shot_stage: ({ sessionId, stage: next }) => {
    if (next !== "select" && next !== "frame") throw fail("invalid", `stage ${String(next)}`);
    if (!session || sessionId !== session.id) throw fail("not_found", `session ${String(sessionId)}`);
    stage = next;
  },
  shot_commit: commit,
  shot_cancel: ({ sessionId }) => {
    if (session && sessionId === session.id) endSession();
  },
  shot_edit: ({ id }) => {
    lastEdited = picture(id).id;
    console.info("[mock] shot_edit", id);
  },
  shot_editor_get: editorGet,
  shot_editor_switch: ({ accept }) => {
    if (typeof accept !== "boolean") throw fail("invalid", "accept");
  },
  shot_editor_commit: editorCommit,
  shot_pin: ({ id }) => {
    console.info("[mock] shot_pin", picture(id).id);
  },
  shot_pin_get: (): PinPayload | null => {
    const c = pageClip(null);
    return c ? { clipId: c.id, imageUrl: c.imageUrl ?? c.thumbUrl ?? "", width: c.width ?? 0, height: c.height ?? 0 } : null;
  },
  shot_pin_resize: ({ scale }) => {
    if (finite(scale, "scale", 0) === 0) throw fail("invalid", "scale");
  },
  shot_pin_close: () => {},
  shot_save: ({ id }) => {
    const c = picture(id);
    shotSeq++;
    return savePicture(c, savedPath(nameOf(c)));
  },
  shot_save_as: ({ id }) => {
    const c = picture(id);
    if (shotFlags().dialogCancel) return null;
    return savePicture(c, `C:\\Users\\me\\Documents\\${nameOf(c)}.png`);
  },
  shot_reveal: ({ id }) => {
    const c = picture(id);
    if (!c.meta.shot?.savedPath) throw fail("not_found", `clip ${String(id)} has no saved file`);
    console.info("[mock] reveal", c.meta.shot.savedPath);
  },
  shots_hud_layout: ({ height }) => {
    finite(height, "height", 0);
  },
  shots_hud_hide: () => {},
  shots_pending: (): ShotAdded[] => [],
  shot_temp_copy: ({ name }) => {
    console.info("[mock] copy temp shot", tempName(name));
  },
  shot_temp_save_as: ({ name }) => (shotFlags().dialogCancel ? null : `C:\\Users\\me\\Documents\\${tempName(name)}`),
  shot_status: (): ShotStatus => {
    const snipping = shotFlags().snipping;
    const taken = snipping && currentSettings().hotkeys.shotRegion === "PrtSc";
    return { snippingKey: snipping, hotkeyErrors: taken ? [{ field: "hotkeys.shotRegion", code: "taken" }] : [] };
  },
  open_windows_settings: ({ page }) => {
    if (page !== "keyboard") throw fail("invalid", `page ${String(page)}`);
    console.info("[mock] open ms-settings:easeofaccess-keyboard");
  },
};

/** Test hook: the stage the overlay last reported. */
export const mockShotStage = (): ShotStage => stage;
