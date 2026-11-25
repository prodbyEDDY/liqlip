// Shared hotkey-capture vocabulary (ported 1:1 from Liquid Tools `src/lib/hotkey.ts`).
// The tokens here MUST stay in sync with the Rust chord parser of the hotkey engine
// (`src-tauri/src/hotkeys/`, W1-D). Notation: "Alt + V". Only keys the Windows
// global-shortcut backend (RegisterHotKey) can actually bind are emitted —
// bare modifiers (Ctrl/Alt/Shift/Win alone) cannot be registered, so they are
// deliberately NOT turned into tokens (a hotkey needs a real, non-modifier key).
// Keys match PHYSICAL keys (`KeyboardEvent.code`), so the Russian layout works.

/**
 * Resolve the non-modifier key of a KeyboardEvent to a human-readable token the
 * Rust parser understands. Returns null for keys that can't be a hotkey (bare
 * modifiers, dead keys, anything unmapped) — the caller keeps waiting.
 */
export function keyToken(e: KeyboardEvent): string | null {
  const code = e.code;
  if (/^Key[A-Z]$/.test(code)) return code.slice(3); // KeyA..KeyZ → A..Z
  if (/^Digit[0-9]$/.test(code)) return code.slice(5); // Digit0..9 → 0..9
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) return code; // F1..F24
  if (/^Numpad[0-9]$/.test(code)) return "Num" + code.slice(6); // Numpad0..9 → Num0..9

  const named: Record<string, string> = {
    // Navigation / editing
    ArrowUp: "Up",
    ArrowDown: "Down",
    ArrowLeft: "Left",
    ArrowRight: "Right",
    Home: "Home",
    End: "End",
    PageUp: "PageUp",
    PageDown: "PageDown",
    Insert: "Insert",
    Delete: "Delete",
    Backspace: "Backspace",
    Space: "Space",
    Enter: "Enter",
    Tab: "Tab",
    Escape: "Escape",
    // Locks / system keys
    CapsLock: "CapsLock",
    NumLock: "NumLock",
    ScrollLock: "ScrollLock",
    PrintScreen: "PrtSc",
    Pause: "Pause",
    // Punctuation (US-position physical keys — shown as the symbol)
    Backquote: "`",
    Minus: "-",
    Equal: "=",
    BracketLeft: "[",
    BracketRight: "]",
    Backslash: "\\",
    Semicolon: ";",
    Quote: "'",
    Comma: ",",
    Period: ".",
    Slash: "/",
    // Numpad operators + enter (distinct from the main keys)
    NumpadEnter: "NumEnter",
    NumpadAdd: "NumAdd",
    NumpadSubtract: "NumSub",
    NumpadMultiply: "NumMul",
    NumpadDivide: "NumDiv",
    NumpadDecimal: "NumDec",
  };
  return named[code] ?? null;
}

/**
 * Side-specific token for a bare modifier key, or null. A lone modifier can't be a
 * RegisterHotKey shortcut; the engine can only bind it through its low-level hook
 * (chords ending in a sided modifier, e.g. "Ctrl + LWin"). Tokens as in Liquid Tools.
 */
export function modifierToken(code: string): string | null {
  const map: Record<string, string> = {
    ControlLeft: "LCtrl",
    ControlRight: "RCtrl",
    AltLeft: "LAlt",
    AltRight: "RAlt",
    ShiftLeft: "LShift",
    ShiftRight: "RShift",
    MetaLeft: "LWin",
    MetaRight: "RWin",
  };
  return map[code] ?? null;
}

/** The held modifiers, in the order Ctrl · Alt · Shift · Super. Accepts any
 *  event carrying the modifier flags (KeyboardEvent OR PointerEvent/MouseEvent),
 *  so mouse-button hotkeys can carry held modifiers too. */
export function activeModifiers(
  e: Pick<KeyboardEvent, "ctrlKey" | "altKey" | "shiftKey" | "metaKey">,
): string[] {
  const mods: string[] = [];
  if (e.ctrlKey) mods.push("Ctrl");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  if (e.metaKey) mods.push("Super");
  return mods;
}

/** True if `code` is a bare modifier key (which can't be a hotkey on its own). */
export function isModifierCode(code: string): boolean {
  return (
    code.startsWith("Control") ||
    code.startsWith("Alt") ||
    code.startsWith("Shift") ||
    code.startsWith("Meta")
  );
}

/**
 * Token for a bindable MOUSE button, or null. Left (0) and right (2) are never
 * bindable — swallowing them would brick normal input — so only the middle
 * button and the two side buttons (X1/X2) qualify. `MouseEvent.button`:
 *   1 = middle → MMB · 3 = X1 (back) → M4 · 4 = X2 (forward) → M5
 * Tokens as in Liquid Tools `hotkeys.rs::token_vk` (Liqlip's engine may not bind them).
 */
export function mouseButtonToken(button: number): string | null {
  switch (button) {
    case 1:
      return "MMB";
    case 3:
      return "M4";
    case 4:
      return "M5";
    default:
      return null;
  }
}
