// Names, icons and full paths for executables shown in the settings lists (ignored apps,
// paste keys). Lists store only file names ("keepass.exe" — a path changes when an app
// updates; capture matches by file name, name without .exe and full path). A friendly name,
// an icon and the full path (tooltip) come from the running apps (`apps_running`), the apps
// already seen in the history (`apps_list`) and "Browse…" picks. Best-effort: a failure just
// means a generic icon.
import { appsList, appsRunning } from "$lib/ipc/commands";
import { exeFileName, exeStem } from "./logic";

export interface AppLook {
  name: string;
  iconUrl: string | null;
  /** Full path when known (shown as a tooltip). */
  path: string | null;
}

/** Proper names of the pre-filled password managers (not translatable). */
const KNOWN: Record<string, string> = {
  "keepass.exe": "KeePass",
  "keepassxc.exe": "KeePassXC",
  "1password.exe": "1Password",
  "bitwarden.exe": "Bitwarden",
};

class AppDirectory {
  private looks = $state<Record<string, AppLook>>({});
  private loaded = false;

  /** Fetch once per window lifetime (call from sections that show exe lists). */
  ensure(): void {
    if (this.loaded) return;
    this.loaded = true;
    void appsList()
      .then((apps) => this.merge(apps.map((a) => ({ exe: a.exe, name: a.name, iconUrl: a.iconUrl }))))
      .catch(() => {});
    void appsRunning()
      .then((apps) => this.merge(apps))
      .catch(() => {});
  }

  /** Remember apps seen elsewhere (the running-apps dialog, a "Browse…" pick). */
  merge(apps: { exe: string; name?: string; iconUrl?: string | null }[]): void {
    const next = { ...this.looks };
    for (const a of apps) {
      const key = exeFileName(a.exe);
      const prev = next[key];
      const isPath = /[\\/]/.test(a.exe);
      next[key] = {
        name: a.name || prev?.name || KNOWN[key] || exeStem(a.exe),
        iconUrl: a.iconUrl ?? prev?.iconUrl ?? null,
        path: isPath ? a.exe : (prev?.path ?? null),
      };
    }
    this.looks = next;
  }

  look(exe: string): AppLook {
    const key = exeFileName(exe);
    return this.looks[key] ?? { name: KNOWN[key] ?? exeStem(exe), iconUrl: null, path: null };
  }
}

export const appDirectory = new AppDirectory();
