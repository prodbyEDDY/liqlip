// The Select tool's operations (SPEC §9.3 rules 4, 9, 10): move, resize by handle, duplicate,
// delete, z-order inside a fixed layer, and applying a style property to objects. Objects are
// never mutated — every function returns new objects / a new array. Pure.
import { FONT_SIZES, insertIndex, layerOf, newId, translate, type EdObject, type Pt, type Rect, type Size } from "../doc";
import type { ToolStyle } from "../defaults";
import { bendFromPoint, constrainEndpoint, CURVED_RATIO, dist } from "../geometry";
import type { HandleId } from "../hit";
import { fromShape, resizeText, textShape, type MeasureText } from "../textLayout";
import type { Mods } from "./create";

export type StyleProp = keyof ToolStyle;

/** Move the objects in `ids` by `(dx, dy)`. */
export function moveObjects(objects: readonly EdObject[], ids: ReadonlySet<string>, dx: number, dy: number): EdObject[] {
  return objects.map((o) => (ids.has(o.id) ? translate(o, dx, dy) : o));
}

/** Shift: lock a drag to its dominant axis. */
export function axisLock(dx: number, dy: number, shift: boolean): [number, number] {
  if (!shift) return [dx, dy];
  return Math.abs(dx) >= Math.abs(dy) ? [dx, 0] : [0, dy];
}

const TEXT_NODE: Partial<Record<HandleId, number>> = { nw: 0, ne: 2, e: 3, se: 4, sw: 6, w: 7 };

/** Resize a box by one of its 8 handles; `keep` — keep the aspect ratio (corners only). */
export function resizeBox(r: Rect, h: HandleId, dx: number, dy: number, keep: boolean): Rect {
  let x0 = r.x,
    y0 = r.y,
    x1 = r.x + r.w,
    y1 = r.y + r.h;
  if (h.includes("w")) x0 += dx;
  if (h.includes("e")) x1 += dx;
  if (h.includes("n")) y0 += dy;
  if (h.includes("s")) y1 += dy;
  if (keep && h.length === 2 && r.w > 0 && r.h > 0) {
    const ratio = r.w / r.h;
    const w = x1 - x0,
      hh = y1 - y0;
    // follow the dominant axis, derive the other; the opposite corner stays pinned
    if (Math.abs(w) / ratio >= Math.abs(hh)) {
      const nh = (Math.abs(w) / ratio) * Math.sign(hh || 1);
      if (h.includes("n")) y0 = y1 - nh;
      else y1 = y0 + nh;
    } else {
      const nw = Math.abs(hh) * ratio * Math.sign(w || 1);
      if (h.includes("w")) x0 = x1 - nw;
      else x1 = x0 + nw;
    }
  }
  return { x: Math.min(x0, x1), y: Math.min(y0, y1), w: Math.abs(x1 - x0), h: Math.abs(y1 - y0) };
}

/** Resize `orig` (the object at the press) by handle `h` with the pointer travelled from `start` to `cur`. */
export function resizeObject(orig: EdObject, h: HandleId, start: Pt, cur: Pt, mods: Mods, unit: number, measure: MeasureText): EdObject {
  const dx = cur[0] - start[0],
    dy = cur[1] - start[1];
  switch (orig.kind) {
    case "arrow":
    case "line": {
      if (h === "mid" && orig.kind === "arrow") return { ...orig, bend: bendFromPoint(orig.a, orig.b, cur) };
      if (h === "a") {
        const p = constrainEndpoint(orig.kind, orig.b[0], orig.b[1], orig.a[0] + dx, orig.a[1] + dy, mods.shift);
        return { ...orig, a: p };
      }
      if (h === "b") {
        const p = constrainEndpoint(orig.kind, orig.a[0], orig.a[1], orig.b[0] + dx, orig.b[1] + dy, mods.shift);
        return { ...orig, b: p };
      }
      return orig;
    }
    case "text": {
      const node = TEXT_NODE[h];
      if (node === undefined) return orig;
      return fromShape(orig, resizeText(textShape(orig, unit, measure), node, dx, dy, measure, unit), unit);
    }
    case "pen":
    case "marker":
    case "counter":
      return orig;
    case "image":
      // pictures keep their proportions; Shift frees them
      return { ...orig, r: resizeBox(orig.r, h, dx, dy, !mods.shift) };
    default:
      return { ...orig, r: resizeBox(orig.r, h, dx, dy, mods.shift) };
  }
}

/** Copies of `ids` (offset by `(dx, dy)`), each placed on top of its layer. */
export function duplicate(objects: readonly EdObject[], ids: ReadonlySet<string>, dx: number, dy: number): { objects: EdObject[]; ids: string[] } {
  const copies = objects.filter((o) => ids.has(o.id)).map((o) => ({ ...translate(o, dx, dy), id: newId() }) as EdObject);
  let out = objects.slice();
  for (const c of copies) {
    const at = insertIndex(out, c.kind);
    out = [...out.slice(0, at), c, ...out.slice(at)];
  }
  return { objects: out, ids: copies.map((c) => c.id) };
}

export function remove(objects: readonly EdObject[], ids: ReadonlySet<string>): EdObject[] {
  return objects.filter((o) => !ids.has(o.id));
}

export type ZDir = "up" | "down" | "top" | "bottom";

/**
 * Move the selected objects inside their own layer (rule 9). Counters are left where they are:
 * their numbers follow the array order, and reordering must never renumber them (LT П-12).
 */
