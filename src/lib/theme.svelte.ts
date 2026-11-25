// App theme (light · dark · system), ported from Liquid Tools `theme.svelte.ts` and
// re-pointed at `Settings.general.theme` (backend-authoritative across windows).
//
// The visual switch is pure CSS: we write the RESOLVED theme to <html data-theme>
// ("light"|"dark"); app.css overrides every --color-*/--shadow-* under
// :root[data-theme="dark"]. "system" resolves live from the OS via matchMedia. A
// localStorage mirror makes the first paint of a cold window correct (no flash); the
// real value arrives with `settings_get` / `settings:changed` (see `appearance.ts`), and
// `theme://changed` carries the resolved theme Rust broadcasts.

import type { ResolvedTheme, ThemePref } from "$lib/ipc/types";
import { onThemeChanged } from "$lib/ipc/events";

const STORAGE_KEY = "liqlip.theme";

/** Reactive preference (read-only for components; change it through settings_set). */
export const theme = $state<{ pref: ThemePref }>({ pref: "system" });

function systemPrefersDark(): boolean {
  return typeof matchMedia !== "undefined" && matchMedia("(prefers-color-scheme: dark)").matches;
}

export function resolveTheme(pref: ThemePref): ResolvedTheme {
  return pref === "system" ? (systemPrefersDark() ? "dark" : "light") : pref;
}

function paint(resolved: ResolvedTheme): void {
  if (typeof document !== "undefined") document.documentElement.dataset.theme = resolved;
}

function readStored(): ThemePref {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    if (v === "light" || v === "dark" || v === "system") return v;
  } catch {
    /* unavailable */
  }
  return "system";
}

/** Apply a preference (from settings) to this window and mirror it for the next cold start. */
export function applyThemePref(pref: ThemePref): void {
  theme.pref = pref;
  paint(resolveTheme(pref));
  try {
    localStorage.setItem(STORAGE_KEY, pref);
  } catch {
    /* ignore */
  }
}

let inited = false;

/** Paint the mirrored theme synchronously (call before mount) and follow live changes:
 *  the OS scheme while in "system" mode, and Rust's `theme://changed`. */
export function initTheme(): void {
  if (inited) return;
  inited = true;
  theme.pref = readStored();
  paint(resolveTheme(theme.pref));
  if (typeof matchMedia !== "undefined") {
    matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
      if (theme.pref === "system") paint(resolveTheme("system"));
    });
  }
  void onThemeChanged((resolved) => paint(resolved));
}
