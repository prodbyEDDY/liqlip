// The shelf key map (SPEC §3.5) — matched by PHYSICAL key (`KeyboardEvent.code`), so every
// shortcut works in the Russian layout (Ctrl+D is `KeyD` whether the key prints "d" or "в").
// Pure: `resolveKey` maps a key press + the focus context to a command; `keyboard.svelte.ts`
// dispatches it. Node-tested (tests/pickerKeymap).

export type NavCommand = "up" | "down" | "left" | "right" | "home" | "end" | "pageUp" | "pageDown";

export type Command =
  | { type: "nav"; dir: NavCommand; extend: boolean }
  | { type: "paste" } // Enter
  | { type: "pastePlain" } // Shift+Enter
  | { type: "copy" } // Ctrl+Enter
  | { type: "pasteKeepOpen" } // Alt+Enter
  | { type: "pasteNth"; n: number; plain: boolean } // Ctrl(+Shift)+1…9
  | { type: "pill"; n: number } // Alt+1…9
  | { type: "pillCycle"; delta: 1 | -1 } // Ctrl+Tab / Ctrl+Shift+Tab
  | { type: "favorite" } // Ctrl+D
  | { type: "rename" } // F2
  | { type: "open" } // Ctrl+O
  | { type: "reveal" } // Ctrl+Shift+O
  | { type: "delete" } // Delete
  | { type: "undo" } // Ctrl+Z
  | { type: "selectAll" } // Ctrl+A
  | { type: "focusSearch" } // Ctrl+F
  | { type: "settings" } // Ctrl+,
  | { type: "escape" } // Esc
  | { type: "preview" } // Space (empty search) / Ctrl+Y   → overlays (W2-C)
  | { type: "actions" } // Ctrl+K                          → overlays (W2-C)
  | { type: "edit" } // Ctrl+E                            → Quick Look in edit mode (W2-C)
  | { type: "stack" }; // Ctrl+Shift+S                    → paste stack

export interface KeyInput {
  code: string;
  key: string;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
  metaKey: boolean;
  repeat?: boolean;
}

export interface KeyContext {
  /** Focus is in the shelf's search field. */
  inSearch: boolean;
  /** Focus is in another text field (folder name, rename) — it owns the keyboard. */
  inOtherInput: boolean;
  /** Focus is on a button — it owns its Enter/Space. */
  onButton: boolean;
  /** The search field has no text (chips don't count: they aren't editable text). */
  searchEmpty: boolean;
  /** The caret sits at the end of the search text with nothing selected. */
  caretAtEnd: boolean;
  /** An undo (delete / move) is available — Ctrl+Z goes to it, not to the text field. */
  undoAvailable: boolean;
}

const DIGIT = /^(?:Digit|Numpad)([1-9])$/;

/** One-shot commands ignore auto-repeat: a held Enter / Ctrl+1 must never paste twice
 *  (coordinator note, W1-C acceptance), a held Delete must not wipe a row of cards. */
const NO_REPEAT: ReadonlySet<Command["type"]> = new Set<Command["type"]>([
  "paste",
  "pastePlain",
  "copy",
  "pasteKeepOpen",
  "pasteNth",
  "favorite",
  "delete",
  "undo",
  "rename",
  "open",
  "reveal",
  "settings",
  "preview",
  "actions",
  "edit",
  "stack",
  "selectAll",
]);

/** Map a key press to a shelf command, or `null` to let the focused element have it.
 *  An auto-repeated one-shot command resolves to null (see `isSwallowedRepeat`). */
export function resolveKey(e: KeyInput, ctx: KeyContext): Command | null {
  const cmd = resolveRaw(e, ctx);
  if (cmd && e.repeat && NO_REPEAT.has(cmd.type)) return null;
  return cmd;
}

/** A held key repeating a one-shot command: the caller prevents its default (a held Enter
 *  must not reach a focused button either) and does nothing else. */
