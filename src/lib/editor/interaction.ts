// The editor's brain (SPEC §9.3 rules 1–12, ARCHITECTURE §18.6): a PURE dispatcher —
// `dispatch(state, event, env) → { state, handled, intent?, effect? }`. It knows nothing about
// the DOM: Editor.svelte turns pointer/keyboard events into `EdEvent`s (points in SOURCE px,
// keys by `KeyboardEvent.code`) and draws whatever state comes back. Every rule is a node test
// on this module (tests/editorRules.test.mjs).
//
// The model in one paragraph: a drawing tool ALWAYS starts a new object on a press — over,
// inside or on top of anything (rule 1), and stays active afterwards (rule 2). New objects are
// never selected (rule 3); instead the just-created object is remembered as `last` until the
// next press, a tool change or Esc, and a property change goes to it and to the tool's default
// (rule 4). Selecting is the Select tool's job, or Ctrl held with any tool (rule 5).
import { isBrowserShortcut, type BrowserKeyInput } from "$lib/browserKeys";
import {
  DRAW_TOOLS,
  FONT_SIZES,
  insertIndex,
  newId,
  type EdDoc,
  type EdObject,
  type ImageObj,
  type Pt,
  type Rect,
  type Size,
  type TextObj,
  type ToolId,
} from "./doc";
import { PRESETS, pushRecent, toolOfKind, type Defaults, type StyleTool, type ToolStyle } from "./defaults";
import { dist, inRect, rectFrom } from "./geometry";
import { cropHandleAt, handleAt, handleCursor, inMarquee, topHit, type HandleId, type HitEnv } from "./hit";
import { emptyHistory, record, redo as histRedo, undo as histUndo, type History, type Snap } from "./history";
import { textPadding, TEXT_LINE_HEIGHT, type MeasureText } from "./textLayout";
import { begin, CLICK_PX, finish, update, type CreateTool, type Mods } from "./tools/create";
import { dragCrop, fullRect, isFull, type CropGrip } from "./tools/crop";
import { applyProp, axisLock, duplicate, hasProp, moveObjects, remove, resizeObject, styleOf, zorder, type StyleProp, type ZDir } from "./tools/transform";

export type { Mods };
export type EditorMode = "overlay" | "window";
export type EditorIntent = "done" | "copy" | "save" | "save_as" | "pin" | "text" | "close";

/** What the dispatcher needs from the host component. */
export interface Env {
  mode: EditorMode;
  /** screen (CSS) px per source px */
  scale: number;
  measure: MeasureText;
  /** overlay: the frame — presses outside it are ignored, drawing is clamped into it */
  clip: Rect | null;
  /** eyedropper: the source colour at a point (`#rrggbb`) */
  sample?: (p: Pt) => string | null;
  /** the host listens to intents (Enter / Ctrl+C / Ctrl+S / Ctrl+P / Ctrl+W are then eaten) */
  intents: boolean;
}

export interface PointerIn {
  pt: Pt;
  button: number;
  mods: Mods;
  time: number;
}

export interface KeyIn {
  code: string;
  ctrl: boolean;
  shift: boolean;
  alt: boolean;
  repeat: boolean;
  /** IME composition in progress (`KeyboardEvent.isComposing`) */
  composing: boolean;
  time: number;
}

export type EdEvent =
  | ({ type: "down" } & PointerIn)
  | ({ type: "move" } & PointerIn)
  | ({ type: "up" } & PointerIn)
  | ({ type: "dbl" } & PointerIn)
  | { type: "cancel" }
  | ({ type: "key" } & KeyIn)
  | { type: "mods"; mods: Mods }
  | { type: "escape" }
  | { type: "tool"; tool: ToolId }
  /** `recent: false` — a live preview (the system colour picker while it is dragged): the colour
   *  is applied but doesn't enter the "recent" list (its final `change` does). Default true. */
  | { type: "prop"; prop: StyleProp; value: ToolStyle[StyleProp]; time: number; recent?: boolean }
  | { type: "undo" }
  | { type: "redo" }
  | { type: "text"; value: string }
  | { type: "textCommit" }
  | { type: "image"; src: string; box: Rect; time: number }
  | { type: "selectAll" }
  | { type: "delete"; time: number }
  | { type: "saved" };

export type Effect = { type: "view"; action: "fit" | "actual" | "in" | "out" } | { type: "cheatsheet" };

export interface Result {
  state: EdState;
  handled: boolean;
  intent?: EditorIntent;
  effect?: Effect;
}

type Gesture =
  | { kind: "create"; id: string; start: Pt; cur: Pt; travel: number; straight: boolean; before: Snap }
  | { kind: "textPlace"; start: Pt; cur: Pt; travel: number }
  | { kind: "move"; start: Pt; origin: EdObject[]; ids: string[]; pressed: string; travel: number; before: Snap }
  | { kind: "resize"; id: string; handle: HandleId; start: Pt; orig: EdObject; before: Snap }
  | { kind: "marquee"; start: Pt; cur: Pt; base: string[] }
  | { kind: "crop"; grip: CropGrip; start: Pt; orig: Rect };

