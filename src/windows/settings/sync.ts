// Optimistic settings sync (pure, no Svelte) — the engine behind `settings.svelte.ts`,
// covered by tests/settingsSync.test.mjs.
//
// Model: `confirmed` is the last Settings the backend reported (settings_get, a
// settings_set result or settings:changed). Every edit is a patch (`path = value`) applied
// on top of it at once (optimistic view). Patches are sent in order, one settings_set at a
// time, each request carrying everything queued so far on top of the latest confirmed
// state. A request that rejects rolls its patches back and marks their fields `failed`;
// `SettingsResult.errors` (taken / invalid) mark fields too — the backend already reverted
// those values, so the view shows the previous value next to the message.
import type { Settings, SettingsError, SettingsResult } from "$lib/ipc/types";
import { pathsOverlap, setPath, type FieldErrorCode } from "./logic";

export interface FieldError {
  code: FieldErrorCode;
  /** The rejection for `failed`. */
  cause?: unknown;
}

export interface SyncSnapshot {
  view: Settings | null;
  errors: Record<string, FieldError>;
  /** A request is in flight or queued. */
  saving: boolean;
}

interface Patch {
  path: string;
  value: unknown;
}

function applyPatches(base: Settings, patches: Patch[]): Settings {
  return patches.reduce((s, p) => setPath(s, p.path, structuredClone(p.value)), base);
}

export class SettingsSync {
  private confirmed: Settings | null = null;
  private queued: Patch[] = [];
  private inflight: Patch[] | null = null;
  private errors: Record<string, FieldError> = {};
  private waiters: (() => void)[] = [];

  constructor(
    private readonly send: (s: Settings) => Promise<SettingsResult>,
    private readonly onChange: (snap: SyncSnapshot) => void,
  ) {}

  snapshot(): SyncSnapshot {
    const view = this.confirmed ? applyPatches(this.confirmed, [...(this.inflight ?? []), ...this.queued]) : null;
    return { view, errors: { ...this.errors }, saving: this.inflight !== null || this.queued.length > 0 };
  }

  get busy(): boolean {
    return this.inflight !== null || this.queued.length > 0;
  }

  /** New authoritative state (settings_get / settings:changed). */
  receive(settings: Settings): void {
    this.confirmed = settings;
    this.emit();
  }

  /** Optimistic edit; clears errors of the same field. Ignored before the first `receive`. */
  update(path: string, value: unknown): void {
    if (!this.confirmed) return;
    this.clear(path);
    this.queued.push({ path, value });
    this.emit();
    void this.flush();
  }

  /** Errors reported outside an edit (e.g. a conflict check). */
  report(errors: SettingsError[]): void {
    for (const e of errors) this.errors[e.field] = { code: e.code };
    this.emit();
  }

  /** Forget the error of a field (or its children). */
  clear(path: string): void {
    let changed = false;
    for (const field of Object.keys(this.errors)) {
      if (pathsOverlap(path, field)) {
        delete this.errors[field];
        changed = true;
      }
    }
    if (changed) this.emit();
  }

  /** Re-send the confirmed state unchanged (a conflict re-check: the backend reports
   *  bindings it could not register); resolves with the errors, rejects like the command. */
  async recheck(): Promise<SettingsError[]> {
    await this.idle();
    if (!this.confirmed || this.inflight) return [];
    this.inflight = [];
    try {
      const res = await this.send(this.confirmed);
      this.confirmed = res.settings;
      for (const e of res.errors) this.errors[e.field] = { code: e.code };
      return res.errors;
    } finally {
      this.settle();
    }
  }

  /** Resolves when nothing is queued or in flight. */
  idle(): Promise<void> {
    if (!this.busy) return Promise.resolve();
    return new Promise((resolve) => this.waiters.push(resolve));
  }

  private async flush(): Promise<void> {
    if (this.inflight || this.queued.length === 0 || !this.confirmed) return;
    const batch = this.queued;
    this.queued = [];
    this.inflight = batch;
    const payload = applyPatches(this.confirmed, batch);
    try {
      const res = await this.send(payload);
      this.confirmed = res.settings;
      for (const e of res.errors) this.errors[e.field] = { code: e.code };
    } catch (cause) {
      for (const p of batch) this.errors[p.path] = { code: "failed", cause };
    } finally {
      this.settle();
    }
  }

  /** After a request: send what queued meanwhile, or wake idle() waiters. */
  private settle(): void {
    this.inflight = null;
    this.emit();
    if (this.queued.length > 0) {
      void this.flush();
    } else {
      const w = this.waiters;
      this.waiters = [];
      for (const f of w) f();
    }
  }

  private emit(): void {
    this.onChange(this.snapshot());
  }
}
