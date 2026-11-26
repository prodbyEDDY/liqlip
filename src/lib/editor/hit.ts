// Hit-testing, bounds and handles (SPEC §9.3 rules 6–7). Tolerances are in SCREEN px and
// converted with `scale` (screen px per source px), so a click feels the same at any zoom.
// Outlined shapes, lines and strokes are hit only on their contour (±6 screen px + half the
// stroke) — a small rectangle inside a big outlined one is clickable (LT defect П-11: filled
// bbox ±4 px). Filled shapes, text, counters, "hide" plates, images and spotlights are hit by
// area. Handles of small objects sit OUTSIDE the object so they never cover it; pen and marker
// strokes have no handles at all (move only). Pure, node-tested (tests/editorRules.test.mjs, rules 6–7).
import { COUNTER_R, MARKER_SCALE, strokePx, type EdObject, type Pt, type Rect } from "./doc";
import {
  arrowControl,
  arrowGeometry,
  arrowHead,
  arrowMid,
  boundsOf,
  dist,
  distPolyline,
  distRectEdge,
  ellipsePoints,
  growRect,
  inEllipse,
  inRect,
  quadPoints,
  rectsIntersect,
} from "./geometry";
import { textShape, type MeasureText } from "./textLayout";

export interface HitEnv {
  unit: number;
  measure: MeasureText;
  /** screen (CSS) px per source px */
  scale: number;
}

/** Contour tolerance, screen px (SPEC: ≥ 6). */
export const HIT_TOL = 6;
/** Drawn handle radius and its hit radius, screen px. */
export const HANDLE_R = 5;
export const HANDLE_HIT = 9;
/** Below this many screen px an object's handles move outside it (W7: 36 → 64 — on a 50 × 40
 *  rectangle eight 10 px handles hid most of its outline). */
export const SMALL = 64;
/** How far a handle steps out of a small object / a text box, screen px (its disc stays clear). */
const STEP = HANDLE_R * 2 + 2;

export type HandleId = "nw" | "n" | "ne" | "e" | "se" | "s" | "sw" | "w" | "a" | "b" | "mid";
export interface Handle {
  id: HandleId;
  pt: Pt;
}

/** Stroke width of an object in source px (0 for objects without one). */
export function lineWidth(o: EdObject, unit: number): number {
  switch (o.kind) {
    case "arrow":
    case "line":
    case "pen":
      return strokePx(o.size, unit);
    case "marker":
      return strokePx(o.size, unit) * MARKER_SCALE;
    case "rect":
    case "ellipse":
      return o.fill ? 0 : strokePx(o.size, unit);
    default:
      return 0;
  }
}

export function counterRadius(size: 1 | 2 | 3 | 4 | 5, unit: number): number {
  return COUNTER_R[size - 1] * unit;
}

/** The geometric box of an object (no stroke): the rect of box kinds, the text box, the
 *  counter's circle box, a path's bounds. */
export function shapeBox(o: EdObject, env: Pick<HitEnv, "unit" | "measure">): Rect {
  switch (o.kind) {
    case "arrow": {
      const c = arrowControl(o.a, o.b, o.bend);
      return boundsOf(quadPoints(o.a, c, o.b, 16));
    }
    case "line":
      return boundsOf([o.a, o.b]);
    case "pen":
    case "marker":
      return boundsOf(o.points);
    case "text": {
      const s = textShape(o, env.unit, env.measure);
      return { x: s.x0, y: s.y0, w: s.x1 - s.x0, h: s.y1 - s.y0 };
    }
    case "counter": {
      const r = counterRadius(o.size, env.unit);
      return { x: o.at[0] - r, y: o.at[1] - r, w: r * 2, h: r * 2 };
    }
    default:
      return o.r;
  }
}

/** Visual bounds (stroke and arrow heads included) — for cropping and dirty regions. */
export function bounds(o: EdObject, env: Pick<HitEnv, "unit" | "measure">): Rect {
  const w = lineWidth(o, env.unit);
  let box = shapeBox(o, env);
  if (o.kind === "arrow") {
    const head = arrowHead(arrowGeometry(o.a, o.b, o.bend, w));
    if (head) box = boundsOf([[box.x, box.y], [box.x + box.w, box.y + box.h], ...head]);
  }
  return growRect(box, w / 2);
}

/** Does `p` (source px) hit object `o`? */
export function hitObject(o: EdObject, p: Pt, env: HitEnv): boolean {
  const tol = HIT_TOL / env.scale;
  const half = lineWidth(o, env.unit) / 2;
  switch (o.kind) {
    case "arrow": {
      const c = arrowControl(o.a, o.b, o.bend);
      if (distPolyline(p, quadPoints(o.a, c, o.b)) <= tol + half) return true;
      const head = arrowHead(arrowGeometry(o.a, o.b, o.bend, half * 2));
      if (head && (distPolyline(p, [head[0], o.b, head[1]]) <= tol + half)) return true;
      if (head && o.style === "double") {
        const tail = arrowHead(arrowGeometry(o.b, o.a, -o.bend, half * 2));
        if (tail && distPolyline(p, [tail[0], o.a, tail[1]]) <= tol + half) return true;
      }
      return false;
    }
    case "line":
      return distPolyline(p, [o.a, o.b]) <= tol + half;
    case "pen":
    case "marker":
      return distPolyline(p, o.points) <= tol + half;
    case "rect":
      return o.fill ? inRect(p, o.r, tol / 2) : distRectEdge(p, o.r) <= tol + half;
    case "ellipse":
      return o.fill ? inEllipse(p, o.r, tol / 2) : distPolyline(p, ellipsePoints(o.r)) <= tol + half;
    case "counter":
      return dist(p, o.at) <= counterRadius(o.size, env.unit) + tol / 2;
    case "spotlight":
      return o.shape === "ellipse" ? inEllipse(p, o.r, tol / 2) : inRect(p, o.r, tol / 2);
    default:
      // text, redact, image — by area
      return inRect(p, shapeBox(o, env), tol / 2);
  }
}