export interface TextEdit {
  id: string;
  isNew: boolean;
  before: Snap;
}

export interface EdState {
  doc: EdDoc;
  tool: ToolId;
  /** the tool to return to after the eyedropper / crop */
  prevTool: ToolId;
  sel: string[];
  /** the just-created object (rule 4) — not a selection: nothing is drawn around it */
  last: string | null;
  hover: string | null;
  hoverHandle: HandleId | "inside" | null;
  pointer: Pt | null;
  mods: Mods;
  gesture: Gesture | null;
  text: TextEdit | null;
  /** pending crop rectangle while the crop tool is active */
  crop: Rect | null;
  hist: History;
  /** the snapshot that counts as "saved" (dirty = the document differs from it) */
  baseline: Snap;
  defaults: Defaults;
  digits: { id: string; buf: string; time: number } | null;
}

const NO_MODS: Mods = { shift: false, ctrl: false, alt: false };

export function createState(doc: EdDoc, defaults: Defaults, tool: ToolId = "arrow"): EdState {
  const snap = { objects: doc.objects, crop: doc.crop };
  return {
    doc,
    tool,
    prevTool: tool === "crop" || tool === "eyedropper" ? "arrow" : tool,
    sel: [],
    last: null,
    hover: null,
    hoverHandle: null,
    pointer: null,
    mods: NO_MODS,
    gesture: null,
    text: null,
    crop: null,
    hist: emptyHistory(),
    baseline: snap,
    defaults,
    digits: null,
  };
}

// ── queries (used by the UI and the tests) ───────────────────────────────────

export const snapOf = (doc: EdDoc): Snap => ({ objects: doc.objects, crop: doc.crop });

export function isDirty(s: EdState): boolean {
  return s.doc.objects !== s.baseline.objects || s.doc.crop !== s.baseline.crop || s.text !== null;
}

export function find(s: EdState, id: string | null): EdObject | undefined {
  return id === null ? undefined : s.doc.objects.find((o) => o.id === id);
}

/** Select mode: the Select tool, or Ctrl held with a drawing tool (rule 5). */
export function selecting(s: EdState, mods: Mods = s.mods): boolean {
  return s.tool === "select" || (mods.ctrl && s.tool !== "crop");
}

/** The style tool behind the current tool (the eyedropper borrows the previous one). */
export function styleTool(s: EdState): StyleTool | null {
  const t = s.tool === "eyedropper" ? s.prevTool : s.tool;
  return t === "select" || t === "crop" || t === "eyedropper" ? null : t;
}

/** Who a property change goes to (rule 4). */
export function propTarget(s: EdState): "selection" | "last" | "tool" | null {
  if (s.sel.length) return "selection";
  if (s.last && find(s, s.last)) return "last";
  return styleTool(s) ? "tool" : null;
}

function targets(s: EdState): EdObject[] {
  if (s.sel.length) return s.doc.objects.filter((o) => s.sel.includes(o.id));
  const l = find(s, s.last);
  return l ? [l] : [];
}

/** The style the properties bar shows (the target's own values). */
export function currentStyle(s: EdState): ToolStyle | null {
  const t = targets(s);
  if (t.length) {
    const kind = toolOfKind(t[0].kind);
    return styleOf(t[0], kind ? s.defaults.tools[kind] : s.defaults.tools.arrow);
  }
  const st = styleTool(s);
  return st ? s.defaults.tools[st] : null;
}

/** Properties that make sense for the current target. */
export function propsFor(s: EdState): StyleProp[] {
  const all: StyleProp[] = ["color", "size", "fill", "round", "arrowStyle", "textStyle", "redactMode", "spotShape"];
  const t = targets(s);
  if (t.length) return all.filter((p) => t.some((o) => hasProp(o, p)));
  const st = styleTool(s);
  if (!st) return [];
  if (st === "text") return ["color", "size", "textStyle"];
  const probe = begin(st, [0, 0], s.defaults.tools[st]);
  return all.filter((p) => hasProp(probe, p));
}

/** Objects whose geometry the live gesture is changing (the renderer may draw cheap previews of
 *  them — a blur is recomputed exactly once the gesture ends). */
export function liveIds(s: EdState): ReadonlySet<string> | null {
  const g = s.gesture;
  if (!g) return null;
  if (g.kind === "create" || g.kind === "resize") return new Set([g.id]);
  if (g.kind === "move") return new Set(g.ids);
  return null;
}

export function canUndo(s: EdState): boolean {
  return s.hist.past.length > 0 || s.gesture !== null;
}
export function canRedo(s: EdState): boolean {
  return s.hist.future.length > 0;
}

function hitEnv(s: EdState, env: Env): HitEnv {
  return { unit: s.doc.unit, measure: env.measure, scale: env.scale };
}

