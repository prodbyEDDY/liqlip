// Flattening for the commit (ARCHITECTURE §18.6 "Экспорт"): a canvas of `rect` in SOURCE px,
// the source drawn 1:1, then the same `drawDoc` the screen uses — WYSIWYG by construction (no
// second renderer, no CSS filters). The RGBA goes into the raw commit body (§18.4).
import type { EdDoc, Rect } from "./doc";
import { drawDoc, type EffectCache, type ImageCache, type Source } from "./render";
import type { MeasureText } from "./textLayout";

export interface Flattened {
  width: number;
  height: number;
  data: Uint8Array;
}

/** `rect` rounded outward to whole pixels and clamped into the picture. */
export function exportRect(rect: Rect, w: number, h: number): Rect {
  const x = Math.max(0, Math.floor(rect.x)),
    y = Math.max(0, Math.floor(rect.y));
  const x1 = Math.min(w, Math.ceil(rect.x + rect.w)),
    y1 = Math.min(h, Math.ceil(rect.y + rect.h));
  return { x, y, w: Math.max(0, x1 - x), h: Math.max(0, y1 - y) };
}

export async function flatten(
  base: Source,
  doc: EdDoc,
  rect: Rect,
  env: { measure: MeasureText; effects: EffectCache; images: ImageCache },
): Promise<Flattened> {
  const r = exportRect(rect, doc.w, doc.h);
  if (r.w < 1 || r.h < 1) throw new Error("invalid: empty export rectangle");
  await env.images.ready(doc);
  if (doc.objects.some((o) => o.kind === "text" || o.kind === "counter") && typeof document !== "undefined") await document.fonts.ready;
  const canvas = new OffscreenCanvas(r.w, r.h);
  const ctx = canvas.getContext("2d")!;
  ctx.translate(-r.x, -r.y);
  ctx.drawImage(base, 0, 0);
  drawDoc(ctx, doc, { measure: env.measure, base, effects: env.effects, images: env.images, skip: null, area: r });
  const img = ctx.getImageData(0, 0, r.w, r.h);
  return { width: r.w, height: r.h, data: new Uint8Array(img.data.buffer, img.data.byteOffset, img.data.byteLength) };
}
