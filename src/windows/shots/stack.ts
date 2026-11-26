// The QAO stack model (SPEC §9.4, ARCHITECTURE §18.8) — pure, node-tested (tests/shotsStack.test.mjs).
// Based on Liquid Tools `src/windows/hud/stack.ts` (read-only source): up to 4 cards, the newest
// at the bottom, a fifth pushes the oldest out. LT's in-stack "pin" (= "don't hide", defect П-16)
// is gone — "Pin" pins the picture on the screen (SPEC §9.5) — so every card can expire.
import type { TranslationKey } from "$lib/i18n/en";
import type { IconName } from "$lib/icons";
import type { Clip, ShotAdded, ShotAddedError } from "$lib/ipc/types";

export const MAX_SHOTS = 4;

/** Card geometry, logical px. The card is a squircle r16 (`--r-card`) with a 4 px frame. */
export const CARD_W = 256;
export const CARD_H = 168;
/** A card never shrinks below this when the work area is shorter than four cards. */
export const CARD_MIN_H = 96;
export const GAP = 12;
/** Transparent margin of the window around the stack: holds `--shadow-pop` (no visible cut at
 *  24 px, like the HUD's sides) and is the visual gap from the work-area edges. */
export const BLEED = 24;
/** The QAO window width W6-B uses (fixed). */
export const WINDOW_W = CARD_W + 2 * BLEED;

/** A transient notice inside a card (never over a neighbour, П-16). */
export interface ShotToast {
  key: TranslationKey;
  icon: IconName;
  error: boolean;
  /** clock time (ms) it disappears at */
  until: number;
}

/** What a card is waiting for (its button shows as busy, a second press is ignored). */
export type ShotBusy = "copy" | "edit" | "pin" | "save" | "reveal" | "text" | "delete";

export interface Shot {
  /** `clip:<id>` or `tmp:<tempPath>` — one card per record */
  key: string;
  clip: Clip | null;
  /** thumbnail or full picture URL; `null` — nothing to show (a shot kept out of history) */
  preview: string | null;
  savedPath: string | null;
  tempPath: string | null;
  /** `save_failed` — the persistent "Couldn't save · Change folder" notice; the card then waits */
  error: ShotAddedError | null;
  /** seconds; 0 = never hide */
  timeout: number;
  /** seconds left */
  remaining: number;
  busy: ShotBusy | null;
  toast: ShotToast | null;
}

/** One card per record: the clip id, else the temporary file (`clip: null`). */
export function shotKey(p: Pick<ShotAdded, "clip" | "tempPath">): string | null {
  if (p.clip) return `clip:${p.clip.id}`;
  if (p.tempPath) return `tmp:${p.tempPath}`;
  return null;
}

/** A fresh card for a `shot:added` payload. */
export function makeShot(p: ShotAdded, timeout: number): Shot | null {
  const key = shotKey(p);
  if (!key) return null;
  const t = Number.isFinite(timeout) && timeout > 0 ? timeout : 0;
  return {
    key,
    clip: p.clip,
    preview: p.clip?.thumbUrl ?? null,
    savedPath: p.savedPath ?? p.clip?.meta.shot?.savedPath ?? null,
    tempPath: p.tempPath,
    error: p.error,
    timeout: t,
    remaining: t,
    busy: null,
    toast: null,
  };
}

/** Add a card at the bottom. The same record again (a pixel-identical shot raises the existing
 *  record, §18.5) moves to the bottom as a fresh card; over `MAX_SHOTS` the oldest goes. */
export function addShot(items: readonly Shot[], shot: Shot): Shot[] {
  const next = items.filter((i) => i.key !== shot.key);
  next.push(shot);
  return next.length > MAX_SHOTS ? next.slice(next.length - MAX_SHOTS) : next;
}

export function removeShot(items: readonly Shot[], key: string): Shot[] {
  return items.filter((i) => i.key !== key);
}

/** A card with the save error waits for the user (an unseen failed save must not vanish). */
export const counts = (s: Shot): boolean => s.timeout > 0 && s.error === null;

/** Advance the timers by `elapsed` seconds (nothing while `paused`); expired cards leave. */
export function tickShots(items: readonly Shot[], elapsed: number, paused: boolean): Shot[] {
  if (paused || elapsed <= 0 || !items.some(counts)) return items as Shot[];
  return items.map((i) => (counts(i) ? { ...i, remaining: i.remaining - elapsed } : i)).filter((i) => !counts(i) || i.remaining > 0);
}

/** Window height for `n` cards, logical px (0 — the stack is empty: hide the window). */
export function stackHeight(n: number): number {
  return n <= 0 ? 0 : 2 * BLEED + n * CARD_H + (n - 1) * GAP;
}

/** The timer strip's fill, 0…1 (1 when the card never hides). */
export function progress(s: Pick<Shot, "timeout" | "remaining">): number {
  return s.timeout > 0 ? Math.max(0, Math.min(1, s.remaining / s.timeout)) : 1;
}
