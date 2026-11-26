// What Windows does with PrtSc (`shot_status`, W6-B): when "Use the Print screen key to open
// screen capture" is on, the key goes to the Snipping Tool and Liqlip can't take it (SPEC
// §9.1). Shared by the Screenshots section and the onboarding step. Re-read whenever the window
// comes back to the front — the user may have just flipped the switch in Windows Settings.
// Until the native side answers (`unimplemented` before W6-B) the warning simply isn't shown.
import { shotStatus as fetchShotStatus } from "$lib/ipc/commands";
import type { SettingsError } from "$lib/ipc/types";

class ShotStatusStore {
  /** Windows sends PrtSc to the Snipping Tool. */
  snippingKey = $state(false);
  /** Registration errors of the screenshot shortcuts (`hotkeys.shot*`, ARCHITECTURE §18.11):
   *  the rows show them even before a `settings_set` re-check reports them. */
  hotkeyErrors = $state<SettingsError[]>([]);
  status = $state<"idle" | "loading" | "ready" | "error">("idle");
  #seq = 0;
  #watchers = 0;
  #off: (() => void) | null = null;

  async refresh(): Promise<void> {
    const mine = ++this.#seq;
    if (this.status !== "ready") this.status = "loading";
    try {
      const s = await fetchShotStatus();
      if (mine !== this.#seq) return;
      this.snippingKey = s.snippingKey;
      this.hotkeyErrors = s.hotkeyErrors;
      this.status = "ready";
    } catch {
      if (mine === this.#seq) this.status = "error";
    }
  }

  /** Keep it fresh while a view shows it (call from an `$effect`; returns the disposer). */
  watch(): () => void {
    void this.refresh();
    if (this.#watchers++ === 0 && typeof window !== "undefined") {
      const onFocus = () => void this.refresh();
      const onVisible = () => {
        if (document.visibilityState === "visible") void this.refresh();
      };
      window.addEventListener("focus", onFocus);
      document.addEventListener("visibilitychange", onVisible);
      this.#off = () => {
        window.removeEventListener("focus", onFocus);
        document.removeEventListener("visibilitychange", onVisible);
      };
    }
    return () => {
      if (--this.#watchers === 0) {
        this.#off?.();
        this.#off = null;
      }
    };
  }
}

export const shotStatusStore = new ShotStatusStore();