export function zorder(objects: readonly EdObject[], ids: ReadonlySet<string>, dir: ZDir): EdObject[] {
  const sel = (o: EdObject) => ids.has(o.id) && o.kind !== "counter";
  const out = objects.slice();
  if (dir === "up") {
    for (let i = out.length - 2; i >= 0; i--) {
      if (sel(out[i]) && !sel(out[i + 1]) && layerOf(out[i + 1].kind) === layerOf(out[i].kind)) [out[i], out[i + 1]] = [out[i + 1], out[i]];
    }
    return out;
  }
  if (dir === "down") {
    for (let i = 1; i < out.length; i++) {
      if (sel(out[i]) && !sel(out[i - 1]) && layerOf(out[i - 1].kind) === layerOf(out[i].kind)) [out[i], out[i - 1]] = [out[i - 1], out[i]];
    }
    return out;
  }
  // top / bottom: regroup each layer with the selected objects at its end / start
  const result: EdObject[] = [];
  for (let layer = 0; layer <= 4; layer++) {
    const group = out.filter((o) => layerOf(o.kind) === layer);
    const picked = group.filter(sel),
      rest = group.filter((o) => !sel(o));
    result.push(...(dir === "top" ? [...rest, ...picked] : [...picked, ...rest]));
  }
  return result;
}

/** Does property `p` apply to objects of this kind? */
export function hasProp(o: EdObject, p: StyleProp): boolean {
  switch (p) {
    case "color":
      return o.kind !== "spotlight" && o.kind !== "image";
    case "size":
      return o.kind !== "spotlight" && o.kind !== "image";
    case "fill":
      return o.kind === "rect" || o.kind === "ellipse";
    case "round":
      return o.kind === "rect";
    case "arrowStyle":
      return o.kind === "arrow";
    case "textStyle":
      return o.kind === "text";
    case "redactMode":
      return o.kind === "redact";
    case "spotShape":
      return o.kind === "spotlight";
  }
}

/** Apply one property (returns the same object when the property doesn't apply or already has
 *  this value — so re-picking the current colour/size leaves no empty undo step and no "dirty"). */
export function applyProp<P extends StyleProp>(o: EdObject, p: P, v: ToolStyle[P]): EdObject {
  if (!hasProp(o, p) || propValue(o, p) === v) return o;
  switch (p) {
    case "color":
      return { ...o, color: v as string } as EdObject;
    case "size":
      if (o.kind === "text") return { ...o, fontSize: FONT_SIZES[(v as Size) - 1] };
      return { ...o, size: v as Size } as EdObject;
    case "fill":
      return { ...o, fill: v as boolean } as EdObject;
    case "round":
      return { ...o, round: v as boolean } as EdObject;
    case "arrowStyle": {
      if (o.kind !== "arrow") return o;
      const style = v as ToolStyle["arrowStyle"];
      const bend = style === "straight" ? 0 : style === "curved" && o.bend === 0 ? CURVED_RATIO * dist(o.a, o.b) : o.bend;
      return { ...o, style, bend };
    }
    case "textStyle":
      return { ...o, style: v as ToolStyle["textStyle"] } as EdObject;
    case "redactMode":
      return { ...o, mode: v as ToolStyle["redactMode"] } as EdObject;
    case "spotShape":
      return { ...o, shape: v as ToolStyle["spotShape"] } as EdObject;
  }
  return o;
}

/** The object's own value of `p` (`undefined` when it has none; a text's size is its slot only
 *  when its font size sits exactly on one). */
function propValue(o: EdObject, p: StyleProp): unknown {
  switch (p) {
    case "color":
      return "color" in o ? o.color : undefined;
    case "size":
      if (o.kind === "text") {
        const i = (FONT_SIZES as readonly number[]).indexOf(o.fontSize);
        return i >= 0 ? i + 1 : undefined;
      }
      return "size" in o ? o.size : undefined;
    case "fill":
      return o.kind === "rect" || o.kind === "ellipse" ? o.fill : undefined;
    case "round":
      return o.kind === "rect" ? o.round === true : undefined;
    case "arrowStyle":
      return o.kind === "arrow" ? o.style : undefined;
    case "textStyle":
      return o.kind === "text" ? o.style : undefined;
    case "redactMode":
      return o.kind === "redact" ? o.mode : undefined;
    case "spotShape":
      return o.kind === "spotlight" ? o.shape : undefined;
  }
}

/** The size slot (1…5) a text object's font size is closest to. */
export function textSizeSlot(fontSize: number): Size {
  let best = 0;
  for (let i = 1; i < FONT_SIZES.length; i++) if (Math.abs(FONT_SIZES[i] - fontSize) < Math.abs(FONT_SIZES[best] - fontSize)) best = i;
  return (best + 1) as Size;
}

/** The style an object shows in the properties bar (fields it doesn't have come from `base`). */
export function styleOf(o: EdObject, base: ToolStyle): ToolStyle {
  const s = { ...base };
  if ("color" in o) s.color = o.color;
  if (o.kind === "text") {
    s.size = textSizeSlot(o.fontSize);
    s.textStyle = o.style;
  } else if ("size" in o) s.size = o.size;
  if (o.kind === "rect" || o.kind === "ellipse") {
    s.fill = o.fill;
    s.round = o.round === true;
  }
  if (o.kind === "arrow") s.arrowStyle = o.style;
  if (o.kind === "redact") s.redactMode = o.mode;
  if (o.kind === "spotlight") s.spotShape = o.shape;
  return s;
}