/** The topmost object under `p` (array order = z-order), skipping `skip`. */
export function topHit(objects: readonly EdObject[], p: Pt, env: HitEnv, skip: string | null = null): EdObject | null {
  for (let i = objects.length - 1; i >= 0; i--) {
    const o = objects[i];
    if (o.id !== skip && hitObject(o, p, env)) return o;
  }
  return null;
}

/** Objects whose box meets the marquee rectangle. */
export function inMarquee(objects: readonly EdObject[], r: Rect, env: Pick<HitEnv, "unit" | "measure">): string[] {
  return objects.filter((o) => rectsIntersect(shapeBox(o, env), r)).map((o) => o.id);
}

const BOX_HANDLES: readonly [HandleId, number, number][] = [
  ["nw", 0, 0],
  ["n", 0.5, 0],
  ["ne", 1, 0],
  ["e", 1, 0.5],
  ["se", 1, 1],
  ["s", 0.5, 1],
  ["sw", 0, 1],
  ["w", 0, 0.5],
];

/**
 * 8 handles around `r`; when `r` is small on screen they step outside by a handle. A thin box
 * also moves its mid-side handles out (a one-line strip's side handles would sit on it).
 * `step: false` — `r` is already grown clear of the content (text): handles stay on it.
 */
function boxHandles(r: Rect, scale: number, only?: ReadonlySet<HandleId>, step = true): Handle[] {
  const out = STEP / scale;
  const smallX = step && r.w * scale < SMALL,
    smallY = step && r.h * scale < SMALL;
  return BOX_HANDLES.filter(([id]) => !only || only.has(id)).map(([id, fx, fy]) => {
    const ox = fx !== 0.5 && (smallX || (fy === 0.5 && smallY)) ? (fx === 0 ? -out : out) : 0;
    const oy = fy !== 0.5 && (smallY || (fx === 0.5 && smallX)) ? (fy === 0 ? -out : out) : 0;
    return { id, pt: [r.x + r.w * fx + ox, r.y + r.h * fy + oy] as Pt };
  });
}

const TEXT_HANDLES: ReadonlySet<HandleId> = new Set(["nw", "ne", "se", "sw", "e", "w"]);
/** Pictures scale with their proportions (SPEC §9.3): corners only. */
const CORNER_HANDLES: ReadonlySet<HandleId> = new Set(["nw", "ne", "se", "sw"]);

/** Handles of a single selected object (pen, marker, counter: none — move only). */
export function handlesOf(o: EdObject, env: HitEnv): Handle[] {
  switch (o.kind) {
    case "pen":
    case "marker":
    case "counter":
      return [];
    case "arrow":
    case "line": {
      const len = dist(o.a, o.b) * env.scale;
      let a = o.a,
        b = o.b;
      if (len < SMALL && len > 0) {
        // short: endpoints step outward along the line so the handles don't cover it
        const k = (HANDLE_R * 2 + 2) / env.scale / dist(o.a, o.b);
        const dx = (o.b[0] - o.a[0]) * k,
          dy = (o.b[1] - o.a[1]) * k;
        a = [o.a[0] - dx, o.a[1] - dy];
        b = [o.b[0] + dx, o.b[1] + dy];
      }
      const hs: Handle[] = [
        { id: "a", pt: a },
        { id: "b", pt: b },
      ];
      if (o.kind === "arrow") hs.push({ id: "mid", pt: arrowMid(o.a, o.b, o.bend) });
      return hs;
    }
    case "text":
      // always clear of the glyphs (the box has only a 0.1 em padding): a handle never covers a letter
      return boxHandles(growRect(shapeBox(o, env), STEP / env.scale), env.scale, TEXT_HANDLES, false);
    case "image":
      return boxHandles(o.r, env.scale, CORNER_HANDLES);
    default:
      return boxHandles(o.r, env.scale);
  }
}

/** The handle of `o` under `p`, if any (nearest wins). */
export function handleAt(o: EdObject, p: Pt, env: HitEnv): HandleId | null {
  let best: HandleId | null = null,
    bestD = HANDLE_HIT / env.scale;
  for (const h of handlesOf(o, env)) {
    const d = dist(p, h.pt);
    if (d <= bestD) {
      best = h.id;
      bestD = d;
    }
  }
  return best;
}

/** Handles of a crop rectangle (always outside the crop edge by half a handle). */
export function cropHandles(r: Rect, scale: number): Handle[] {
  return boxHandles(r, scale);
}

export function cropHandleAt(r: Rect, p: Pt, scale: number): HandleId | null {
  let best: HandleId | null = null,
    bestD = HANDLE_HIT / scale;
  for (const h of cropHandles(r, scale)) {
    const d = dist(p, h.pt);
    if (d <= bestD) {
      best = h.id;
      bestD = d;
    }
  }
  return best;
}

/** CSS cursor for a box handle. */
export function handleCursor(id: HandleId): string {
  switch (id) {
    case "nw":
    case "se":
      return "nwse-resize";
    case "ne":
    case "sw":
      return "nesw-resize";
    case "n":
    case "s":
      return "ns-resize";
    case "e":
    case "w":
      return "ew-resize";
    default:
      return "move";
  }
}