/** CSS cursor (rule 5): crosshair for drawing; move / resize over objects in select mode. */
export function cursorFor(s: EdState): string {
  const g = s.gesture;
  if (g) {
    if (g.kind === "move") return "move";
    if (g.kind === "resize") return handleCursor(g.handle);
    if (g.kind === "crop") return g.grip === "move" ? "move" : g.grip === "new" ? "crosshair" : handleCursor(g.grip);
    if (g.kind === "marquee") return "default";
    return s.tool === "text" ? "text" : "crosshair";
  }
  if (s.tool === "crop") {
    if (s.hoverHandle === "inside") return "move";
    return s.hoverHandle ? handleCursor(s.hoverHandle) : "crosshair";
  }
  if (selecting(s)) {
    if (s.hoverHandle && s.hoverHandle !== "inside") return handleCursor(s.hoverHandle);
    return s.hover ? "move" : "default";
  }
  return s.tool === "text" ? "text" : "crosshair";
}

// ── helpers ──────────────────────────────────────────────────────────────────

function withObjects(s: EdState, objects: EdObject[]): EdState {
  return { ...s, doc: { ...s.doc, objects } };
}

function replaceObj(objects: readonly EdObject[], o: EdObject): EdObject[] {
  return objects.map((x) => (x.id === o.id ? o : x));
}

function insertObj(objects: readonly EdObject[], o: EdObject): EdObject[] {
  const at = insertIndex(objects, o.kind);
  return [...objects.slice(0, at), o, ...objects.slice(at)];
}

function clampToClip(p: Pt, env: Env): Pt {
  const c = env.mode === "overlay" ? env.clip : null;
  if (!c) return p;
  return [Math.max(c.x, Math.min(c.x + c.w, p[0])), Math.max(c.y, Math.min(c.y + c.h, p[1]))];
}

function done(state: EdState, handled = true, extra: Partial<Result> = {}): Result {
  return { state, handled, ...extra };
}

/** Finish a text session: kept (one history step), or removed when empty (rule 8). */
function commitText(s: EdState): EdState {
  const te = s.text;
  if (!te) return s;
  const o = find(s, te.id) as TextObj | undefined;
  let next: EdState = { ...s, text: null };
  if (!o) return next;
  const empty = o.text.trim() === "";
  if (empty) {
    next = withObjects(next, remove(s.doc.objects, new Set([te.id])));
    if (!te.isNew) next = { ...next, hist: record(s.hist, te.before, null, 0) };
    return { ...next, sel: next.sel.filter((id) => id !== te.id), last: next.last === te.id ? null : next.last };
  }
  const orig = te.before.objects.find((x) => x.id === te.id);
  if (te.isNew || !orig || JSON.stringify(orig) !== JSON.stringify(o)) {
    next = { ...next, hist: record(s.hist, te.before, null, 0) };
    if (te.isNew) next = { ...next, last: te.id };
  }
  return next;
}

function startTextEdit(s: EdState, o: TextObj): EdState {
  return { ...s, text: { id: o.id, isNew: false, before: snapOf(s.doc) }, sel: [], gesture: null };
}

/** Undo the live gesture (rule 11: Ctrl+Z during a gesture; right click; Esc). */
function cancelGesture(s: EdState): EdState {
  const g = s.gesture;
  if (!g) return s;
  const base = { ...s, gesture: null };
  switch (g.kind) {
    case "create":
    case "move":
    case "resize":
      return { ...base, doc: { ...s.doc, objects: g.before.objects, crop: g.before.crop } };
    case "marquee":
      return { ...base, sel: g.base };
    case "crop":
      return { ...base, crop: g.orig };
    default:
      return base;
  }
}

function setTool(s: EdState, tool: ToolId, env: Env): EdState {
  if (tool === "crop" && env.mode !== "window") return s;
  let next = cancelGesture(commitText(s));
  if (next.tool === tool) {
    // `C` again inside the crop tool: back to the full picture
    if (tool === "crop") return { ...next, crop: fullRect(next.doc.w, next.doc.h) };
    return next;
  }
  const prevTool = next.tool === "crop" || next.tool === "eyedropper" ? next.prevTool : next.tool;
  next = { ...next, tool, prevTool, last: null, crop: null, hover: null, hoverHandle: null, digits: null };
  if (tool === "crop") next = { ...next, crop: next.doc.crop ?? fullRect(next.doc.w, next.doc.h), sel: [] };
  else if (tool !== "select" && tool !== "eyedropper") next = { ...next, sel: [] };
  return next;
}

/** Rule 4: a property change goes to the selection, else to the just-created object, and
 *  always becomes the default of the tool(s) involved. */
function setProp<P extends StyleProp>(s: EdState, prop: P, value: ToolStyle[P], time: number, recent = true): EdState {
  const tgt = targets(s);
  let next = s;
  if (tgt.length) {
    const before = snapOf(s.doc);
    const ids = new Set(tgt.map((o) => o.id));
    let changed = false;
    const objects = s.doc.objects.map((o) => {
      if (!ids.has(o.id)) return o;
      const n = applyProp(o, prop, value);
      if (n !== o) changed = true;
      return n;
    });
    if (changed) {
      next = withObjects(next, objects);
      next = { ...next, hist: record(s.hist, before, `prop:${prop}:${[...ids].join(",")}`, time) };
    }
  }
  // defaults: the kinds of the targets, or the current tool
  const tools = new Set<StyleTool>();
  for (const o of tgt) {
    const t = toolOfKind(o.kind);
    if (t) tools.add(t);
  }
  if (!s.sel.length) {
    const st = styleTool(s);
    if (st) tools.add(st);
  }
  return setDefaults(next, tools, prop, value, recent);
}

