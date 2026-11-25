// Browser accelerator keys of WebView2, shared by every Liqlip window (W4, FIX-15; moved out
// of the shelf's keymap). WebView2 runs "browser accelerator keys" by default and Tauri 2.11
// doesn't expose the switch, so each window swallows the ones it doesn't use: F5 / Ctrl+R
// would reload the window mid-session, Ctrl+P prints, F7 asks about caret browsing, Alt+←
// navigates back. Matched by physical key (`KeyboardEvent.code`) — the Russian layout too.
// Pure, node-tested (tests/browserKeys).

/** What the check reads from a `KeyboardEvent`. */
export interface BrowserKeyInput {
  code: string;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
  metaKey: boolean;
}

export interface BrowserKeyOptions {
  /** Also Ctrl+F (find on page) — for windows that don't use Ctrl+F themselves (the shelf
   *  does: it focuses its search). */
  find?: boolean;
}

/** Ctrl + these keys are browser features: reload, print, find next, view source, save page,
 *  downloads, history, new window. None edits text. */
const BROWSER_CTRL: ReadonlySet<string> = new Set(["KeyR", "KeyP", "KeyG", "KeyU", "KeyS", "KeyJ", "KeyH", "KeyN", "F5"]);
/** Ctrl+Shift: hard reload, find previous, new private window. */
const BROWSER_CTRL_SHIFT: ReadonlySet<string> = new Set(["KeyR", "KeyG", "KeyN", "F5"]);
const BROWSER_PLAIN: ReadonlySet<string> = new Set(["F3", "F5", "F7", "BrowserBack", "BrowserForward", "BrowserRefresh"]);

/**
 * A key press that would run a browser feature inside the window. The window's keyboard
 * handler prevents its default when nothing else took the key. DevTools keys (F12,
 * Ctrl+Shift+I/J) are left alone: release builds have no DevTools. Text editing (Ctrl+A/C/V/X/Z,
 * arrows, Home/End) never matches.
 */
export function isBrowserShortcut(e: BrowserKeyInput, opts: BrowserKeyOptions = {}): boolean {
  if (e.metaKey) return false;
  if (e.ctrlKey && !e.altKey) {
    if (opts.find && !e.shiftKey && e.code === "KeyF") return true;
    return (e.shiftKey ? BROWSER_CTRL_SHIFT : BROWSER_CTRL).has(e.code);
  }
  if (e.altKey && !e.ctrlKey && !e.shiftKey) return e.code === "ArrowLeft" || e.code === "ArrowRight";
  if (!e.ctrlKey && !e.altKey) return BROWSER_PLAIN.has(e.code);
  return false;
}
