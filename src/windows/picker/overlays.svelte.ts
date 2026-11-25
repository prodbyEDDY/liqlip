// Overlay layer registry — the extension point for W2-C (Quick Look, the full Ctrl+K actions
// menu, the paste-stack popover, folder dialogs). A layer is a component rendered by the root's
// `OverlayHost` slot over the grid, inside the panel. Layers stack; the top one owns the keyboard
// (Esc closes it first — SPEC §3.5 "Esc layer by layer"). W1 registers simple built-in layers
// (`overlays/defaults.ts`); W2-C replaces them by calling `overlays.register(kind, Component)`.
//
//   overlays.open("actions", { ids, anchor })   → Layer
//   overlays.close()                           → closes the top layer (or a given one)
//   overlays.top                               → the top layer or null
//   overlays.register("preview", QuickLook)    → the component that renders a kind
//
// A layer component receives `{ layer, close }` (`LayerProps`) and may set `layer.onKey` to
// handle keys before the default (Esc → close). See docs/picker.md §«Точки расширения».
import type { Component } from "svelte";

export type OverlayKind = "actions" | "preview" | "stack" | "confirm" | "pillMenu" | (string & {});

export interface OverlayCtx {
  /** Records the layer acts on (the selection, or the card under the pointer). */
  ids?: number[];
  /** Viewport point to anchor a menu to (context menu, the ⋯ button). */
  anchor?: { x: number; y: number };
  [key: string]: unknown;
}

export interface Layer {
  id: number;
  kind: OverlayKind;
  ctx: OverlayCtx;
  /** Key handler of the layer: return true when the key was handled (the root then calls
   *  preventDefault). Unhandled Esc closes the layer; other keys never reach the grid. */
  onKey: ((e: KeyboardEvent) => boolean) | null;
}

export interface LayerProps {
  layer: Layer;
  close: () => void;
}

class Overlays {
  /** Open layers, bottom → top. */
  layers = $state.raw<Layer[]>([]);
  #components = $state.raw<Record<string, Component<LayerProps>>>({});
  #seq = 0;

  get top(): Layer | null {
    return this.layers.length ? this.layers[this.layers.length - 1] : null;
  }

  /** Register (or replace) the component that renders a kind of layer. */
  register(kind: OverlayKind, component: Component<LayerProps>): void {
    this.#components = { ...this.#components, [kind]: component };
  }

  component(kind: OverlayKind): Component<LayerProps> | null {
    return this.#components[kind] ?? null;
  }

  /** Open a layer on top. Opening a kind that is already on top replaces it. */
  open(kind: OverlayKind, ctx: OverlayCtx = {}): Layer {
    const layer: Layer = { id: ++this.#seq, kind, ctx, onKey: null };
    const rest = this.top?.kind === kind ? this.layers.slice(0, -1) : this.layers;
    this.layers = [...rest, layer];
    return layer;
  }

  /** Close the given layer (and everything above it), or the top one. */
  close(layer: Layer | null = this.top): void {
    if (!layer) return;
    const i = this.layers.findIndex((l) => l.id === layer.id);
    if (i >= 0) this.layers = this.layers.slice(0, i);
  }

  closeAll(): void {
    if (this.layers.length) this.layers = [];
  }

  isOpen(kind: OverlayKind): boolean {
    return this.layers.some((l) => l.kind === kind);
  }

  /** Route a key to the top layer. Returns true when a layer is open (the grid must not see
   *  the key); calls preventDefault for keys the layer handled and for Esc. */
  handleKey(e: KeyboardEvent): boolean {
    const top = this.top;
    if (!top) return false;
    if (top.onKey?.(e)) {
      e.preventDefault();
      return true;
    }
    if (e.code === "Escape") {
      e.preventDefault();
      this.close(top);
    }
    return true;
  }
}

export const overlays = new Overlays();