function setDefaults<P extends StyleProp>(s: EdState, tools: Iterable<StyleTool>, prop: P, value: ToolStyle[P], toRecent = true): EdState {
  const all = { ...s.defaults.tools };
  for (const t of tools) all[t] = { ...all[t], [prop]: value };
  const recent = prop === "color" && toRecent ? pushRecent(s.defaults.recent, value as string) : s.defaults.recent;
  return { ...s, defaults: { tools: all, recent } };
}

/** A property change while typing goes to the text being typed (part of its one undo step). */
function setTextProp<P extends StyleProp>(s: EdState, prop: P, value: ToolStyle[P], recent = true): EdState {
  const o = s.text ? find(s, s.text.id) : undefined;
  const next = o ? withObjects(s, replaceObj(s.doc.objects, applyProp(o, prop, value))) : s;
  return setDefaults(next, ["text"], prop, value, recent);
}

function cycle<T>(list: readonly T[], cur: T): T {
  return list[(list.indexOf(cur) + 1) % list.length];
}

function updateHover(s: EdState, p: Pt, mods: Mods, env: Env): EdState {
  if (s.tool === "crop" && s.crop) {
    const h = cropHandleAt(s.crop, p, env.scale);
    return { ...s, hover: null, hoverHandle: h ?? (inRect(p, s.crop) ? "inside" : null) };
  }
  if (!selecting(s, mods)) return s.hover || s.hoverHandle ? { ...s, hover: null, hoverHandle: null } : s;
  const he = hitEnv(s, env);
  const single = s.sel.length === 1 ? find(s, s.sel[0]) : undefined;
  const handle = single ? handleAt(single, p, he) : null;
  const hit = topHit(s.doc.objects, p, he, s.text?.id ?? null);
  return { ...s, hover: hit?.id ?? null, hoverHandle: handle };
}

// ── pointer ──────────────────────────────────────────────────────────────────

function onDown(s0: EdState, e: PointerIn, env: Env): Result {
  let s: EdState = { ...s0, pointer: e.pt, mods: e.mods };
  if (s.gesture) {
    // a second button during a gesture: right click cancels it (SPEC §9.2), others are ignored
    return done(e.button === 2 ? cancelGesture(s) : s);
  }
  if (e.button !== 0) return done(s0, false); // right click: nothing; middle: the host pans
  if (env.mode === "overlay" && env.clip && !inRect(e.pt, env.clip)) {
    return done(s.text ? commitText(s) : s0, !!s.text);
  }
  s = commitText(s);
  const p = e.pt;
  const he = hitEnv(s, env);

  if (s.tool === "crop") {
    const r = s.crop ?? fullRect(s.doc.w, s.doc.h);
    const h = cropHandleAt(r, p, env.scale);
    // inside the crop it moves — unless the crop is still the whole picture: then a drag draws it
    const grip: CropGrip = h ?? (inRect(p, r) && !isFull(r, s.doc.w, s.doc.h) ? "move" : "new");
    return done({ ...s, crop: r, gesture: { kind: "crop", grip, start: p, orig: r } });
  }

  if (s.tool === "eyedropper" && !e.mods.ctrl) {
    const c = env.sample?.(p) ?? null;
    let next: EdState = { ...s, tool: s.prevTool };
    if (c) next = setProp({ ...s, tool: "eyedropper" }, "color", c, e.time);
    return done({ ...next, tool: s.prevTool, hover: null, hoverHandle: null });
  }

  if (selecting(s, e.mods)) {
    const single = s.sel.length === 1 ? find(s, s.sel[0]) : undefined;
    const handle = single ? handleAt(single, p, he) : null;
    if (single && handle) {
      return done({ ...s, last: null, gesture: { kind: "resize", id: single.id, handle, start: p, orig: single, before: snapOf(s.doc) } });
    }
    const hit = topHit(s.doc.objects, p, he);
    if (hit) {
      let sel = s.sel;
      if (e.mods.shift) {
        if (sel.includes(hit.id)) return done({ ...s, last: null, sel: sel.filter((id) => id !== hit.id) });
        sel = [...sel, hit.id];
      } else if (!sel.includes(hit.id)) sel = [hit.id];
      const before = snapOf(s.doc);
      let objects = s.doc.objects;
      if (e.mods.alt) {
        // Alt-drag duplicates (rule 10): copies stay under the pointer and move, originals stay
        const d = duplicate(objects, new Set(sel), 0, 0);
        objects = d.objects;
        sel = d.ids;
      }
      const next = withObjects({ ...s, sel, last: null }, objects);
      return done({ ...next, gesture: { kind: "move", start: p, origin: objects, ids: sel, pressed: e.mods.alt ? sel[0] : hit.id, travel: 0, before } });
    }
    const base = e.mods.shift ? s.sel : [];
    return done({ ...s, sel: base, last: null, gesture: { kind: "marquee", start: p, cur: p, base } });
  }

  if (!DRAW_TOOLS.has(s.tool)) return done(s, false);

  // rule 1: a drawing tool never hit-tests existing objects — a press is a new object
  s = { ...s, sel: [], last: null, hover: null, hoverHandle: null, digits: null };
  const style = s.defaults.tools[s.tool as StyleTool];
  const at = clampToClip(p, env);
  if (s.tool === "text") {
    // the one exception (rule 8): a click with Text on existing text edits it
    const hit = topHit(
      s.doc.objects.filter((o) => o.kind === "text"),
      p,
      he,
    );
    if (hit && hit.kind === "text") return done(startTextEdit(s, hit));
    return done({ ...s, gesture: { kind: "textPlace", start: at, cur: at, travel: 0 } });
  }
  const draft = begin(s.tool as CreateTool, at, style);
  const before = snapOf(s.doc);
  const straight = e.mods.shift && (draft.kind === "pen" || draft.kind === "marker");
  const next = withObjects(s, insertObj(s.doc.objects, draft));
  return done({ ...next, gesture: { kind: "create", id: draft.id, start: at, cur: at, travel: 0, straight, before } });
}

