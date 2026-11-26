// Drawing (ARCHITECTURE §18.6). `drawDoc` paints the annotation objects in array order (the
// array is kept in layer order: hide → spotlight → marker → shapes/lines/pen/text/pictures →
// counters); the SAME function draws the screen and the export, so the export is the preview by
// construction. `drawChrome` paints hover outlines, the selection and handles on a separate
// light canvas. Nothing here runs in a loop: the component calls it when state changes.
import { contrastInk, layoutText, paintText, textShape, type MeasureText } from "./textLayout";
import { counterNumbers, MARKER_ALPHA, MARKER_SCALE, strokePx, TEXT_FONT, TEXT_WEIGHT, type EdDoc, type EdObject, type Pt, type Rect } from "./doc";
import { applyEffect, blurRadius, effectMargin, pixelRect, type EffectMode } from "./effects";
import { arrowGeometry, arrowHead, paintArrow, tracePath } from "./geometry";
import { counterRadius, cropHandles, HANDLE_R, handlesOf, lineWidth, shapeBox, type HitEnv } from "./hit";
import type { View } from "./view";

type Ctx = CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D;
export type Source = ImageBitmap | HTMLCanvasElement | OffscreenCanvas | HTMLImageElement;

// ── caches ───────────────────────────────────────────────────────────────────

/** Effect tiles by (rect, mode, size, unit): computed once from the source pixels. Bounded by
 *  total area (a drag makes a new tile per move; 16 Mpx ≈ 64 MB at most), oldest out first. */
export class EffectCache {
  private map = new Map<string, OffscreenCanvas>();
  private area = 0;
  constructor(private budget = 16_000_000) {}

  get(base: Source, r: Rect, mode: EffectMode, size: 1 | 2 | 3 | 4 | 5, unit: number, bw: number, bh: number): { tile: OffscreenCanvas; at: Rect } | null {
    const pr = pixelRect(r);
    const x0 = Math.max(0, pr.x),
      y0 = Math.max(0, pr.y);
    const x1 = Math.min(bw, pr.x + pr.w),
      y1 = Math.min(bh, pr.y + pr.h);
    if (x1 <= x0 || y1 <= y0) return null;
    const at = { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
    const key = `${mode}|${size}|${unit}|${at.x},${at.y},${at.w},${at.h}`;
    let tile = this.map.get(key);
    if (!tile) {
      const m = effectMargin(mode, size, unit);
      const gx = Math.max(0, at.x - m),
        gy = Math.max(0, at.y - m);
      const gw = Math.min(bw, at.x + at.w + m) - gx,
        gh = Math.min(bh, at.y + at.h + m) - gy;
      const grab = new OffscreenCanvas(gw, gh);
      const gctx = grab.getContext("2d", { willReadFrequently: true })!;
      gctx.drawImage(base, -gx, -gy);
      const px = gctx.getImageData(0, 0, gw, gh);
      const out = applyEffect(mode, px, { x: at.x - gx, y: at.y - gy, w: at.w, h: at.h }, size, unit);
      tile = new OffscreenCanvas(at.w, at.h);
      tile.getContext("2d")!.putImageData(new ImageData(out as Uint8ClampedArray<ArrayBuffer>, at.w, at.h), 0, 0);
      this.area += at.w * at.h;
      while (this.area > this.budget && this.map.size) {
        const [k, old] = this.map.entries().next().value!;
        this.map.delete(k);
        this.area -= old.width * old.height;
      }
      this.map.set(key, tile);
    }
    return { tile, at };
  }

  clear(): void {
    this.map.clear();
    this.area = 0;
  }
}

/** Decoded pictures of `image` objects (data URLs). */
export class ImageCache {
  private map = new Map<string, ImageBitmap | "loading" | "error">();
  private waiters = new Map<string, Promise<void>>();
  constructor(private onReady: () => void = () => {}) {}

  get(src: string): ImageBitmap | null {
    const v = this.map.get(src);
    if (v === undefined) void this.load(src);
    return v instanceof ImageBitmap ? v : null;
  }

  load(src: string): Promise<void> {
    const w = this.waiters.get(src);
    if (w) return w;
    this.map.set(src, "loading");
    const p = (async () => {
      try {
        const img = new Image();
        img.src = src;
        await img.decode();
        this.map.set(src, await createImageBitmap(img));
      } catch {
        this.map.set(src, "error");
      }
      this.onReady();
    })();
    this.waiters.set(src, p);
    return p;
  }

