// HUD toast logic (pure, tested in tests/hudToast.test.mjs). The Svelte view renders what
// these functions decide; the native window is shown/hidden by Rust (`picker/hud.rs`).
import type { HudKind, HudShow } from "$lib/ipc/types";

/** How long the toast stays before its exit transition. Rust hides the native window at
 *  1.66 s (`picker/hud.rs::HIDE_AFTER`), after the ~180 ms exit has finished. */
export const VISIBLE_MS = 1400;

/** `countdown` · `shot_failed` · `text_copied` · `text_empty` — W5 (screenshots, ARCHITECTURE §18.10):
 *  the countdown's `text` is the seconds left, drawn as a large digit in place of the {keys}
 *  chip (W6-F, `countdownDigit`). */
export const HUD_KINDS: readonly HudKind[] = ["copied", "elevated", "stack_empty", "paused", "resumed", "skip_next", "countdown", "shot_failed", "text_copied", "text_empty"];

/** i18n key per kind (`hud.*`). */
export const KIND_KEY = {
  copied: "hud.copied",
  elevated: "hud.elevated",
  stack_empty: "hud.stackEmpty",
  paused: "hud.paused",
  resumed: "hud.resumed",
  skip_next: "hud.skipNext",
  countdown: "hud.countdown",
  shot_failed: "hud.shotFailed",
  text_copied: "hud.textCopied",
  text_empty: "hud.textEmpty",
} as const satisfies Record<HudKind, string>;

/** Icon per kind (`src/lib/icons.ts`). */
export const KIND_ICON = {
  copied: "check",
  elevated: "shield",
  stack_empty: "stack",
  paused: "pause",
  resumed: "play",
  skip_next: "skip",
  countdown: "clock",
  shot_failed: "warning",
  text_copied: "ocr",
  text_empty: "info",
} as const satisfies Record<HudKind, string>;

export type Segment = { kind: "text"; text: string } | { kind: "keys" };

/** Split a translated template on `{keys}` (rendered as a key chip). */
export function segments(template: string): Segment[] {
  const out: Segment[] = [];
  const parts = template.split("{keys}");
  parts.forEach((text, i) => {
    if (text) out.push({ kind: "text", text });
    if (i < parts.length - 1) out.push({ kind: "keys" });
  });
  return out;
}

/** What the `{keys}` chip says: `HudShow.text` — the paste chord of the target app, sent by
 *  the paste flow ("Shift+Insert" for PuTTY/mintty, a per-app setting…) — or the default
 *  (`hud.pasteKeys`, "Ctrl+V") when there is none. */
export function keysLabel(toast: HudShow, fallback: string): string {
  return toast.text?.trim() || fallback;
}

/** A new `hud:show` while the same toast is still visible only extends it (no second entry
 *  animation — the paste flow and the hotkey path may both report "stack empty"). A countdown
 *  ticking 3 → 2 → 1 is one toast too: only its digit changes (W6-F). */
export function isSameToast(current: HudShow | null, visible: boolean, next: HudShow): boolean {
  if (!visible || current === null || current.kind !== next.kind) return false;
  return next.kind === "countdown" || (current.text ?? null) === (next.text ?? null);
}

/** The seconds a `countdown` shows (`text` "3" → "3"); `null` for other kinds or a text that
 *  is not a small whole number (then the digit is left out rather than showing garbage). */
export function countdownDigit(toast: HudShow): string | null {
  if (toast.kind !== "countdown") return null;
  const v = toast.text?.trim() ?? "";
  return /^\d{1,2}$/.test(v) ? String(Number(v)) : null;
}

/** Browser preview: `?kind=copied` (static), `?text=…` (the key chip; `+` is `%2B` in a
 *  query: `&text=Shift%2BInsert`), `?demo=1` (cycles every kind). */
export function previewFromQuery(search: string): { toast: HudShow | null; demo: boolean } {
  const q = new URLSearchParams(search);
  const kind = q.get("kind");
  const text = q.get("text");
  const toast =
    kind && (HUD_KINDS as readonly string[]).includes(kind)
      ? { kind: kind as HudKind, ...(text ? { text } : {}) }
      : null;
  return { toast, demo: q.get("demo") === "1" };
}