function onMove(s0: EdState, e: PointerIn, env: Env): Result {
  const s: EdState = { ...s0, pointer: e.pt, mods: e.mods };
  const g = s.gesture;
  if (!g) return done(updateHover(s, e.pt, e.mods, env), false);
  const p = e.pt;
  switch (g.kind) {
    case "create": {
      const cur = clampToClip(p, env);
      const travel = Math.max(g.travel, dist(cur, g.start) * env.scale);
      const draft = find(s, g.id);
      if (!draft) return done({ ...s, gesture: null });
      const next = update(draft, g.start, cur, e.mods, { unit: s.doc.unit, scale: env.scale }, g.straight);
      return done({ ...withObjects(s, replaceObj(s.doc.objects, next)), gesture: { ...g, cur, travel } });
    }
    case "textPlace": {
      const cur = clampToClip(p, env);
      return done({ ...s, gesture: { ...g, cur, travel: Math.max(g.travel, dist(cur, g.start) * env.scale) } });
    }
    case "move": {
      const [dx, dy] = axisLock(p[0] - g.start[0], p[1] - g.start[1], e.mods.shift);
      const travel = Math.max(g.travel, Math.hypot(dx, dy) * env.scale);
      return done({ ...withObjects(s, moveObjects(g.origin, new Set(g.ids), dx, dy)), gesture: { ...g, travel } });
    }
    case "resize": {
      const next = resizeObject(g.orig, g.handle, g.start, p, e.mods, s.doc.unit, env.measure);
      return done(withObjects(s, replaceObj(s.doc.objects, next)));
    }
    case "marquee": {
      const r = rectFrom(g.start, p);
      const picked = inMarquee(s.doc.objects, r, hitEnv(s, env));
      return done({ ...s, sel: [...new Set([...g.base, ...picked])], gesture: { ...g, cur: p } });
    }
    case "crop": {
      const crop = dragCrop(g.orig, g.grip, g.start, p, s.doc.w, s.doc.h, e.mods, Math.max(1, 8 / env.scale));
      return done({ ...s, crop });
    }
  }
}

function onUp(s0: EdState, e: PointerIn, env: Env): Result {
  const r = finishGesture(s0, e, env);
  // what is under the pointer now (after a move / an Alt-copy the old hover outline was stale)
  return r.handled ? { ...r, state: updateHover(r.state, e.pt, e.mods, env) } : r;
}

function finishGesture(s0: EdState, e: PointerIn, env: Env): Result {
  const s: EdState = { ...s0, pointer: e.pt, mods: e.mods };
  const g = s.gesture;
  if (!g) return done(s, false);
  const base: EdState = { ...s, gesture: null };
  switch (g.kind) {
    case "create": {
      const draft = find(s, g.id);
      const kept = draft ? finish(draft, g.travel, { unit: s.doc.unit, scale: env.scale }, clampToClip(e.pt, env)) : null;
      if (!kept) return done(withObjects(base, g.before.objects));
      const next = withObjects(base, replaceObj(s.doc.objects, kept));
      return done({ ...next, last: kept.id, hist: record(s.hist, g.before, null, e.time) });
    }
    case "textPlace": {
      const style = s.defaults.tools.text;
      const fontSize = FONT_SIZES[style.size - 1];
      const px = fontSize * s.doc.unit;
      const pad = textPadding(style.textStyle === "badge" ? "badge" : "text", px);
      const block = g.travel >= CLICK_PX;
      const r = rectFrom(g.start, g.cur);
      const at: Pt = block ? [r.x, r.y] : [g.start[0] - pad.x, g.start[1] - (px * TEXT_LINE_HEIGHT) / 2 - pad.y];
      const width = block ? Math.max(r.w, pad.x * 2 + px) : null;
      const o: TextObj = { id: newId(), color: style.color, kind: "text", at, width, text: "", fontSize, style: style.textStyle };
      const before = snapOf(s.doc);
      const next = withObjects(base, insertObj(s.doc.objects, o));
      return done({ ...next, text: { id: o.id, isNew: true, before } });
    }
    case "move": {
      if (g.travel < 1) {
        // Alt-click without a drag: drop the copies again
        if (g.origin !== g.before.objects) return done({ ...withObjects(base, g.before.objects), sel: [] });
        // a click on one member of a multi-selection selects just it
        return done({ ...base, sel: !e.mods.shift && s.sel.length > 1 ? [g.pressed] : s.sel });
      }
      return done({ ...base, hist: record(s.hist, g.before, null, e.time) });
    }
    case "resize": {
      const changed = find(s, g.id) !== g.orig;
      return done(changed ? { ...base, hist: record(s.hist, g.before, null, e.time) } : base);
    }
    case "marquee":
      return done(base);
    case "crop":
      return done(base);
  }
}