  /** Wait until every picture of `doc` is decoded (the export needs them all). */
  async ready(doc: EdDoc): Promise<void> {
    await Promise.all(doc.objects.filter((o) => o.kind === "image").map((o) => this.load((o as { src: string }).src)));
  }
}

export interface RenderEnv {
  measure: MeasureText;
  base: Source;
  effects: EffectCache;
  images: ImageCache;
  /** the text being typed is drawn by the text field, not the canvas */
  skip: string | null;
  /** the area the spotlight dims (the whole picture, or the overlay's frame) */
  area: Rect;
  /** screen only: objects a live gesture is reshaping — a blur is previewed with the canvas
   *  filter (the exact one costs 100+ ms per move on a big area); the export never passes it */
  live?: ReadonlySet<string> | null;
}

/** σ of the canvas filter matching `blur()` (three box passes of radius round(r / 2)). */
export function blurSigma(size: 1 | 2 | 3 | 4 | 5, unit: number): number {
  const rad = Math.max(1, Math.round(blurRadius(size, unit) / 2));
  return Math.sqrt((3 * ((2 * rad + 1) ** 2 - 1)) / 12);
}

/** Live preview of a blur plate: the source around it, blurred by the GPU canvas filter. */
function blurPreview(ctx: Ctx, o: Extract<EdObject, { kind: "redact" }>, doc: EdDoc, base: Source): void {
  const sigma = blurSigma(o.size, doc.unit);
  const m = Math.ceil(sigma * 3);
  const pr = pixelRect(o.r);
  const gx = Math.max(0, pr.x - m),
    gy = Math.max(0, pr.y - m);
  const gw = Math.min(doc.w, pr.x + pr.w + m) - gx,
    gh = Math.min(doc.h, pr.y + pr.h + m) - gy;
  if (gw < 1 || gh < 1) return;
  const tile = new OffscreenCanvas(gw, gh);
  const t = tile.getContext("2d")!;
  t.filter = `blur(${sigma}px)`;
  t.drawImage(base, gx, gy, gw, gh, 0, 0, gw, gh);
  ctx.beginPath();
  ctx.rect(o.r.x, o.r.y, o.r.w, o.r.h);
  ctx.clip();
  ctx.drawImage(tile, gx, gy);
}

/** A canvas 2D `measureText`-backed text measure (Inter weight 600). */
export function makeMeasure(): MeasureText {
  const c = new OffscreenCanvas(1, 1).getContext("2d")!;
  let last = "";
  return (text, size, font) => {
    const f = `${TEXT_WEIGHT} ${size}px ${font}`;
    if (f !== last) {
      c.font = f;
      last = f;
    }
    return c.measureText(text).width;
  };
}

// ── documents ────────────────────────────────────────────────────────────────

function roundedRect(ctx: Ctx, r: Rect, rad: number): void {
  const k = Math.max(0, Math.min(rad, r.w / 2, r.h / 2));
  ctx.roundRect(r.x, r.y, r.w, r.h, k);
}

/** Rounded-rectangle radius: a little more than the stroke, never more than half a side. */
export function cornerRadius(size: 1 | 2 | 3 | 4 | 5, unit: number): number {
  return strokePx(size, unit) * 1.5 + 3 * unit;
}

function drawSpotlights(ctx: Ctx, spots: readonly Extract<EdObject, { kind: "spotlight" }>[], area: Rect): void {
  if (!spots.length) return;
  ctx.save();
  // clip = area minus each hole, intersected → area minus the UNION of the holes
  for (const s of spots) {
    ctx.beginPath();
    ctx.rect(area.x, area.y, area.w, area.h);
    if (s.shape === "ellipse") ctx.ellipse(s.r.x + s.r.w / 2, s.r.y + s.r.h / 2, s.r.w / 2, s.r.h / 2, 0, 0, Math.PI * 2);
    else ctx.rect(s.r.x, s.r.y, s.r.w, s.r.h);
    ctx.clip("evenodd");
  }
  ctx.fillStyle = "rgba(0, 0, 0, 0.55)";
  ctx.fillRect(area.x, area.y, area.w, area.h);
  ctx.restore();
}

function drawObject(ctx: Ctx, o: EdObject, doc: EdDoc, env: RenderEnv, numbers: Map<string, number>): void {
  const unit = doc.unit;
  ctx.save();
  switch (o.kind) {
    case "redact": {
      if (o.mode === "solid") {
        ctx.fillStyle = o.color;
        ctx.fillRect(o.r.x, o.r.y, o.r.w, o.r.h);
      } else if (o.mode === "blur" && env.live?.has(o.id)) {
        blurPreview(ctx, o, doc, env.base);
      } else {
        const fx = env.effects.get(env.base, o.r, o.mode, o.size, unit, doc.w, doc.h);
        if (fx) {
          ctx.beginPath();
          ctx.rect(o.r.x, o.r.y, o.r.w, o.r.h);
          ctx.clip();
          ctx.imageSmoothingEnabled = o.mode !== "pixelate";
          ctx.drawImage(fx.tile, fx.at.x, fx.at.y);
        }
      }
      break;
    }
    case "marker":
      ctx.globalCompositeOperation = "multiply";
      ctx.globalAlpha = MARKER_ALPHA;
      ctx.strokeStyle = o.color;
      ctx.lineWidth = strokePx(o.size, unit) * MARKER_SCALE;
      ctx.lineCap = "butt";
      ctx.lineJoin = "round";
      ctx.beginPath();
      tracePath(ctx, o.points);
      ctx.stroke();
      break;
    case "pen":
      ctx.strokeStyle = o.color;
      ctx.lineWidth = strokePx(o.size, unit);
      ctx.lineCap = "round";
      ctx.lineJoin = "round";
      ctx.beginPath();
      tracePath(ctx, o.points);
      ctx.stroke();
      break;
    case "line":
      ctx.strokeStyle = o.color;
      ctx.lineWidth = strokePx(o.size, unit);
      ctx.lineCap = "round";
      ctx.beginPath();
      ctx.moveTo(o.a[0], o.a[1]);
      ctx.lineTo(o.b[0], o.b[1]);
      ctx.stroke();
      break;
    case "arrow":
      ctx.strokeStyle = o.color;
      paintArrow(ctx, arrowGeometry(o.a, o.b, o.bend, strokePx(o.size, unit)), o.style === "double");
      break;
    case "rect":
    case "ellipse": {
      ctx.beginPath();
      if (o.kind === "ellipse") ctx.ellipse(o.r.x + o.r.w / 2, o.r.y + o.r.h / 2, o.r.w / 2, o.r.h / 2, 0, 0, Math.PI * 2);
      else if (o.round) roundedRect(ctx, o.r, cornerRadius(o.size, unit));
      else ctx.rect(o.r.x, o.r.y, o.r.w, o.r.h);
      if (o.fill) {
        ctx.fillStyle = o.color;
        ctx.fill();
      } else {
        ctx.strokeStyle = o.color;
        ctx.lineWidth = strokePx(o.size, unit);
        ctx.lineJoin = o.kind === "rect" && !o.round ? "miter" : "round";
        ctx.stroke();
      }
      break;
    }
    case "text": {
      if (o.id === env.skip) break;
      const s = textShape(o, unit, env.measure);
      if (o.style === "badge") {
        ctx.fillStyle = o.color;
        ctx.beginPath();
        roundedRect(ctx, { x: s.x0, y: s.y0, w: s.x1 - s.x0, h: s.y1 - s.y0 }, s.fontSize * 0.35);
        ctx.fill();
        paintText(ctx, s, contrastInk(o.color), env.measure, TEXT_WEIGHT);
      } else paintText(ctx, s, o.color, env.measure, TEXT_WEIGHT);
      break;
    }
    case "counter": {
      const r = counterRadius(o.size, unit);
      ctx.fillStyle = o.color;
      ctx.beginPath();
      ctx.arc(o.at[0], o.at[1], r, 0, Math.PI * 2);
      ctx.fill();
      const n = String(numbers.get(o.id) ?? 1);
      ctx.fillStyle = contrastInk(o.color);
      ctx.font = `700 ${r * (n.length > 2 ? 0.8 : n.length > 1 ? 0.95 : 1.1)}px ${TEXT_FONT}`;
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      const m = ctx.measureText(n);
      // optical centre: the ink box of the digits, not the em box
      const dy = (m.actualBoundingBoxAscent - m.actualBoundingBoxDescent) / 2;
      ctx.textBaseline = "alphabetic";
      ctx.fillText(n, o.at[0], o.at[1] + dy);
      break;
    }
    case "image": {
      const bmp = env.images.get(o.src);
      if (bmp) ctx.drawImage(bmp, o.r.x, o.r.y, o.r.w, o.r.h);
      break;
    }
    case "spotlight":
      break;
  }
  ctx.restore();
}

/** Paint every object of `doc` (source px under the current transform). */
export function drawDoc(ctx: Ctx, doc: EdDoc, env: RenderEnv): void {
  const numbers = counterNumbers(doc.objects);
  let spotsDone = false;
  for (const o of doc.objects) {
    if (!spotsDone && o.kind !== "redact") {
      drawSpotlights(
        ctx,
        doc.objects.filter((x): x is Extract<EdObject, { kind: "spotlight" }> => x.kind === "spotlight"),
        env.area,
      );
      spotsDone = true;
    }
    drawObject(ctx, o, doc, env, numbers);
  }
}

// ── chrome (separate canvas) ─────────────────────────────────────────────────

/** Trace an object's own shape (for hover/selection outlines), grown by `g` source px. */
function traceShape(ctx: Ctx, o: EdObject, env: HitEnv, g = 0): void {
  const ell = (r: Rect) => ctx.ellipse(r.x + r.w / 2, r.y + r.h / 2, Math.max(0, r.w / 2 + g), Math.max(0, r.h / 2 + g), 0, 0, Math.PI * 2);
  const box = (r: Rect) => ctx.rect(r.x - g, r.y - g, r.w + g * 2, r.h + g * 2);
  switch (o.kind) {
    case "arrow": {
      const geo = arrowGeometry(o.a, o.b, o.bend, strokePx(o.size, env.unit));
      ctx.moveTo(geo.x0, geo.y0);
      ctx.quadraticCurveTo((geo.x0 + geo.x1) / 2 + geo.mx, (geo.y0 + geo.y1) / 2 + geo.my, geo.x1, geo.y1);
      const head = arrowHead(geo);
      if (head) {
        ctx.moveTo(...head[0]);
        ctx.lineTo(geo.x1, geo.y1);
        ctx.lineTo(...head[1]);
      }
      break;
    }
    case "line":
      ctx.moveTo(o.a[0], o.a[1]);
      ctx.lineTo(o.b[0], o.b[1]);
      break;
    case "pen":
    case "marker":
      tracePath(ctx, o.points);
      break;
    case "ellipse":
      ell(o.r);
      break;
    case "spotlight":
      if (o.shape === "ellipse") ell(o.r);
      else box(o.r);
      break;
    case "counter": {
      const r = counterRadius(o.size, env.unit) + g;
      ctx.moveTo(o.at[0] + r, o.at[1]);
      ctx.arc(o.at[0], o.at[1], r, 0, Math.PI * 2);
      break;
    }
    default:
      box(shapeBox(o, env));
  }
}

/**
 * Outline the object's shape in screen px: a light halo + a dark line, readable on any picture.
 * Strokes (pen, marker, lines, arrows) get the halo around the stroke itself — no rectangle.
 */
function outline(ctx: Ctx, o: EdObject, env: HitEnv, strong: boolean): void {
  const z = env.scale;
  const w = lineWidth(o, env.unit);
  const isStroke = o.kind === "pen" || o.kind === "marker" || o.kind === "line" || o.kind === "arrow";
  ctx.save();
  ctx.lineCap = o.kind === "marker" ? "butt" : "round";
  ctx.lineJoin = "round";
  ctx.beginPath();
  // boxes: the outline runs just OUTSIDE the stroke, so the object's own colour stays visible
  traceShape(ctx, o, env, isStroke ? 0 : w / 2 + 2.5 / z);
  if (isStroke) {
    // a ring hugging the stroke: wide light pass, then cut the stroke's own width out of it
    ctx.strokeStyle = strong ? "rgba(255,255,255,0.95)" : "rgba(255,255,255,0.8)";
    ctx.lineWidth = w + 9 / z;
    ctx.stroke();
    ctx.strokeStyle = strong ? "rgba(17,17,17,0.92)" : "rgba(17,17,17,0.6)";
    ctx.lineWidth = w + (strong ? 5 : 4) / z;
    ctx.stroke();
    ctx.globalCompositeOperation = "destination-out";
    ctx.lineWidth = w + 1 / z;
    ctx.stroke();
  } else {
    ctx.strokeStyle = "rgba(255,255,255,0.95)";
    ctx.lineWidth = 3 / z;
    ctx.stroke();
    ctx.strokeStyle = strong ? "rgba(17,17,17,0.95)" : "rgba(17,17,17,0.65)";
    ctx.lineWidth = 1.25 / z;
    if (!strong) ctx.setLineDash([4 / z, 3 / z]);
    ctx.stroke();
  }
  ctx.restore();
}

function handle(ctx: Ctx, p: Pt, z: number, round = true): void {
  ctx.beginPath();
  if (round) ctx.arc(p[0], p[1], HANDLE_R / z, 0, Math.PI * 2);
  else ctx.rect(p[0] - HANDLE_R / z, p[1] - HANDLE_R / z, (HANDLE_R * 2) / z, (HANDLE_R * 2) / z);
  ctx.fillStyle = "#ffffff";
  ctx.shadowColor = "rgba(0,0,0,0.35)";
  ctx.shadowBlur = 3;
  ctx.fill();
  ctx.shadowColor = "transparent";
  ctx.strokeStyle = "rgba(17,17,17,0.9)";
  ctx.lineWidth = 1.25 / z;
  ctx.stroke();
}

export interface ChromeState {
  objects: readonly EdObject[];
  sel: readonly string[];
  hover: string | null;
  /** marquee rectangle (source px) */
  marquee: Rect | null;
  /** pending crop (window): dims the outside, 8 handles */
  crop: Rect | null;
  /** picture size (crop dimming) */
  size: { w: number; h: number };
  /** the text being typed: no chrome for it */
  editing: string | null;
}

/** Hover outline, selection outlines + handles, marquee, crop frame (source px under the transform). */
export function drawChrome(ctx: Ctx, st: ChromeState, env: HitEnv): void {
  const z = env.scale;
  if (st.crop) {
    const c = st.crop;
    ctx.save();
    ctx.beginPath();
    ctx.rect(0, 0, st.size.w, st.size.h);
    ctx.rect(c.x, c.y, c.w, c.h);
    ctx.fillStyle = "rgba(0,0,0,0.5)";
    ctx.fill("evenodd");
    ctx.strokeStyle = "rgba(255,255,255,0.95)";
    ctx.lineWidth = 1.5 / z;
    ctx.strokeRect(c.x, c.y, c.w, c.h);
    // rule-of-thirds guides
    ctx.strokeStyle = "rgba(255,255,255,0.35)";
    ctx.lineWidth = 1 / z;
    ctx.beginPath();
    for (const f of [1 / 3, 2 / 3]) {
      ctx.moveTo(c.x + c.w * f, c.y);
      ctx.lineTo(c.x + c.w * f, c.y + c.h);
      ctx.moveTo(c.x, c.y + c.h * f);
      ctx.lineTo(c.x + c.w, c.y + c.h * f);
    }
    ctx.stroke();
    ctx.restore();
    for (const h of cropHandles(c, z)) handle(ctx, h.pt, z, false);
    return;
  }
  const byId = new Map(st.objects.map((o) => [o.id, o]));
  if (st.hover && !st.sel.includes(st.hover) && st.hover !== st.editing) {
    const o = byId.get(st.hover);
    if (o) outline(ctx, o, env, false);
  }
  for (const id of st.sel) {
    const o = byId.get(id);
    if (o && id !== st.editing) outline(ctx, o, env, true);
  }
  if (st.sel.length === 1) {
    const o = byId.get(st.sel[0]);
    // the arrow's bend handle is a square: it reads as "shape", not "end"
    if (o && o.id !== st.editing) for (const h of handlesOf(o, env)) handle(ctx, h.pt, z, h.id !== "mid");
  }
  if (st.marquee) {
    const m = st.marquee;
    ctx.save();
    ctx.fillStyle = "rgba(128,128,128,0.12)";
    ctx.fillRect(m.x, m.y, m.w, m.h);
    ctx.strokeStyle = "rgba(255,255,255,0.9)";
    ctx.lineWidth = 2.5 / z;
    ctx.strokeRect(m.x, m.y, m.w, m.h);
    ctx.strokeStyle = "rgba(17,17,17,0.85)";
    ctx.lineWidth = 1 / z;
    ctx.setLineDash([4 / z, 3 / z]);
    ctx.strokeRect(m.x, m.y, m.w, m.h);
    ctx.restore();
  }
}

/** Set a canvas transform so source px draw at `view` on a canvas with `dpr` device px per CSS px. */
export function setView(ctx: Ctx, v: View, dpr: number): void {
  ctx.setTransform(v.z * dpr, 0, 0, v.z * dpr, v.tx * dpr, v.ty * dpr);
}

/** Text box layout for the text field (same layout as the canvas). */
export function textFieldLayout(o: Extract<EdObject, { kind: "text" }>, unit: number, measure: MeasureText) {
  const s = textShape(o, unit, measure);
  return { shape: s, layout: layoutText(s, measure) };
}

