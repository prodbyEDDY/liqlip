// Annotation editor (SPEC §9.3, ARCHITECTURE §18.6) — public entry for hosts.
//
// HOW A HOST EMBEDS THE EDITOR
// ─────────────────────────────
//   import { Editor, Toolbar, PropsBar, createEditorState, emptyDoc } from "$lib/editor";
//
//   const ed = createEditorState();                  // one store per editor
//   let api: Editor;                                  // bind:this → EditorApi below
//   const base = await createImageBitmap(img);        // <img crossorigin="anonymous" src=frameUrl/baseUrl>
//   const doc = emptyDoc(base.width, base.height, monitor.scale);   // or the stored AnnotDoc
//
//   <div style="position:relative; …">                <!-- Editor fills its positioned parent -->
//     <Editor bind:this={api} {base} {doc} mode="overlay" clip={frame} state={ed}
//             onchange={(d) => …} onintent={(i) => …} />
//   </div>
//   <Toolbar state={ed} tips="above" />  <PropsBar state={ed} />    <!-- placed by the host -->
//
//   Keys — ONE window keydown listener in the host:
//     if (api.handleKey(e)) return;        // eaten by the editor (it already preventDefault-ed)
//     if (e.code === "Escape") { if (!api.escape()) { /* nothing to unwind: close / confirm */ } return; }
//     … host keys (Enter → done, F/R/M in stage 1, Ctrl+arrows for the frame …)
//   `handleKey` never eats Escape (that is `escape()`), never eats Ctrl+V (an image is pasted
//   through the window `paste` event, so don't preventDefault Ctrl+V), and while text is being
//   typed it eats every key except Escape (the text field stops its own keys from bubbling).
//   When the host passes `onintent`, Enter / Ctrl+C / Ctrl+S / Ctrl+Shift+S / Ctrl+P / Ctrl+W are
//   eaten and arrive as intents 'done' / 'copy' / 'save' / 'save_as' / 'pin' / 'close' (and a
//   double click on empty space with the Select tool in overlay mode → 'done'); without
//   `onintent` they return `false` and the host handles them itself.
//
//   Overlay specifics: `clip` = the frame in frame px (it may change while the frame is resized;
//   the editor repaints). Presses outside `clip` are ignored (the host's frame handles live
//   there — put the host's chrome ABOVE the editor), drawing is clamped into it, the editor
//   paints the source only inside `clip` (outside it the canvas is transparent: the host's
//   dimmed frame shows through). `ed.busy` is true during a pointer gesture (fade the panel
//   under it). Commit: `await api.exportRGBA(frame)` → `{ width, height, data }` for the raw
//   body; the document for the commit is `frameDoc(api.doc(), frame)` — coordinates relative
//   to the frame, objects entirely outside it dropped.
//
// EditorApi (bind:this):
//   handleKey(e: KeyboardEvent): boolean · escape(): boolean · hasObjects(): boolean ·
//   dirty(): boolean · doc(): AnnotDoc · exportRGBA(rect: Rect): Promise<{width, height, data}> ·
//   setTool(t: ToolId): void · tool(): ToolId · markSaved(): void (additive: dirty → false)
// Props: base, doc, mode, clip?, onchange?, onintent?, state? (additive: the shared store).
// Store (`createEditorState()`): tool, busy, typing, canUndo/canRedo, dirty, zoom, cheatsheet,
//   setTool(), setProp(), undo(), redo(), viewAction('fit'|'actual'|'in'|'out') (window mode).
export { default as Editor } from "./Editor.svelte";
export { default as Toolbar } from "./Toolbar.svelte";
export { default as PropsBar } from "./PropsBar.svelte";
export { createEditorState, EditorState } from "./state.svelte";
export { emptyDoc, loadDoc, cropDoc, TOOLS, type ToolId, type EdDoc, type EdObject } from "./doc";
export { resetDefaults } from "./defaults";
export type { EditorIntent, EditorMode } from "./interaction";
export type { Flattened } from "./export";

import type { AnnotDoc, Rect } from "$lib/ipc/types";
import { cropDoc, type ToolId } from "./doc";

/** What `bind:this` on `Editor` gives the host (ARCHITECTURE §18.6). */
export interface EditorApi {
  /** true — the editor ate the key (and called preventDefault); the host does nothing more */
  handleKey(e: KeyboardEvent): boolean;
  /** one step of the Esc ladder; false — nothing left to unwind (the host decides) */
  escape(): boolean;
  hasObjects(): boolean;
  dirty(): boolean;
  /** the current document (coordinates — px of the source: the whole monitor frame in the overlay) */
  doc(): AnnotDoc;
  /** flatten `rect` (source px); width/height = the rect size (rounded outward, clamped) */
  exportRGBA(rect: Rect): Promise<{ width: number; height: number; data: Uint8Array }>;
  setTool(t: ToolId): void;
  tool(): ToolId;
}
import { bounds } from "./hit";
import { makeMeasure } from "./render";

/** The document of the committed frame: shifted to the frame's corner, outside objects dropped. */
export function frameDoc(doc: AnnotDoc, frame: Rect): AnnotDoc {
  const measure = makeMeasure();
  return cropDoc(doc, frame, (o) => bounds(o, { unit: doc.unit, measure }));
}