function onDbl(s: EdState, e: PointerIn, env: Env): Result {
  if (s.text || s.gesture) return done(s, false);
  if (selecting(s, e.mods)) {
    const hit = topHit(s.doc.objects, e.pt, hitEnv(s, env));
    if (hit?.kind === "text") return done(startTextEdit(s, hit));
    if (!hit && env.mode === "overlay" && env.intents && s.tool === "select") return done(s, true, { intent: "done" });
    return done(s, false);
  }
  return done(s, s.tool === "text");
}

// ── keyboard ─────────────────────────────────────────────────────────────────

const TOOL_KEYS: Record<string, ToolId> = {
  KeyV: "select",
  KeyA: "arrow",
  KeyL: "line",
  KeyR: "rect",
  KeyE: "ellipse",
  KeyO: "ellipse",
  KeyP: "pen",
  KeyD: "pen",
  KeyH: "marker",
  KeyT: "text",
  KeyN: "counter",
  KeyB: "redact",
  KeyS: "spotlight",
  KeyC: "crop",
  KeyI: "eyedropper",
};

/** Shift + tool key: the tool's next variant. */
const VARIANTS: Partial<Record<ToolId, { prop: StyleProp; list: readonly string[] }>> = {
  arrow: { prop: "arrowStyle", list: ["straight", "curved", "double"] },
  text: { prop: "textStyle", list: ["plain", "badge"] },
  redact: { prop: "redactMode", list: ["solid", "pixelate", "blur"] },
  spotlight: { prop: "spotShape", list: ["rect", "ellipse"] },
};

const DIGIT = /^(?:Digit|Numpad)(\d)$/;

function intent(s: EdState, env: Env, i: EditorIntent): Result {
  return env.intents ? done(s, true, { intent: i }) : done(s, false);
}

