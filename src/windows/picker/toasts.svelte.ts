// Shelf toasts (one at a time, newest wins) and the undo memory behind Ctrl+Z (SPEC §3.13:
// "Deleted 3 · Undo" for 6 s; Ctrl+Z works while the toast is visible and 30 s after).
import type { IconName } from "$lib/icons";

export interface ToastAction {
  label: string;
  /** Icon of the action capsule (default: undo). */
  icon?: IconName;
  run: () => void;
}

export interface ToastState {
  id: number;
  message: string;
  action: ToastAction | null;
  error: boolean;
  icon: IconName;
}

const UNDO_TOAST_MS = 6_000;
const UNDO_GRACE_MS = 30_000;
/** The store keeps an undo token for 2 minutes (ARCHITECTURE §4.2). */
const TOKEN_TTL_MS = 120_000;

class Toasts {
  current = $state<ToastState | null>(null);
  #timer: ReturnType<typeof setTimeout> | undefined;
  #seq = 0;
  /** Last undoable operation: its token and until when Ctrl+Z may use it. */
  #undo: { token: string; until: number } | null = null;

  show(message: string, opts: { action?: ToastAction; error?: boolean; duration?: number; icon?: IconName } = {}): void {
    clearTimeout(this.#timer);
    const id = ++this.#seq;
    const error = opts.error ?? false;
    this.current = { id, message, action: opts.action ?? null, error, icon: opts.icon ?? (error ? "warning" : "check") };
    this.#timer = setTimeout(() => {
      if (this.current?.id === id) this.current = null;
    }, opts.duration ?? (opts.action ? UNDO_TOAST_MS : 2_400));
  }

  error(message: string): void {
    this.show(message, { error: true, duration: 3_200 });
  }

  dismiss(): void {
    clearTimeout(this.#timer);
    this.current = null;
  }

  /** Remember an undoable operation (the toast itself is shown by the caller). */
  rememberUndo(token: string): void {
    const now = Date.now();
    this.#undo = { token, until: Math.min(now + UNDO_TOAST_MS + UNDO_GRACE_MS, now + TOKEN_TTL_MS) };
  }

  /** The token Ctrl+Z may use now, or null. */
  get undoToken(): string | null {
    if (!this.#undo || Date.now() > this.#undo.until) return null;
    return this.#undo.token;
  }

  /** Take the token (an undo is used once). */
  takeUndo(): string | null {
    const token = this.undoToken;
    this.#undo = null;
    return token;
  }
}

export const toasts = new Toasts();