export function isSwallowedRepeat(e: KeyInput, ctx: KeyContext): boolean {
  const cmd = resolveRaw(e, ctx);
  return !!cmd && !!e.repeat && NO_REPEAT.has(cmd.type);
}

function resolveRaw(e: KeyInput, ctx: KeyContext): Command | null {
  if (e.metaKey) return null;
  const { code } = e;
  const ctrl = e.ctrlKey;
  const shift = e.shiftKey;
  const alt = e.altKey;
  const plainKey = !ctrl && !alt;

  if (code === "Escape" && !ctrl && !alt && !shift) return { type: "escape" };
  // Another text field owns everything else (its own Enter/Esc handlers run first).
  if (ctx.inOtherInput) return null;

  // Ctrl / Alt digit chords.
  const digit = DIGIT.exec(code);
  if (digit) {
    const n = Number(digit[1]);
    if (ctrl && !alt) return { type: "pasteNth", n, plain: shift };
    if (alt && !ctrl && !shift) return { type: "pill", n };
    return null;
  }

  if (code === "Enter" || code === "NumpadEnter") {
    if (ctx.onButton) return null;
    if (ctrl && !alt && !shift) return { type: "copy" };
    if (alt && !ctrl && !shift) return { type: "pasteKeepOpen" };
    if (shift && !ctrl && !alt) return { type: "pastePlain" };
    if (!ctrl && !alt && !shift) return { type: "paste" };
    return null;
  }

  if (code === "Tab" && ctrl && !alt) return { type: "pillCycle", delta: shift ? -1 : 1 };

  const textEditing = ctx.inSearch && !ctx.searchEmpty;
  const nav = (dir: NavCommand): Command => ({ type: "nav", dir, extend: shift });

  if (plainKey) {
    switch (code) {
      case "ArrowUp":
        return nav("up");
      case "ArrowDown":
        return nav("down");
      case "ArrowLeft":
        // With text in the search field ←/→ move the caret (Shift: select text).
        return textEditing ? null : nav("left");
      case "ArrowRight":
        return textEditing ? null : nav("right");
      case "Home":
        return textEditing ? null : nav("home");
      case "End":
        return textEditing ? null : nav("end");
      case "PageUp":
        return nav("pageUp");
      case "PageDown":
        return nav("pageDown");
    }
  }
  if (!ctrl && !alt && !shift) {
    switch (code) {
      case "F2":
        return { type: "rename" };
      case "Delete":
        // In a search with text, Delete edits the text unless there's nothing after the caret.
        return textEditing && !ctx.caretAtEnd ? null : { type: "delete" };
      case "Space":
        if (ctx.onButton) return null;
        return ctx.searchEmpty || !ctx.inSearch ? { type: "preview" } : null;
    }
  }

  if (ctrl && !alt) {
    switch (code) {
      case "KeyD":
        return shift ? null : { type: "favorite" };
      case "KeyO":
        return shift ? { type: "reveal" } : { type: "open" };
      case "KeyZ":
        if (shift) return null;
        return ctx.undoAvailable || !textEditing ? { type: "undo" } : null;
      case "KeyA":
        if (shift) return null;
        return textEditing ? null : { type: "selectAll" };
      case "KeyF":
        return shift ? null : { type: "focusSearch" };
      case "Comma":
        return shift ? null : { type: "settings" };
      case "KeyY":
        return shift ? null : { type: "preview" };
      case "KeyK":
        return shift ? null : { type: "actions" };
      case "KeyE":
        return shift ? null : { type: "edit" };
      case "KeyS":
        return shift ? { type: "stack" } : null;
    }
  }
  return null;
}

/** A printable character typed outside the search field — it belongs in the search. */
export function isTypingKey(e: KeyInput): boolean {
  return !e.ctrlKey && !e.altKey && !e.metaKey && e.key.length === 1 && e.key !== " ";
}
