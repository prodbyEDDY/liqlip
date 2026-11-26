// The state machine of the capture overlay host (ARCHITECTURE §18.7; W6-D):
//
//   idle ──shot:session──▶ select ──target──▶ frame ──Done/Copy/…──▶ committing ──ok──▶ idle
//                            │  └─(text mode / quick)────────────────▶ committing
//                            └──────────── Esc / shot:end ─────────────────────────────▶ idle
//   committing ──error──▶ back to the stage it came from (the work is never lost)
//
// Plus what the host decides on its own: which `shot:session` payload restarts the overlay, the
// commit header (§18.4) and the Esc ladder's last step (SPEC §9.2: with objects drawn, close
// only on a second Esc within 3 s). Pure — node-tested (tests/captureMachine.test.mjs).
import type { AnnotDoc, Rect, ShotAction, ShotCommit, ShotSession, ShotSessionMode, ShotStage } from "$lib/ipc/types";

export type Stage = "idle" | "select" | "frame" | "committing";

export interface HostState {
  stage: Stage;
  sessionId: number | null;
  mode: ShotSessionMode;
  /** "Capture on release": stage 2 is skipped. */
  quick: boolean;
  /** The chosen region (frame px): the stage-2 frame, or what a stage-1 commit sends. */
  frame: Rect | null;
  /** Where a failed commit returns. */
  resume: "select" | "frame" | null;
}

export const IDLE: HostState = { stage: "idle", sessionId: null, mode: "region", quick: false, frame: null, resume: null };

/** A session starts (or restarts on another monitor): stage 1. */
export function begin(s: Pick<ShotSession, "id" | "mode" | "quick">): HostState {
  return { stage: "select", sessionId: s.id, mode: s.mode, quick: s.quick, frame: null, resume: null };
}

/** How the host treats an incoming session (`shot:session` or the `shot_session()` answer). */
export type SessionChange = "new" | "update" | "ignore";

/**
 * Another id — a new session (reset). The same id while selecting — the session followed the
 * cursor to another monitor or got its window tree: take the new data. The same id later
 * (frame, committing) — Rust doesn't move a session past stage 1: ignore.
 */
export function sessionChange(current: Pick<ShotSession, "id"> | null, stage: Stage, incoming: Pick<ShotSession, "id">): SessionChange {
  if (!current || current.id !== incoming.id || stage === "idle") return "new";
  return stage === "select" ? "update" : "ignore";
}

/**
 * A target was chosen: a window, a dragged area, the whole screen, the last area (stage 1), or a
 * new area dragged outside the frame (stage 2). Text mode commits `text` right away; "quick"
 * commits `done` from stage 1; otherwise the frame stage starts (or keeps going with the new rect).
 */
export function choose(st: HostState, rect: Rect): { state: HostState; commit: ShotAction | null } {
  if (st.stage !== "select" && st.stage !== "frame") return { state: st, commit: null };
  if (st.mode === "text") return { state: { ...st, frame: rect }, commit: "text" };
  if (st.quick && st.stage === "select") return { state: { ...st, frame: rect }, commit: "done" };
  return { state: { ...st, stage: "frame", frame: rect }, commit: null };
}

/** Start a commit, or `null` when it can't start (nothing chosen, already committing, a text
 *  session asked for anything but `text`). */
export function startCommit(st: HostState, action: ShotAction): HostState | null {
  if (st.sessionId === null || !st.frame) return null;
  if (st.stage !== "select" && st.stage !== "frame") return null;
  if (st.mode === "text" && action !== "text") return null;
  return { ...st, stage: "committing", resume: st.stage };
}

/** The commit failed: back where it started — the frame and the drawing are still there. */
export function commitFailed(st: HostState): HostState {
  if (st.stage !== "committing") return st;
  return { ...st, stage: st.resume ?? "select", resume: null };
}

/** The session is over (`shot:end`, a finished commit, a cancel). Another session's end is ignored. */
export function finish(st: HostState, sessionId?: number): HostState {
  if (sessionId !== undefined && st.sessionId !== null && sessionId !== st.sessionId) return st;
  return IDLE;
}

/**
 * What the overlay reports with `shot_stage`. Rust moves the overlay to the monitor under the
 * cursor only while it hears `select` (§18.3 "following the cursor"), and SPEC §9.2 says it follows
 * "until a selection starts" — so `frame` goes out as soon as one starts in stage 1 (a press that
 * may become a drag, an area from the keyboard), not only in stage 2. W7-R4: dragging an area past
 * the monitor edge used to move the overlay to the other monitor mid-drag (the drag was lost). An
 * abandoned selection (Esc, right click, a failed commit back to stage 1) says `select` again.
 * `null` — no session.
 */
export function reportedStage(st: Pick<HostState, "stage">, selectionStarted: boolean): ShotStage | null {
  if (st.stage === "idle") return null;
  return st.stage === "select" && !selectionStarted ? "select" : "frame";
}

/** The JSON header of the `shot_commit` body (§18.4): no pixels → `width/height = 0`, `doc` null. */
export function commitHeader(sessionId: number, rect: Rect, action: ShotAction, doc: AnnotDoc | null, px: { width: number; height: number } | null): ShotCommit {
  return { sessionId, rect: { ...rect }, action, doc: px ? doc : null, width: px?.width ?? 0, height: px?.height ?? 0 };
}

/** Second Esc within this window closes without saving (SPEC §9.2). */
export const CONFIRM_MS = 3000;

export type EscOutcome = "gesture" | "editor" | "arm" | "cancel";

/**
 * One step of the Esc ladder. The host's own gesture (dragging an area, moving or resizing the
 * frame) goes first; then the editor's steps (`editor.escape()` — gesture, text, selection, the
 * Select tool); when nothing is left: no objects → cancel the session; objects → arm the
 * "Close without saving?" notice, and a second Esc while it is up cancels.
 * `editorEscaped` is evaluated by the caller only when needed (it has side effects).
 */
export function escapeLadder(s: { hostGesture: boolean; editorEscaped: boolean; hasObjects: boolean; armedAt: number | null; now: number }): EscOutcome {
  if (s.hostGesture) return "gesture";
  if (s.editorEscaped) return "editor";
  if (!s.hasObjects) return "cancel";
  return confirmArmed(s.armedAt, s.now) ? "cancel" : "arm";
}

export const confirmArmed = (armedAt: number | null, now: number): boolean => armedAt !== null && now - armedAt < CONFIRM_MS;
