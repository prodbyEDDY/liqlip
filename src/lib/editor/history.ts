// Undo history (SPEC §9.3 rule 11): a stack of document snapshots. Objects are immutable — a
// change replaces only the objects it touches — so consecutive snapshots share everything
// else (structural sharing instead of LT's full clones, defect П-26). One finished operation =
// one step; a series with the same key faster than 500 ms (nudges, wheel, `]` repeats) is
// merged into one step. Pure, node-tested (tests/editorDoc.test.mjs, "history: …").
import type { EdObject, Rect } from "./doc";

/** What a step restores. */
export interface Snap {
  objects: EdObject[];
  crop: Rect | null;
}

export interface History {
  past: Snap[];
  future: Snap[];
  /** key + time of the last step (merging) */
  key: string | null;
  time: number;
}

export const MERGE_MS = 500;
export const HISTORY_MAX = 100;

export function emptyHistory(): History {
  return { past: [], future: [], key: null, time: 0 };
}

/**
 * Record that the document changed from `before`. With `key` equal to the previous step's and
 * less than 500 ms later, the change joins that step (no new entry). Redo is dropped.
 */
export function record(h: History, before: Snap, key: string | null, now: number): History {
  if (key !== null && key === h.key && now - h.time < MERGE_MS && h.past.length) {
    return { past: h.past, future: [], key, time: now };
  }
  const past = h.past.length >= HISTORY_MAX ? h.past.slice(h.past.length - HISTORY_MAX + 1) : h.past.slice();
  past.push(before);
  return { past, future: [], key, time: now };
}

/** Step back: returns the snapshot to restore and the new history (`null` — nothing to undo). */
export function undo(h: History, current: Snap): { snap: Snap; history: History } | null {
  if (!h.past.length) return null;
  const past = h.past.slice(0, -1);
  return { snap: h.past[h.past.length - 1], history: { past, future: [...h.future, current], key: null, time: 0 } };
}

export function redo(h: History, current: Snap): { snap: Snap; history: History } | null {
  if (!h.future.length) return null;
  const future = h.future.slice(0, -1);
  return { snap: h.future[h.future.length - 1], history: { past: [...h.past, current], future, key: null, time: 0 } };
}
