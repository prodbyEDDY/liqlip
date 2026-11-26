// The editor's shared `$state` store (ARCHITECTURE §18.6): one per editor, created by the host
// with `createEditorState()` and passed to `Editor`, `Toolbar` and `PropsBar`, which the host
// places wherever it wants (overlay: at the frame; window: on top). It wraps the pure
// dispatcher (interaction.ts) and persists tool defaults when they change.
import { emptyDoc, type EdDoc, type ToolId } from "./doc";
import { loadDefaults, saveDefaults, type ToolStyle } from "./defaults";
import {
  canRedo,
  canUndo,
  createState,
  currentStyle,
  dispatch,
  isDirty,
  propsFor,
  propTarget,
  type EditorMode,
  type EdEvent,
  type EdState,
  type Env,
  type Result,
} from "./interaction";
import type { StyleProp } from "./tools/transform";

const fallbackMeasure = (text: string, size: number) => text.length * size * 0.55;

export class EditorState {
  /** the whole editor state (immutable snapshots — replaced, never mutated) */
  ed = $state.raw<EdState>(createState(emptyDoc(1, 1), loadDefaults()));
  mode = $state<EditorMode>("window");
  /** the key cheatsheet (`?`) is open */
  cheatsheet = $state(false);
  /** the colour popover is open */
  colorOpen = $state(false);
  /** window mode: current zoom (1 = 100 %), kept by Editor.svelte */
  zoom = $state(1);
  /** window mode: run a view action (set by Editor.svelte) */
  viewAction: (a: "fit" | "actual" | "in" | "out") => void = () => {};

  /** Set by Editor.svelte: what the dispatcher needs from the canvas (scale, measure, clip…). */
  env: () => Env = () => ({ mode: this.mode, scale: 1, measure: fallbackMeasure, clip: null, intents: false });
  /** Set by Editor.svelte: redraws, intents, view effects, `onchange`. */
  onResult: ((r: Result, prev: EdState) => void) | null = null;

  get tool(): ToolId {
    return this.ed.tool;
  }
  /** a pointer gesture is in progress (the overlay fades its panel while drawing under it) */
  get busy(): boolean {
    return this.ed.gesture !== null;
  }
  get typing(): boolean {
    return this.ed.text !== null;
  }
  get canUndo(): boolean {
    return canUndo(this.ed);
  }
  get canRedo(): boolean {
    return canRedo(this.ed);
  }
  get dirty(): boolean {
    return isDirty(this.ed);
  }
  /** the values the properties bar shows */
  get style(): ToolStyle | null {
    return currentStyle(this.ed);
  }
  /** properties that apply to the current target */
  get props(): StyleProp[] {
    return propsFor(this.ed);
  }
  /** who a property change goes to (rule 4) */
  get target(): "selection" | "last" | "tool" | null {
    return propTarget(this.ed);
  }
  get recent(): string[] {
    return this.ed.defaults.recent;
  }

  dispatch(ev: EdEvent): Result {
    const prev = this.ed;
    const r = dispatch(prev, ev, this.env());
    if (r.state !== prev) this.ed = r.state;
    if (r.state.defaults !== prev.defaults) saveDefaults(r.state.defaults);
    if (r.effect?.type === "cheatsheet") this.cheatsheet = !this.cheatsheet;
    this.onResult?.(r, prev);
    return r;
  }

  setTool(tool: ToolId): void {
    this.colorOpen = false;
    this.dispatch({ type: "tool", tool });
  }

  setProp<P extends StyleProp>(prop: P, value: ToolStyle[P]): void {
    this.dispatch({ type: "prop", prop, value, time: performance.now() });
  }

  undo(): void {
    this.dispatch({ type: "undo" });
  }

  redo(): void {
    this.dispatch({ type: "redo" });
  }

  /** Start over on a document (the tool and the defaults carry over). */
  load(doc: EdDoc): void {
    const tool = this.ed.tool === "crop" || this.ed.tool === "eyedropper" ? this.ed.prevTool : this.ed.tool;
    const prev = this.ed;
    this.ed = createState(doc, loadDefaults(), tool);
    this.cheatsheet = false;
    this.colorOpen = false;
    this.onResult?.({ state: this.ed, handled: true }, prev);
  }
}

/** One store per editor; hand it to `Editor`, `Toolbar` and `PropsBar`. */
export function createEditorState(): EditorState {
  return new EditorState();
}
