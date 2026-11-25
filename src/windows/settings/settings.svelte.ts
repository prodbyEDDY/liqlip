// Settings store of the settings window: `settings_get` at start, `settings:changed` keeps
// it current, every edit goes through `settings_set` optimistically with rollback
// (engine: ./sync.ts). Defaults are never invented here — they come from `settings_get`.
import { settingsGet, settingsSet } from "$lib/ipc/commands";
import { errorMessage } from "$lib/ipc/errors";
import { onSettingsChanged } from "$lib/ipc/events";
import type { Settings, SettingsError } from "$lib/ipc/types";
import { t, type TranslationKey } from "$lib/i18n/index.svelte";
import { fieldErrorKey } from "./logic";
import { SettingsSync, type FieldError, type SyncSnapshot } from "./sync";

class SettingsStore {
  /** Optimistic view; `null` until the first `settings_get`. */
  value = $state<Settings | null>(null);
  status = $state<"loading" | "ready" | "error">("loading");
  loadError = $state<unknown>(null);
  errors = $state<Record<string, FieldError>>({});
  saving = $state(false);

  private sync = new SettingsSync(settingsSet, (snap: SyncSnapshot) => {
    this.value = snap.view;
    this.errors = snap.errors;
    this.saving = snap.saving;
  });
  private started = false;

  /** Subscribe once (window lifetime) and load. */
  start(): void {
    if (this.started) return;
    this.started = true;
    void onSettingsChanged((s) => this.sync.receive(s));
    // The window is pre-created and only shown/hidden: refresh when it becomes visible in
    // case an event was missed or Rust patched a field without broadcasting (window bounds).
    if (typeof document !== "undefined") {
      document.addEventListener("visibilitychange", () => {
        if (document.visibilityState === "visible" && this.status === "ready" && !this.sync.busy) {
          void settingsGet()
            .then((s) => this.sync.receive(s))
            .catch(() => {});
        }
      });
    }
    void this.load();
  }

  async load(): Promise<void> {
    this.status = "loading";
    this.loadError = null;
    try {
      this.sync.receive(await settingsGet());
      this.status = "ready";
    } catch (e) {
      this.loadError = e;
      this.status = "error";
    }
  }

  /** Optimistic edit of one dotted path (`"picker.hints"`). */
  set(path: string, value: unknown): void {
    this.sync.update(path, value);
  }

  /** Field error code (`taken` | `invalid` | `failed`) or undefined. */
  error(field: string): FieldError | undefined {
    return this.errors[field];
  }

  /** Localized message for a field error (or null). */
  message(field: string): string | null {
    const e = this.errors[field];
    if (!e) return null;
    if (e.code === "failed") return t("settings.error.notSaved", { reason: errorMessage(e.cause) });
    return t(fieldErrorKey(field, e.code) as TranslationKey);
  }

  clearError(field: string): void {
    this.sync.clear(field);
  }

  /** Errors found outside an edit (onboarding conflict check). */
  report(errors: SettingsError[]): void {
    this.sync.report(errors);
  }

  /** Re-send the current state so the backend re-reports conflicts (onboarding). */
  recheck(): Promise<SettingsError[]> {
    return this.sync.recheck();
  }

  idle(): Promise<void> {
    return this.sync.idle();
  }
}

export const settings = new SettingsStore();
