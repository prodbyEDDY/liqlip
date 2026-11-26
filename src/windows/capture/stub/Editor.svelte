<script lang="ts">
  // ⚠ TEMPORARY STUB (W6-D) of W6-C's `Editor.svelte` (ARCHITECTURE §18.6) — see `../editorStub.ts`.
  // Same props and the same `bind:this` API; draws only rectangles (tool `rect`) inside `clip`.
  // `mode: 'overlay'`: 1 source px = 1 physical px, the canvas fills its parent.
  import { untrack } from "svelte";
  import type { AnnotDoc, AnnotObject, Rect } from "$lib/ipc/types";
  import type { EditorIntent, EditorState, ToolId } from "../editorStub";

  interface Props {
    state: EditorState;
    base: ImageBitmap;
    doc: AnnotDoc;
    mode: "overlay" | "window";
    clip?: Rect | null;
    onchange?: (doc: AnnotDoc) => void;
    onintent?: (i: EditorIntent) => void;
  }
  // `onintent` is part of the API; the stub never asks the host for anything.
  let { state: es, base, doc: initial, mode, clip = null, onchange }: Props = $props();

  type RectObj = Extract<AnnotObject, { kind: "rect" | "ellipse" }>;
  const TABLE = [2, 3, 5, 8, 12];
  const toolStore = untrack(() => es.tool);
  const colorStore = untrack(() => es.color);
  const start = untrack(() => structuredClone(initial));
  let objects = $state.raw<AnnotObject[]>(start.objects);
  let changed = $state(false);
  let draft = $state.raw<{ id: number; a: [number, number]; r: Rect } | null>(null);
  let cv = $state<HTMLCanvasElement | null>(null);
  let seq = 0;

  const docNow = (): AnnotDoc => ({ ...start, objects: structuredClone(objects) });

  function toSource(e: PointerEvent): [number, number] {
    const r = cv!.getBoundingClientRect();
    return [((e.clientX - r.left) * base.width) / r.width, ((e.clientY - r.top) * base.height) / r.height];
  }

  const inClip = (x: number, y: number) => !clip || (x >= clip.x && y >= clip.y && x < clip.x + clip.w && y < clip.y + clip.h);
  const clampX = (x: number) => (clip ? Math.min(clip.x + clip.w, Math.max(clip.x, x)) : x);
  const clampY = (y: number) => (clip ? Math.min(clip.y + clip.h, Math.max(clip.y, y)) : y);

  function onpointerdown(e: PointerEvent): void {
    if (e.button === 2 && draft) {
      draft = null; // right click during a gesture cancels the object (SPEC §9.2)
      return;
    }
    if (e.button !== 0 || $toolStore !== "rect") return;
    const [x, y] = toSource(e);
    if (!inClip(x, y)) return;
    cv!.setPointerCapture(e.pointerId);
    draft = { id: e.pointerId, a: [x, y], r: { x, y, w: 0, h: 0 } };
  }

  function onpointermove(e: PointerEvent): void {
    if (!draft || draft.id !== e.pointerId) return;
    const [x, y] = toSource(e);
    const bx = clampX(x);
    const by = clampY(y);
    const [ax, ay] = draft.a;
    draft = { ...draft, r: { x: Math.min(ax, bx), y: Math.min(ay, by), w: Math.abs(bx - ax), h: Math.abs(by - ay) } };
  }

  function onpointerup(e: PointerEvent): void {
    if (!draft || draft.id !== e.pointerId) return;
    const r = draft.r;
    draft = null;
    if (r.w < 4 || r.h < 4) return; // a click makes no object (SPEC §9.3 rule 1)
    const o: RectObj = { id: `stub-${++seq}`, color: $colorStore, kind: "rect", r, fill: false, size: 3 };
    objects = [...objects, o];
    changed = true;
    onchange?.(docNow());
  }

  function paint(g: CanvasRenderingContext2D, list: readonly AnnotObject[], extra: Rect | null): void {
    for (const o of list) {
      if (o.kind !== "rect") continue;
      g.strokeStyle = o.color;
      g.lineWidth = TABLE[o.size - 1] * start.unit;
      g.strokeRect(o.r.x, o.r.y, o.r.w, o.r.h);
    }
    if (extra) {
      g.strokeStyle = $colorStore;
      g.lineWidth = TABLE[2] * start.unit;
      g.strokeRect(extra.x, extra.y, extra.w, extra.h);
    }
  }

  // Redraw on change only (no rAF loop).
  $effect(() => {
    if (!cv) return;
    const g = cv.getContext("2d")!;
    g.clearRect(0, 0, cv.width, cv.height);
    g.drawImage(base, 0, 0);
    g.save();
    if (clip) {
      g.beginPath();
      g.rect(clip.x, clip.y, clip.w, clip.h);
      g.clip();
    }
    paint(g, objects, draft?.r ?? null);
    g.restore();
  });

  // ── §18.6 EditorApi ──
  export function handleKey(e: KeyboardEvent): boolean {
    const plain = !e.ctrlKey && !e.altKey && !e.metaKey && !e.shiftKey;
    if (plain && e.code === "KeyV") return setTool("select"), true;
    if (plain && e.code === "KeyR") return setTool("rect"), true;
    if (e.ctrlKey && !e.altKey && !e.shiftKey && e.code === "KeyZ") {
      if (objects.length) {
        objects = objects.slice(0, -1);
        changed = true;
        onchange?.(docNow());
      }
      return true;
    }
    return false;
  }
  export function escape(): boolean {
    if (draft) return (draft = null), true;
    if ($toolStore !== "select") return setTool("select"), true;
    return false;
  }
  export const hasObjects = (): boolean => objects.length > 0;
  export const dirty = (): boolean => changed;
  export const doc = (): AnnotDoc => docNow();
  export async function exportRGBA(rect: Rect): Promise<{ width: number; height: number; data: Uint8Array }> {
    const c = document.createElement("canvas");
    c.width = rect.w;
    c.height = rect.h;
    const g = c.getContext("2d")!;
    g.drawImage(base, -rect.x, -rect.y);
    g.translate(-rect.x, -rect.y);
    paint(g, objects, null);
    const img = g.getImageData(0, 0, rect.w, rect.h);
    return { width: rect.w, height: rect.h, data: new Uint8Array(img.data.buffer) };
  }
  export function setTool(t: ToolId): void {
    toolStore.set(t);
  }
  export const tool = (): ToolId => $toolStore;
</script>

<canvas
  bind:this={cv}
  class="stub-editor"
  class:stub-editor--draw={$toolStore === "rect"}
  data-mode={mode}
  width={base.width}
  height={base.height}
  {onpointerdown}
  {onpointermove}
  {onpointerup}
  onpointercancel={() => (draft = null)}
></canvas>

<style>
  .stub-editor {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    touch-action: none;
  }
  .stub-editor--draw {
    cursor: crosshair;
  }
</style>
