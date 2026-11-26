// Creating objects with the drawing tools (SPEC §9.3 table). Each tool is a tiny state
// machine driven by interaction.ts: `begin` on the press makes the draft, `update` follows the
// pointer, `finish` returns the object to keep or `null` (a click without a drag — < 4 screen
// px — creates nothing for arrow / line / boxes; rule 1). Pure.
import { MARKER_SCALE, newId, strokePx, type BoxObj, type EdObject, type Pt, type ToolId } from "../doc";
import type { ToolStyle } from "../defaults";
import { addPoint, constrainEndpoint, CURVED_RATIO, dist, dragBox, simplify, streamline } from "../geometry";

export interface Mods {
  shift: boolean;
  ctrl: boolean;
  alt: boolean;
}

export interface CreateEnv {
  unit: number;
  /** screen px per source px */
  scale: number;
}

/** Below this many screen px of travel a press is a click. */
export const CLICK_PX = 4;

/** Pen smoothing: how far each point lags toward the pointer (0 = raw). */
const STREAMLINE = 0.5;

export type CreateTool = Extract<ToolId, "arrow" | "line" | "rect" | "ellipse" | "pen" | "marker" | "counter" | "redact" | "spotlight">;

/** Draft at the press point. `straight` (Shift held at the start) — pen: a 45° line, marker: a horizontal one. */
export function begin(tool: CreateTool, p: Pt, style: ToolStyle): EdObject {
  const id = newId();
  const { color, size } = style;
  switch (tool) {
    case "arrow":
      return {
        id,
        color,
        kind: "arrow",
        a: p,
        b: p,
        bend: 0,
        style: style.arrowStyle,
        size,
      };
    case "line":
      return { id, color, kind: "line", a: p, b: p, size };
    case "rect":
    case "ellipse": {
      const o: BoxObj = { id, color, kind: tool, r: { x: p[0], y: p[1], w: 0, h: 0 }, fill: style.fill, size };
      if (tool === "rect" && style.round) o.round = true;
      return o;
    }
    case "pen":
    case "marker":
      return { id, color, kind: tool, points: [p], size };
    case "counter":
      return { id, color, kind: "counter", at: p, n: null, size };
    case "redact":
      return { id, color, kind: "redact", r: { x: p[0], y: p[1], w: 0, h: 0 }, mode: style.redactMode, size };
    case "spotlight":
      return { id, kind: "spotlight", r: { x: p[0], y: p[1], w: 0, h: 0 }, shape: style.spotShape };
  }
}

/** Follow the pointer. `straight` was decided at the press (Shift at the start of a stroke). */
export function update(draft: EdObject, start: Pt, cur: Pt, mods: Mods, env: CreateEnv, straight: boolean): EdObject {
  switch (draft.kind) {
    case "arrow":
    case "line": {
      const b = constrainEndpoint(draft.kind, start[0], start[1], cur[0], cur[1], mods.shift);
      // a "curved" arrow arches with its length (bend is in px)
      if (draft.kind === "arrow" && draft.style === "curved") return { ...draft, b, bend: CURVED_RATIO * dist(start, b) };
      return { ...draft, b };
    }
    case "rect":
    case "ellipse":
    case "redact":
    case "spotlight":
      return { ...draft, r: dragBox(start, cur, mods.shift, mods.alt) };
    case "pen":
    case "marker": {
      if (straight) {
        const end: Pt = draft.kind === "marker" ? [cur[0], start[1]] : constrainEndpoint("pen", start[0], start[1], cur[0], cur[1], true);
        return { ...draft, points: [start, end] };
      }
      const last = draft.points[draft.points.length - 1];
      const p = streamline(last, cur, STREAMLINE);
      return { ...draft, points: addPoint(draft.points, p, 1.5 / env.scale) };
    }
    case "counter":
      return { ...draft, at: cur };
    default:
      return draft;
  }
}

/** The object to keep, or `null` to discard the draft. `travel` = max pointer travel, screen px. */
export function finish(draft: EdObject, travel: number, env: CreateEnv, cur?: Pt): EdObject | null {
  switch (draft.kind) {
    case "arrow":
    case "line":
      return travel < CLICK_PX || dist(draft.a, draft.b) * env.scale < CLICK_PX ? null : draft;
    case "rect":
    case "ellipse":
    case "redact":
    case "spotlight":
      if (travel < CLICK_PX) return null;
      return draft.r.w * env.scale < 1 && draft.r.h * env.scale < 1 ? null : draft;
    case "pen":
    case "marker": {
      // a marker click leaves nothing: its flat caps make a zero-length stroke invisible, and an
      // invisible object would still count (dirty, "close without saving?", hit-testable). A pen
      // click is a dot (round caps) — kept.
      if (draft.kind === "marker" && travel < CLICK_PX) return null;
      let points = draft.points;
      // the streamlined tail lags behind the pointer: end where the pointer was released
      if (cur && points.length > 1 && travel >= CLICK_PX) points = [...points, cur];
      const w = strokePx(draft.size, env.unit) * (draft.kind === "marker" ? MARKER_SCALE : 1);
      points = simplify(points, Math.min(w / 4, 0.75 / env.scale));
      return { ...draft, points };
    }
    default:
      return draft;
  }
}
