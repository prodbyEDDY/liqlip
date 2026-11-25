// Liquid Tools import state, shared by the Import section and the onboarding step:
// `import_lt_probe` (found / not found / error / unimplemented), `import_lt_run` with
// `import:progress`, and the final report. Commands are W2-B; until then they answer
// `unimplemented` and the UI shows "arrives in an update" — never an error.
import { importLtProbe, importLtRun } from "$lib/ipc/commands";
import { errorCode } from "$lib/ipc/errors";
import { onImportProgress } from "$lib/ipc/events";
import type { ImportProbe, ImportProgress, ImportReport } from "$lib/ipc/types";

export type ProbeState = "idle" | "loading" | "found" | "notFound" | "error" | "unavailable";

class Importer {
  probeState = $state<ProbeState>("idle");
  probe = $state<ImportProbe | null>(null);
  running = $state(false);
  progress = $state<ImportProgress | null>(null);
  report = $state<ImportReport | null>(null);
  runError = $state<unknown>(null);
  private listening = false;

  /** Look for the Liquid Tools database (re-run with `force`). */
  async check(force = false): Promise<ProbeState> {
    if (!force && this.probeState !== "idle" && this.probeState !== "error") return this.probeState;
    this.probeState = "loading";
    try {
      const p = await importLtProbe();
      this.probe = p;
      this.probeState = p.found ? "found" : "notFound";
    } catch (e) {
      this.probeState = errorCode(e) === "unimplemented" ? "unavailable" : "error";
    }
    return this.probeState;
  }

  async run(): Promise<ImportReport | null> {
    if (this.running) return null;
    this.listen();
    this.running = true;
    this.report = null;
    this.runError = null;
    this.progress = null;
    try {
      const r = await importLtRun();
      this.report = r;
      return r;
    } catch (e) {
      if (errorCode(e) === "unimplemented") this.probeState = "unavailable";
      else this.runError = e;
      return null;
    } finally {
      this.running = false;
      this.progress = null;
    }
  }

  private listen(): void {
    if (this.listening) return;
    this.listening = true;
    void onImportProgress((p) => {
      if (this.running) this.progress = p;
    });
  }
}

export const importer = new Importer();
