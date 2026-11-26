<script lang="ts">
  // The annotation editor canvas (ARCHITECTURE §18.6) — one component for both hosts: the
  // capture overlay (`mode: 'overlay'`: 1 source px = 1 physical px, no zoom, drawing and export
  // limited to `clip`) and the editor window (`mode: 'window'`: zoom and pan). Two canvases:
  // the document (redrawn only when the document or the view changes) and a light chrome canvas
  // (hover, selection, handles, marquee, crop) — no rAF loop (LT defect П-20). All decisions are
  // made by the pure dispatcher (interaction.ts); this file only translates DOM events into
  // `EdEvent`s in source px and paints what comes back. API for hosts: see index.ts.
  import { onMount, untrack } from "svelte";
  import type { AnnotDoc, Rect } from "$lib/ipc/types";
  import { loadDoc, type EdDoc, type Pt, type ToolId } from "./doc";
  import { cursorFor, find, liveIds, secondaryPress, type EditorIntent, type Mods } from "./interaction";
  import { resetLayout } from "./textLayout";
  import { createEditorState, type EditorState } from "./state.svelte";
  import { drawChrome, drawDoc, EffectCache, ImageCache, makeMeasure, setView } from "./render";
  import { flatten, type Flattened } from "./export";
  import { clampPan, fitView, stepZoom, toSource, zoomAt, type View } from "./view";
  import { rectFrom } from "./geometry";
  import TextInput from "./TextInput.svelte";
  import Cheatsheet from "./Cheatsheet.svelte";

  interface Props {
    /** the decoded source (overlay — the monitor frame; window — the record's source) */
    base: ImageBitmap;
    /** the starting document (a new shot — empty, unit = monitor scale) */
    doc: AnnotDoc;
    mode: "overlay" | "window";
    /** overlay: the frame (source px); drawing and export stay inside; may change any time */
    clip?: Rect | null;
    /** every finished history step */
    onchange?: (doc: AnnotDoc) => void;
    /** Enter / Ctrl+C / Ctrl+S / Ctrl+Shift+S / Ctrl+P / Ctrl+W / double click on empty (overlay, Select) */
    onintent?: (i: EditorIntent) => void;
    /** the shared store for Toolbar / PropsBar (`createEditorState()`); one is made if omitted */
    state?: EditorState;
  }
  let { base, doc: initialDoc, mode, clip = null, onchange, onintent, state: external }: Props = $props();

  const st: EditorState = untrack(() => external) ?? createEditorState();
  const measure = makeMeasure();
  const effects = new EffectCache();
  const images = new ImageCache(() => schedule(true));

  let box: HTMLDivElement;
  let docCanvas: HTMLCanvasElement;
  let chromeCanvas: HTMLCanvasElement;
  let cw = $state(0);
  let ch = $state(0);
  let dpr = 1;
  let view = $state<View>({ z: 1, tx: 0, ty: 0 });
  /** the view follows the box (fit) until the user zooms or pans */
  let fitted = true;
  let spaceHeld = $state(false);
  let pan: { x: number; y: number; v: View; id: number } | null = $state(null);
  let lastMods: Mods = { shift: false, ctrl: false, alt: false };

  const ed = $derived(st.ed);
  const editing = $derived(ed.text ? find(ed, ed.text.id) : undefined);
  const cursor = $derived(pan ? "grabbing" : spaceHeld ? "grab" : cursorFor(ed));

  /** What the window shows: the crop (applied) or the whole picture; the overlay: its frame. */
  function content(): Rect {
    const d = st.ed.doc;
    if (mode === "overlay") return clip ?? { x: 0, y: 0, w: d.w, h: d.h };
    if (st.ed.tool === "crop") return { x: 0, y: 0, w: d.w, h: d.h };
    return d.crop ?? { x: 0, y: 0, w: d.w, h: d.h };
  }

  // ── document & store wiring ────────────────────────────────────────────────

  function init(b: ImageBitmap, d: AnnotDoc): void {
    effects.clear();
    st.mode = mode;
    const loaded: EdDoc = loadDoc(d, b.width, b.height, d?.unit ?? 1).doc;
    st.load(loaded);
    fitted = true;
    layoutView();
  }

  $effect(() => {
    const b = base,
      d = initialDoc;
    untrack(() => init(b, d));
  });

  st.env = () => ({
    mode,
    scale: view.z,
    measure,
    clip: mode === "overlay" ? clip : null,
    sample,
    intents: !!onintent,
  });

  st.onResult = (r, prev) => {
    const s = r.state;
    // a gesture's end repaints the document too: live previews (blur) give way to the exact effect
    if (s.doc !== prev.doc || s.text?.id !== prev.text?.id || !s.gesture !== !prev.gesture) schedule(true);
    else if (s.sel !== prev.sel || s.hover !== prev.hover || s.hoverHandle !== prev.hoverHandle || s.gesture !== prev.gesture || s.crop !== prev.crop || s.tool !== prev.tool) schedule(false);
    if (contentKey(s.doc.crop, s.tool) !== contentKey(prev.doc.crop, prev.tool) && mode === "window") {
      fitted = true;
      layoutView();
    }
    if (s.hist !== prev.hist && !s.gesture) onchange?.(s.doc);
    if (r.intent) onintent?.(r.intent);
    if (r.effect?.type === "view") viewAction(r.effect.action);
  };
  st.viewAction = (a) => viewAction(a);

  function contentKey(crop: Rect | null, tool: ToolId): string {
    return tool === "crop" ? "full" : crop ? `${crop.x},${crop.y},${crop.w},${crop.h}` : "full";
  }

  // overlay: the frame moves → repaint
  $effect(() => {
    void (clip && [clip.x, clip.y, clip.w, clip.h]);
    untrack(() => schedule(true));
  });

  // ── view ───────────────────────────────────────────────────────────────────

  function layoutView(): void {
    if (!cw || !ch) return;
    if (mode === "overlay") {
      const z = cw / Math.max(1, st.ed.doc.w);
      view = { z, tx: 0, ty: 0 };
    } else if (fitted) view = fitView(content(), cw, ch);
    st.zoom = view.z;
    schedule(true);
  }

  function setViewTo(v: View): void {
    view = clampPan(v, content(), cw, ch);
    st.zoom = view.z;
    fitted = false;
    schedule(true);
  }

  function viewAction(a: "fit" | "actual" | "in" | "out"): void {
    if (mode !== "window") return;
    if (a === "fit") {
      fitted = true;
      layoutView();
      return;
    }
    const z = a === "actual" ? 1 : stepZoom(view.z, a === "in" ? 1 : -1);
    setViewTo(zoomAt(view, z, cw / 2, ch / 2));
  }

  onMount(() => {
    const ro = new ResizeObserver(() => {
      const r = box.getBoundingClientRect();
      cw = r.width;
      ch = r.height;
      dpr = window.devicePixelRatio || 1;
      for (const c of [docCanvas, chromeCanvas]) {
        c.width = Math.max(1, Math.round(cw * dpr));
        c.height = Math.max(1, Math.round(ch * dpr));
      }
      layoutView();
    });
    ro.observe(box);
    const wheel = (e: WheelEvent) => onWheel(e);
    box.addEventListener("wheel", wheel, { passive: false });
    // a font face (e.g. Inter's Cyrillic subset, fetched on first use) arriving after a text was
    // measured/drawn with the fallback: measure again and repaint
    const fonts = typeof document !== "undefined" ? document.fonts : undefined;
    const onFonts = () => {
      resetLayout(measure);
      schedule(true);
    };
    fonts?.addEventListener("loadingdone", onFonts);
    return () => {
      ro.disconnect();
      box.removeEventListener("wheel", wheel);
      fonts?.removeEventListener("loadingdone", onFonts);
      if (raf) cancelAnimationFrame(raf);
    };
  });

  // ── painting (on demand, coalesced into one frame) ─────────────────────────

  let raf = 0;
  let docDirty = true;
  function schedule(docToo: boolean): void {
    if (docToo) docDirty = true;
    if (!raf && typeof requestAnimationFrame !== "undefined") raf = requestAnimationFrame(paint);
  }

  function paint(): void {
    raf = 0;
    if (!docCanvas || !chromeCanvas) return;
    const s = st.ed;
    if (docDirty) {
      docDirty = false;
      const ctx = docCanvas.getContext("2d")!;
      ctx.setTransform(1, 0, 0, 1, 0, 0);
      ctx.clearRect(0, 0, docCanvas.width, docCanvas.height);
      setView(ctx, view, dpr);
      const area = content();
      ctx.save();
      ctx.beginPath();
      ctx.rect(area.x, area.y, area.w, area.h);
      ctx.clip();
      ctx.imageSmoothingEnabled = view.z < 1;
      ctx.imageSmoothingQuality = "high";
      ctx.drawImage(base, 0, 0);
      ctx.imageSmoothingEnabled = true;
      drawDoc(ctx, s.doc, { measure, base, effects, images, skip: s.text?.id ?? null, area, live: liveIds(s) });
      ctx.restore();
    }
    const cx = chromeCanvas.getContext("2d")!;
    cx.setTransform(1, 0, 0, 1, 0, 0);
    cx.clearRect(0, 0, chromeCanvas.width, chromeCanvas.height);
    setView(cx, view, dpr);
    const g = s.gesture;
    drawChrome(
      cx,
      {
        objects: s.doc.objects,
        sel: s.sel,
        hover: s.hover,
        marquee: g?.kind === "marquee" ? rectFrom(g.start, g.cur) : null,
        crop: s.tool === "crop" ? s.crop : null,
        size: { w: s.doc.w, h: s.doc.h },
        editing: s.text?.id ?? null,
      },
      { unit: s.doc.unit, measure, scale: view.z },
    );
  }

  // ── pointer ────────────────────────────────────────────────────────────────

  function toSrc(e: { clientX: number; clientY: number }): Pt {
    const r = chromeCanvas.getBoundingClientRect();
    return toSource(view, e.clientX - r.left, e.clientY - r.top);
  }
  const modsOf = (e: MouseEvent | KeyboardEvent): Mods => ({ shift: e.shiftKey, ctrl: e.ctrlKey || e.metaKey, alt: e.altKey });

  function onPointerDown(e: PointerEvent): void {
    if (st.cheatsheet) {
      st.cheatsheet = false;
      return;
    }
    st.colorOpen = false;
    const active = document.activeElement as HTMLElement | null;
    if (active && active !== document.body && !box.contains(active)) active.blur();
    if (mode === "window" && (e.button === 1 || (e.button === 0 && spaceHeld))) {
      e.preventDefault();
      pan = { x: e.clientX, y: e.clientY, v: view, id: e.pointerId };
      chromeCanvas.setPointerCapture(e.pointerId);
      return;
    }
    if (e.button === 1) e.preventDefault();
    const r = st.dispatch({ type: "down", pt: toSrc(e), button: e.button, mods: modsOf(e), time: e.timeStamp });
    if (r.handled) e.preventDefault();
    if (st.ed.gesture) chromeCanvas.setPointerCapture(e.pointerId);
  }

  function onPointerMove(e: PointerEvent): void {
    if (pan) {
      setViewTo({ z: pan.v.z, tx: pan.v.tx + e.clientX - pan.x, ty: pan.v.ty + e.clientY - pan.y });
      return;
    }
    const mods = modsOf(e);
    const g = st.ed.gesture;
    if (g && secondaryPress(e)) {
      // right button while drawing (a chorded press arrives as a move): cancel the object
      st.dispatch({ type: "down", pt: toSrc(e), button: 2, mods, time: e.timeStamp });
      return;
    }
    // pen strokes take every coalesced sample (smoother curves); everything else the last one
    const samples = g?.kind === "create" && typeof e.getCoalescedEvents === "function" ? e.getCoalescedEvents() : [];
    if (samples.length > 1) for (const c of samples) st.dispatch({ type: "move", pt: toSrc(c), button: 0, mods, time: c.timeStamp });
    else st.dispatch({ type: "move", pt: toSrc(e), button: 0, mods, time: e.timeStamp });
  }

  function onPointerUp(e: PointerEvent): void {
    if (pan && pan.id === e.pointerId) {
      pan = null;
      return;
    }
    if (st.ed.gesture) st.dispatch({ type: "up", pt: toSrc(e), button: e.button, mods: modsOf(e), time: e.timeStamp });
  }

  function onLost(): void {
    pan = null;
    if (st.ed.gesture) st.dispatch({ type: "cancel" });
  }

  function onDbl(e: MouseEvent): void {
    st.dispatch({ type: "dbl", pt: toSrc(e), button: 0, mods: modsOf(e), time: e.timeStamp });
  }

  function onWheel(e: WheelEvent): void {
    if (mode !== "window") return;
    e.preventDefault();
    const r = chromeCanvas.getBoundingClientRect();
    const sx = e.clientX - r.left,
      sy = e.clientY - r.top;
    const dy = e.deltaMode === 1 ? e.deltaY * 16 : e.deltaY;
    const dx = e.deltaMode === 1 ? e.deltaX * 16 : e.deltaX;
    if (e.ctrlKey || e.metaKey) setViewTo(zoomAt(view, view.z * Math.exp(-dy * 0.0015), sx, sy));
    else if (e.shiftKey) setViewTo({ ...view, tx: view.tx - dy - dx });
    else setViewTo({ ...view, tx: view.tx - dx, ty: view.ty - dy });
  }

  // ── keyboard: modifier tracking, Space-to-pan, paste ───────────────────────

  function isField(el: EventTarget | null): boolean {
    const n = el as HTMLElement | null;
    return !!n && (n.tagName === "INPUT" || n.tagName === "TEXTAREA" || n.tagName === "SELECT" || n.isContentEditable);
  }

  function trackKeys(e: KeyboardEvent): void {
    const m = modsOf(e);
    if (m.shift !== lastMods.shift || m.ctrl !== lastMods.ctrl || m.alt !== lastMods.alt) {
      lastMods = m;
      st.dispatch({ type: "mods", mods: m });
    }
    if (mode === "window" && e.code === "Space" && !isField(e.target) && !st.typing) {
      const down = e.type === "keydown";
      if (down && (e.target as HTMLElement)?.tagName === "BUTTON") return;
      spaceHeld = down;
      if (down) e.preventDefault();
    }
  }

  function onBlur(): void {
    spaceHeld = false;
    pan = null;
    if (lastMods.ctrl || lastMods.shift || lastMods.alt) {
      lastMods = { shift: false, ctrl: false, alt: false };
      st.dispatch({ type: "mods", mods: lastMods });
    }
    if (st.ed.gesture) st.dispatch({ type: "cancel" });
  }

  const PASTE_TYPES = /^image\/(png|jpeg|webp|gif|bmp)$/;
  const PASTE_MAX_BYTES = 25 * 1024 * 1024;
  const PASTE_MAX_PX = 40_000_000;

  async function onPaste(e: ClipboardEvent): Promise<void> {
    if (isField(e.target) || st.typing) return;
    const item = [...(e.clipboardData?.items ?? [])].find((i) => i.kind === "file" && PASTE_TYPES.test(i.type));
    const file = item?.getAsFile();
    if (!file) return;
    e.preventDefault();
    if (file.size > PASTE_MAX_BYTES) return;
    try {
      const bmp = await createImageBitmap(file);
      if (bmp.width * bmp.height > PASTE_MAX_PX) return;
      const c = new OffscreenCanvas(bmp.width, bmp.height);
      c.getContext("2d")!.drawImage(bmp, 0, 0);
      const blob = await c.convertToBlob({ type: "image/png" });
      const src = await new Promise<string>((res, rej) => {
        const fr = new FileReader();
        fr.onload = () => res(String(fr.result));
        fr.onerror = () => rej(fr.error);
        fr.readAsDataURL(blob);
      });
      // centred in what is visible, at most 60 % of it, proportions kept, never enlarged
      const area = visibleArea();
      const k = Math.min(1, (area.w * 0.6) / bmp.width, (area.h * 0.6) / bmp.height);
      const w = bmp.width * k,
        h = bmp.height * k;
      st.dispatch({ type: "image", src, box: { x: area.x + (area.w - w) / 2, y: area.y + (area.h - h) / 2, w, h }, time: performance.now() });
    } catch (err) {
      console.warn("[editor] paste failed", err);
    }
  }

  function visibleArea(): Rect {
    const c = content();
    if (mode === "overlay") return c;
    const [x0, y0] = toSource(view, 0, 0);
    const [x1, y1] = toSource(view, cw, ch);
    const x = Math.max(c.x, x0),
      y = Math.max(c.y, y0);
    const r = { x, y, w: Math.min(c.x + c.w, x1) - x, h: Math.min(c.y + c.h, y1) - y };
    return r.w > 1 && r.h > 1 ? r : c;
  }

  function sample(p: Pt): string | null {
    const x = Math.floor(p[0]),
      y = Math.floor(p[1]);
    if (x < 0 || y < 0 || x >= base.width || y >= base.height) return null;
    const c = new OffscreenCanvas(1, 1).getContext("2d", { willReadFrequently: true })!;
    c.drawImage(base, -x, -y);
    const d = c.getImageData(0, 0, 1, 1).data;
    return `#${[d[0], d[1], d[2]].map((v) => v.toString(16).padStart(2, "0")).join("")}`;
  }

  // ── API (bind:this) — ARCHITECTURE §18.6 ───────────────────────────────────

  /** Hosts call this first for every keydown; `true` — eaten (the host does nothing more).
   *  Escape is never eaten here: the host calls `escape()`. Ctrl+V is left to the `paste` event. */
  export function handleKey(e: KeyboardEvent): boolean {
    if (e.type !== "keydown") return false;
    // an IME composing owns every key, Esc too (it cancels the composition — the host must not
    // run its Esc ladder); otherwise while typing every key but Esc belongs to the text field
    if (e.isComposing || e.keyCode === 229) return true;
    if (st.typing) return e.code !== "Escape";
    if (isField(e.target)) return false;
    const r = st.dispatch({
      type: "key",
      code: e.code,
      ctrl: e.ctrlKey || e.metaKey,
      shift: e.shiftKey,
      alt: e.altKey,
      repeat: e.repeat,
      composing: e.isComposing,
      time: e.timeStamp,
    });
    if (r.handled) e.preventDefault();
    return r.handled;
  }

  /** One step of the Esc ladder; `false` — nothing left to unwind (the host decides). */
  export function escape(): boolean {
    if (st.cheatsheet) {
      st.cheatsheet = false;
      return true;
    }
    if (st.colorOpen) {
      st.colorOpen = false;
      return true;
    }
    return st.dispatch({ type: "escape" }).handled;
  }

  export function hasObjects(): boolean {
    return st.ed.doc.objects.length > 0;
  }

  export function dirty(): boolean {
    return st.dirty;
  }

  /** The current document (source px). */
  export function doc(): AnnotDoc {
    return st.ed.doc;
  }

  /** Flatten `rect` (source px): the source + the annotations, drawn by the same code as the screen. */
  export async function exportRGBA(rect: Rect): Promise<Flattened> {
    if (st.ed.text) st.dispatch({ type: "textCommit" });
    if (st.ed.gesture) st.dispatch({ type: "cancel" });
    const area = mode === "overlay" && clip ? intersect(rect, clip) : rect;
    return flatten(base, st.ed.doc, area, { measure, effects, images });
  }

  export function setTool(t: ToolId): void {
    st.setTool(t);
  }

  export function tool(): ToolId {
    return st.ed.tool;
  }

  /** Additive: mark the current document as saved (dirty → false). */
  export function markSaved(): void {
    st.dispatch({ type: "saved" });
  }

  function intersect(a: Rect, b: Rect): Rect {
    const x = Math.max(a.x, b.x),
      y = Math.max(a.y, b.y);
    return { x, y, w: Math.max(0, Math.min(a.x + a.w, b.x + b.w) - x), h: Math.max(0, Math.min(a.y + a.h, b.y + b.h) - y) };
  }
</script>

<svelte:window onkeydown={trackKeys} onkeyup={trackKeys} onblur={onBlur} onpaste={onPaste} />

<div class="ed" class:ed--overlay={mode === "overlay"} bind:this={box}>
  <canvas class="ed__layer" bind:this={docCanvas} aria-hidden="true"></canvas>
  <canvas
    class="ed__layer ed__chrome"
    bind:this={chromeCanvas}
    style:cursor
    onpointerdown={onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={onPointerUp}
    onpointercancel={onLost}
    onlostpointercapture={onLost}
    ondblclick={onDbl}
    oncontextmenu={(e) => e.preventDefault()}
  ></canvas>
  {#if editing && editing.kind === "text"}
    {#key editing.id}
      <TextInput state={st} obj={editing} {view} unit={ed.doc.unit} {measure} />
    {/key}
  {/if}
  {#if st.cheatsheet}<Cheatsheet state={st} />{/if}
</div>

<style>
  .ed {
    position: absolute;
    inset: 0;
    overflow: hidden;
    touch-action: none;
  }
  .ed__layer {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    display: block;
  }
</style>