function onKey(s: EdState, k: KeyIn, env: Env): Result {
  // rule 12: nothing reaches the editor while text is typed or an IME composes (the text
  // field stops its own keys; this is the safety net). Esc goes through escape().
  if (k.code === "Escape") return done(s, false);
  if (s.text || k.composing) return done(s, true);
  const ctrl = k.ctrl;
  if (s.gesture) {
    if (ctrl && k.code === "KeyZ") return done(cancelGesture(s));
    return done(s, true);
  }
  if (ctrl && !k.alt) {
    switch (k.code) {
      case "KeyZ":
        return done(k.shift ? doRedo(s) : doUndo(s));
      case "KeyY":
        return done(doRedo(s));
      case "KeyA":
        return done({ ...setTool(s, "select", env), sel: s.doc.objects.map((o) => o.id) });
      case "KeyD": {
        if (!s.sel.length) return done(s, true);
        const off = 10 * s.doc.unit;
        const before = snapOf(s.doc);
        const d = duplicate(s.doc.objects, new Set(s.sel), off, off);
        return done({ ...withObjects(s, d.objects), sel: d.ids, hist: record(s.hist, before, null, k.time) });
      }
      case "BracketRight":
      case "BracketLeft": {
        if (!s.sel.length) return done(s, true);
        const dir: ZDir = k.code === "BracketRight" ? (k.shift ? "top" : "up") : k.shift ? "bottom" : "down";
        const objects = zorder(s.doc.objects, new Set(s.sel), dir);
        const moved = objects.some((o, i) => o !== s.doc.objects[i]);
        return done(moved ? { ...withObjects(s, objects), hist: record(s.hist, snapOf(s.doc), null, k.time) } : s);
      }
      case "KeyC":
        return intent(s, env, "copy");
      case "KeyS":
        return intent(s, env, k.shift ? "save_as" : "save");
      case "KeyP":
        return intent(s, env, "pin");
      case "KeyW":
        return intent(s, env, "close");
      case "Digit0":
      case "Numpad0":
        return env.mode === "window" ? done(s, true, { effect: { type: "view", action: "fit" } }) : done(s, false);
      case "Digit1":
      case "Numpad1":
        return env.mode === "window" ? done(s, true, { effect: { type: "view", action: "actual" } }) : done(s, false);
      case "Equal":
      case "NumpadAdd":
        return env.mode === "window" ? done(s, true, { effect: { type: "view", action: "in" } }) : done(s, false);
      case "Minus":
      case "NumpadSubtract":
        return env.mode === "window" ? done(s, true, { effect: { type: "view", action: "out" } }) : done(s, false);
      default:
        return done(s, false); // Ctrl+V reaches the page as a `paste` event
    }
  }
  if (k.alt || ctrl) return done(s, false);

  const code = k.code;
  if (k.shift && code === "Slash") return done(s, true, { effect: { type: "cheatsheet" } });
  const digit = DIGIT.exec(code);
  if (digit) {
    const d = Number(digit[1]);
    if (k.shift && d >= 1 && d <= 8) return done(setProp(s, "color", PRESETS[d - 1], k.time));
    // a digit on one selected counter sets its number ("1", "2" typed quickly → 12)
    const one = s.sel.length === 1 ? find(s, s.sel[0]) : undefined;
    if (!k.shift && one?.kind === "counter") {
      const buf = s.digits && s.digits.id === one.id && k.time - s.digits.time < 1000 ? (s.digits.buf + d).slice(-3) : String(d);
      const n = Number(buf);
      const before = snapOf(s.doc);
      const next = withObjects(s, replaceObj(s.doc.objects, { ...one, n }));
      return done({ ...next, digits: { id: one.id, buf, time: k.time }, hist: record(s.hist, before, `num:${one.id}`, k.time) });
    }
    if (!k.shift && d >= 1 && d <= 5) return done(setProp(s, "size", d as Size, k.time));
    return done(s, false);
  }
  if (code === "BracketLeft" || code === "BracketRight") {
    const cur = currentStyle(s);
    if (!cur) return done(s, true);
    const size = Math.max(1, Math.min(5, cur.size + (code === "BracketRight" ? 1 : -1))) as Size;
    return done(size === cur.size ? s : setProp(s, "size", size, k.time));
  }
  if (code === "Enter" || code === "NumpadEnter") {
    if (s.tool === "crop") return done(applyCrop(s, k.time));
    const one = s.sel.length === 1 ? find(s, s.sel[0]) : undefined;
    if (one?.kind === "text") return done(startTextEdit(s, one));
    return intent(s, env, "done");
  }
  if (code === "Delete" || code === "Backspace") {
    if (!s.sel.length) return done(s, false);
    return done(deleteSelection(s, k.time));
  }
  if (code.startsWith("Arrow")) {
    if (!s.sel.length) return done(s, false);
    const step = k.shift ? 10 : 1;
    const dx = code === "ArrowLeft" ? -step : code === "ArrowRight" ? step : 0;
    const dy = code === "ArrowUp" ? -step : code === "ArrowDown" ? step : 0;
    const before = snapOf(s.doc);
    const next = withObjects(s, moveObjects(s.doc.objects, new Set(s.sel), dx, dy));
    return done({ ...next, hist: record(s.hist, before, `nudge:${s.sel.join(",")}`, k.time) });
  }
  if (code === "Space") return done(s, env.mode === "window");
  if (code === "KeyF") {
    const cur = currentStyle(s);
    if (!cur || !propsFor(s).includes("fill")) return done(s, false);
    return done(setProp(s, "fill", !cur.fill, k.time));
  }
  const tool = TOOL_KEYS[code];
  if (tool) {
    if (tool === "crop" && env.mode !== "window") return done(s, false);
    if (k.shift && VARIANTS[tool]) {
      const v = VARIANTS[tool]!;
      let next = s.tool === tool ? s : setTool(s, tool, env);
      const cur = currentStyle(next) ?? next.defaults.tools[tool as StyleTool];
      next = setProp(next, v.prop, cycle(v.list, cur[v.prop] as string) as ToolStyle[StyleProp], k.time);
      return done(next);
    }
    if (k.shift) return done(s, false);
    return done(setTool(s, tool, env));
  }
  return done(s, false);
}

function doUndo(s: EdState): EdState {
  if (s.gesture) return cancelGesture(s);
  const r = histUndo(s.hist, snapOf(s.doc));
  if (!r) return s;
  return restore(s, r.snap, r.history);
}

function doRedo(s: EdState): EdState {
  const r = histRedo(s.hist, snapOf(s.doc));
  if (!r) return s;
  return restore(s, r.snap, r.history);
}

function restore(s: EdState, snap: Snap, hist: History): EdState {
  const ids = new Set(snap.objects.map((o) => o.id));
  return {
    ...s,
    doc: { ...s.doc, objects: snap.objects, crop: snap.crop },
    hist,
    sel: s.sel.filter((id) => ids.has(id)),
    last: null,
    hover: null,
    hoverHandle: null,
    crop: s.tool === "crop" ? (snap.crop ?? fullRect(s.doc.w, s.doc.h)) : s.crop,
  };
}

function deleteSelection(s: EdState, time: number): EdState {
  const before = snapOf(s.doc);
  const next = withObjects(s, remove(s.doc.objects, new Set(s.sel)));
  return { ...next, sel: [], last: null, hover: null, hoverHandle: null, hist: record(s.hist, before, null, time) };
}

