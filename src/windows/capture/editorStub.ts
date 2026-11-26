// ⚠ TEMPORARY STUB (W6-D) — stands in for W6-C's annotation editor (`$lib/editor`, ARCHITECTURE
// §18.6) until that branch is merged, so the capture overlay builds and can be checked in the
// browser. It mirrors the §18.6 API the overlay programs against and nothing more: an editor
// that draws plain rectangles (V — Select, R — Rectangle, Ctrl+Z — undo) inside `clip`.
//
// INTEGRATION (coordinator): delete this file and `./stub/`, and point the ONE import in
// `Capture.svelte` at `$lib/editor` (or make this file `export * from "$lib/editor"`). Where the
// overlay relies on the editor API is listed in ARCHITECTURE §18.7 "Опора на API редактора".
//
// Svelte components can't be declared in a `.ts` module, so the three stub components live in
// `./stub/*.svelte`; this file is the single import point (types, shared state, components).
import { writable, type Writable } from "svelte/store";
import type { AnnotDoc, Rect } from "$lib/ipc/types";
export { default as Editor } from "./stub/Editor.svelte";
export { default as Toolbar } from "./stub/Toolbar.svelte";
export { default as PropsBar } from "./stub/PropsBar.svelte";

/** Editor tools (SPEC §9.3). The overlay only asks whether the tool is `select`. */
export type ToolId = "select" | "arrow" | "line" | "rect" | "ellipse" | "pen" | "marker" | "text" | "counter" | "redact" | "spotlight" | "crop" | "eyedropper";

/** What the editor asks its host to do (Enter, Ctrl+C/S/P, …) — §18.6 `onintent`. */
export type EditorIntent = "done" | "copy" | "save" | "save_as" | "pin" | "text" | "close";

/** §18.6 `EditorApi` — what `bind:this` on `<Editor>` gives the host. */
export interface EditorApi {
  handleKey(e: KeyboardEvent): boolean;
  escape(): boolean;
  hasObjects(): boolean;
  dirty(): boolean;
  doc(): AnnotDoc;
  exportRGBA(rect: Rect): Promise<{ width: number; height: number; data: Uint8Array }>;
  setTool(t: ToolId): void;
  tool(): ToolId;
}

/** The state `Editor`, `Toolbar` and `PropsBar` share (§18.6 `createEditorState()`). The stub keeps
 *  it in stores; the real editor keeps `$state` — the overlay only passes it through. */
export interface EditorState {
  tool: Writable<ToolId>;
  color: Writable<string>;
}

export function createEditorState(): EditorState {
  return { tool: writable<ToolId>("select"), color: writable("#e5484d") };
}
