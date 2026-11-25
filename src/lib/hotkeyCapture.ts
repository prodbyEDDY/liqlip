// Shared hotkey recorder (ported from Liquid Tools `src/lib/hotkeyCapture.ts`, re-wired to
// the Liqlip IPC contract: `hotkey_record_start/stop` + `hotkeys:captured { combo }`, where
// `combo === ""` means "cancelled"). Tauri combines focused-window listeners with the
// native recorder: the former keeps normal keys recordable if Windows silently removes a
// low-level hook, while native capture handles global/owned keys. In a plain browser only
// the focused-window recorder runs.

import {
  keyToken,
  activeModifiers,
  isModifierCode,
  modifierToken,
  mouseButtonToken,
} from "$lib/hotkey";
import { isTauri } from "$lib/ipc/runtime";
import { hotkeyRecordStart, hotkeyRecordStop } from "$lib/ipc/commands";
import { onHotkeysCaptured } from "$lib/ipc/events";

export interface CaptureSpec {
  /** Accept a lone sided modifier (push-to-talk style). */
  bareModifier?: boolean;
  /** Accept middle/X mouse buttons. */
  mouse?: boolean;
  /** A chord was recorded — the panel persists + registers it. */
  onCommit: (combo: string) => void;
  /** Esc — the panel leaves its capturing state. */
  onCancel: () => void;
}

export interface CaptureSession {
  /** Stop recording (idempotent). Call on commit / cancel / unmount / toggle. */
  stop: () => void;
}

export function startHotkeyCapture(spec: CaptureSpec): CaptureSession {
  return isTauri() ? startNative(spec) : startBrowser(spec);
}

function startNative(spec: CaptureSpec): CaptureSession {
  let live = true;
  let settled = false;
  let unlisten: (() => void) | null = null;
  const fallback = startBrowser({
    ...spec,
    onCommit: commit,
    onCancel: cancel,
  });

  function stop() {
    if (!live) return;
    live = false;
    fallback.stop();
    unlisten?.();
    unlisten = null;
    void hotkeyRecordStop().catch(() => {});
  }

  function commit(combo: string) {
    if (!live || settled) return;
    settled = true;
    stop();
    spec.onCommit(combo);
  }

  function cancel() {
    if (!live || settled) return;
    settled = true;
    stop();
    spec.onCancel();
  }

  // Listener first, THEN arm — a chord pressed immediately after arming can't
  // slip between the two awaits.
  void onHotkeysCaptured(({ combo }) => {
    if (!live) return;
    if (combo) commit(combo);
    else cancel();
  }).then((dispose) => {
    if (!live) {
      dispose();
      return;
    }
    unlisten = dispose;
    void hotkeyRecordStart().catch((error) => {
      // Focused-window capture remains usable for ordinary keys.
      console.error("[hotkey-capture] native recorder failed to start", error);
    });
  }).catch((error) => {
    // Focused-window capture remains usable for ordinary keys.
    console.error("[hotkey-capture] native listener failed to start", error);
  });

  return {
    stop,
  };
}

function startBrowser(spec: CaptureSpec): CaptureSession {
  // The most-recent modifier keydown — committed as a LONE hotkey on its keyup
  // if no other key followed (push-to-talk), matching the native semantics.
  let lastMod: string | null = null;
  let modifierChord: string | null = null;

  function modifierFamily(code: string): string | null {
    if (code.startsWith("Control")) return "Ctrl";
    if (code.startsWith("Alt")) return "Alt";
    if (code.startsWith("Shift")) return "Shift";
    if (code.startsWith("Meta")) return "Super";
    return null;
  }

  function onKeydown(e: KeyboardEvent) {
    // Capture owns every keystroke while armed — never let it reach the app.
    e.preventDefault();
    e.stopPropagation();
    if (e.code === "Escape" && !e.ctrlKey && !e.altKey && !e.shiftKey && !e.metaKey) {
      spec.onCancel();
      return;
    }
    if (isModifierCode(e.code)) {
      lastMod = e.code;
      const token = modifierToken(e.code);
      const family = modifierFamily(e.code);
      const preceding = activeModifiers(e).filter((mod) => mod !== family);
      modifierChord =
        token && preceding.length > 0
          ? [...preceding, token].join(" + ")
          : null;
      return;
    }
    lastMod = null;
    modifierChord = null;
    const token = keyToken(e);
    if (!token) return; // unsupported key — keep waiting
    spec.onCommit([...activeModifiers(e), token].join(" + "));
  }

  function onKeyup(e: KeyboardEvent) {
    if (e.code !== lastMod) return;
    if (modifierChord) {
      spec.onCommit(modifierChord);
      return;
    }
    if (!spec.bareModifier) return;
    const tok = modifierToken(e.code);
    if (tok) spec.onCommit(tok);
  }

  function onPointerDown(e: PointerEvent) {
    if (!spec.mouse) return;
    const token = mouseButtonToken(e.button);
    if (!token) return;
    e.preventDefault();
    e.stopPropagation();
    spec.onCommit([...activeModifiers(e), token].join(" + "));
  }

  window.addEventListener("keydown", onKeydown, true);
  window.addEventListener("keyup", onKeyup, true);
  window.addEventListener("pointerdown", onPointerDown, true);
  return {
    stop() {
      window.removeEventListener("keydown", onKeydown, true);
      window.removeEventListener("keyup", onKeyup, true);
      window.removeEventListener("pointerdown", onPointerDown, true);
    },
  };
}
