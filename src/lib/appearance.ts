// Per-window bootstrap of theme + language from `Settings.general` (backend-authoritative).
// Call once from each window entry BEFORE mount: the mirrored theme/locale paint the
// first frame, then the real settings arrive and every later `settings:changed` applies.
import { initTheme, applyThemePref } from "$lib/theme.svelte";
import { applyLanguage } from "$lib/i18n/index.svelte";
import { settingsGet } from "$lib/ipc/commands";
import { onSettingsChanged } from "$lib/ipc/events";
import type { Settings } from "$lib/ipc/types";

function apply(s: Settings): void {
  applyThemePref(s.general.theme);
  applyLanguage(s.general.language);
}

export function initAppearance(): void {
  initTheme();
  void settingsGet()
    .then(apply)
    .catch((e) => console.error("[appearance] settings_get failed", e));
  void onSettingsChanged(apply);
}