function applyCrop(s: EdState, time: number): EdState {
  const r = s.crop;
  const crop = !r || isFull(r, s.doc.w, s.doc.h) ? null : r;
  const same = crop === s.doc.crop || (crop && s.doc.crop && crop.x === s.doc.crop.x && crop.y === s.doc.crop.y && crop.w === s.doc.crop.w && crop.h === s.doc.crop.h);
  let next: EdState = { ...s, tool: s.prevTool, crop: null, hover: null, hoverHandle: null };
  if (!same) next = { ...next, doc: { ...s.doc, crop }, hist: record(s.hist, snapOf(s.doc), null, time) };
  return next;
}

/** One step of the Esc ladder (SPEC §9.2): gesture → text → crop → selection → Select tool.
 *  `handled: false` — nothing left to unwind (the host decides about closing). */
export function escapeStep(s: EdState): Result {
  if (s.gesture) return done(cancelGesture(s));
  if (s.text) return done(commitText(s));
  if (s.tool === "crop") return done({ ...s, tool: s.prevTool, crop: null, hoverHandle: null });
  if (s.sel.length) return done({ ...s, sel: [], last: null, digits: null });
  const cleared = { ...s, last: null };
  if (s.tool !== "select") return done({ ...cleared, tool: "select", hover: null, hoverHandle: null });
  return done(cleared, false);
}

// ── DOM helpers (pure; Editor.svelte and TextInput.svelte call them) ─────────

/**
 * A right button pressed WHILE another is held. Pointer Events fire no second `pointerdown` for
 * a chorded press — only a `pointermove` with `button === 2` and bit 2 in `buttons` — so the
 * "right click cancels the object being drawn" rule (SPEC §9.2) has to look at moves.
 */
export function secondaryPress(e: { button: number; buttons: number }): boolean {
  return e.button === 2 && (e.buttons & 2) !== 0;
}

/** What a key pressed in the text field means (rule 8, rule 12). */
export type TextKey = "ime" | "escape" | "commit" | "action" | "browser" | "type";

/**
 * Keys typed into a text object's field. `ime` — an IME is composing: the IME owns the key, Esc
 * included (it cancels the composition, not the text). `escape` — travels on to the host
 * (`escape()` finishes the text). `commit` — Ctrl+Enter. `action` — Ctrl+S / Ctrl+Shift+S /
 * Ctrl+P / Ctrl+W: finish the text, then the editor runs the shortcut as usual. `browser` — a
 * WebView2 accelerator (F5 / Ctrl+R reload the window and lose the drawing, Ctrl+P prints…):
 * swallowed. `type` — an ordinary key, it stays in the field.
 */
export function textKey(e: BrowserKeyInput & { isComposing: boolean; keyCode: number }): TextKey {
  if (e.isComposing || e.keyCode === 229) return "ime";
  if (e.code === "Escape") return "escape";
  const ctrl = (e.ctrlKey || e.metaKey) && !e.altKey;
  if (ctrl && (e.code === "Enter" || e.code === "NumpadEnter")) return "commit";
  if (ctrl && (e.code === "KeyS" || (!e.shiftKey && (e.code === "KeyP" || e.code === "KeyW")))) return "action";
  if (isBrowserShortcut(e, { find: true })) return "browser";
  return "type";
}

// ── the dispatcher ───────────────────────────────────────────────────────────

export function dispatch(s: EdState, e: EdEvent, env: Env): Result {
  switch (e.type) {
    case "down":
      return onDown(s, e, env);
    case "move":
      return onMove(s, e, env);
    case "up":
      return onUp(s, e, env);
    case "dbl":
      return onDbl(s, e, env);
    case "cancel":
      return done(cancelGesture(s));
    case "mods": {
      const next = { ...s, mods: e.mods };
      if (!s.pointer) return done(next, false);
      if (s.gesture) return onMove(next, { pt: s.pointer, button: 0, mods: e.mods, time: 0 }, env);
      return done(updateHover(next, s.pointer, e.mods, env), false);
    }
    case "key":
      return onKey(s, e, env);
    case "escape":
      return escapeStep(s);
    case "tool":
      return done(setTool(s, e.tool, env));
    case "prop":
      return done(s.text ? setTextProp(s, e.prop, e.value, e.recent ?? true) : setProp(s, e.prop, e.value, e.time, e.recent ?? true));
    case "undo":
      return done(doUndo(commitText(s)));
    case "redo":
      return done(doRedo(commitText(s)));
    case "text": {
      if (!s.text) return done(s, false);
      const o = find(s, s.text.id);
      if (!o || o.kind !== "text") return done(s, false);
      return done(withObjects(s, replaceObj(s.doc.objects, { ...o, text: e.value })));
    }
    case "textCommit":
      return done(commitText(s));
    case "image": {
      const s1 = cancelGesture(commitText(s));
      const o: ImageObj = { id: newId(), kind: "image", r: e.box, src: e.src };
      const before = snapOf(s1.doc);
      const next = withObjects(s1, insertObj(s1.doc.objects, o));
      return done({ ...next, sel: [], last: o.id, hist: record(s1.hist, before, null, e.time) });
    }
    case "selectAll":
      return done({ ...setTool(s, "select", env), sel: s.doc.objects.map((o) => o.id) });
    case "delete":
      return s.sel.length ? done(deleteSelection(s, e.time)) : done(s, false);
    case "saved":
      return done({ ...s, baseline: snapOf(s.doc) });
  }
}
