// Mock of `settings_get` / `settings_set` (owner: W1-F; W0 baseline). Defaults are the
// Rust ones — `tests/settingsDefaults.test.mjs` checks `MOCK_DEFAULTS` against
// `tests/fixtures/settings.default.json`, which a Rust test also checks. Validation mirrors
// `src-tauri/src/settings.rs::validate`; hotkey conflicts are simulated for a few chords
// Windows reserves, so the "taken" UI can be built. Persisted in localStorage so every
// window page of the browser preview shares one state.
//
// Stable API for other mock modules: `currentSettings()`.
import type { ResolvedTheme, Settings, SettingsError, SettingsResult } from "../types";
import { mockEmit } from "./bus";

export const MOCK_DEFAULTS: Settings = {
  version: 1,
  general: { language: "system", theme: "system", autostart: true, onboardingDone: false },
  picker: {
    position: "center",
    centerRow: false,
    monitor: "cursor",
    hoverOpen: false,
    hints: true,
    favoritesFirst: false,
    clickAction: "paste",
    closeOnCopy: true,
    alwaysPlain: false,
    bumpOnPaste: false,
    rememberSearch: "60s",
    mergeSeparator: "newline",
    pasteKeys: [],
  },
  hotkeys: { summon: "Alt + V", stackNext: "Alt + Shift + V", pauseToggle: "", ignoreNext: "" },
  storage: {
    retention: "3m",
    maxEntries: 5000,
    maxSizeMb: 1024,
    types: { text: true, link: true, code: true, color: true, contact: true, image: true, file: true },
  },
  privacy: {
    respectMarkers: true,
    ignoredApps: ["keepass.exe", "keepassxc.exe", "1password.exe", "bitwarden.exe"],
    secretPatterns: true,
  },
  stack: { newlineAfterPaste: false },
  ocr: { enabled: true, language: "auto" },
  window: { settingsBounds: null },
};

/** Chords the mock pretends another app holds (RegisterHotKey would refuse them). */
const TAKEN = new Set(["Alt + Tab", "Ctrl + Alt + Delete", "Super + L", "Ctrl + Shift + Escape"]);

const STORAGE_KEY = "liqlip.mock.settings";

function load(): Settings {
  try {
    const raw = typeof localStorage !== "undefined" ? localStorage.getItem(STORAGE_KEY) : null;
    if (raw) {
      const parsed = JSON.parse(raw) as Partial<Settings>;
      // Shallow-per-section merge onto the defaults (lenient, like the Rust loader).
      const merged = structuredClone(MOCK_DEFAULTS) as unknown as Record<string, unknown>;
      for (const [k, v] of Object.entries(parsed)) {
        const base = merged[k];
        merged[k] = base && typeof base === "object" && v && typeof v === "object" && !Array.isArray(v) ? { ...base, ...v } : (v ?? base);
      }
      merged.version = 1;
      return merged as unknown as Settings;
    }
  } catch {
    /* fall back to defaults */
  }
  return structuredClone(MOCK_DEFAULTS);
}

let state: Settings | null = null;

export function currentSettings(): Settings {
  state ??= load();
  return state;
}

function persist(s: Settings): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(s));
  } catch {
    /* unavailable (node tests, private mode) */
  }
}

const badName = (s: string, max: number) => !s || [...s].length > max || /[\u0000-\u001f]/.test(s);
const hasDupes = (xs: string[]) => new Set(xs.map((x) => x.toLowerCase())).size !== xs.length;

/** Mirror of the Rust `validate` + the hotkey hook's conflict checks. */
export function validate(prev: Settings, input: Settings): { next: Settings; errors: SettingsError[] } {
  const next = structuredClone(input);
  const errors: SettingsError[] = [];
  const bad = (field: string, code: SettingsError["code"] = "invalid") => errors.push({ field, code });

  if (next.version !== 1) {
    next.version = 1;
    bad("version");
  }
  next.picker.monitor = next.picker.monitor.trim();
  if (badName(next.picker.monitor, 256)) {
    next.picker.monitor = prev.picker.monitor;
    bad("picker.monitor");
  }
  next.picker.pasteKeys = next.picker.pasteKeys.map((k) => ({ ...k, exe: k.exe.trim() }));
  if (
    next.picker.pasteKeys.length > 500 ||
    next.picker.pasteKeys.some((k) => badName(k.exe, 260)) ||
    hasDupes(next.picker.pasteKeys.map((k) => k.exe))
  ) {
    next.picker.pasteKeys = structuredClone(prev.picker.pasteKeys);
    bad("picker.pasteKeys");
  }
  const hk = next.hotkeys;
  for (const k of ["summon", "stackNext", "pauseToggle", "ignoreNext"] as const) {
    hk[k] = hk[k].trim();
    if ([...hk[k]].length > 64) {
      hk[k] = prev.hotkeys[k];
      bad(`hotkeys.${k}`);
    } else if (hk[k] !== prev.hotkeys[k] && TAKEN.has(hk[k])) {
      hk[k] = prev.hotkeys[k];
      bad(`hotkeys.${k}`, "taken");
    }
  }
  const used = new Map<string, string>();
  for (const k of ["summon", "stackNext", "pauseToggle", "ignoreNext"] as const) {
    if (!hk[k]) continue;
    const other = used.get(hk[k]);
    if (other !== undefined && hk[k] !== prev.hotkeys[k]) {
      hk[k] = prev.hotkeys[k];
      bad(`hotkeys.${k}`);
    } else {
      used.set(hk[k], k);
    }
  }
  if (!(next.storage.maxEntries >= 0 && next.storage.maxEntries <= 10_000_000)) {
    next.storage.maxEntries = prev.storage.maxEntries;
    bad("storage.maxEntries");
  }
  if (!(next.storage.maxSizeMb >= 0 && next.storage.maxSizeMb <= 1_048_576)) {
    next.storage.maxSizeMb = prev.storage.maxSizeMb;
    bad("storage.maxSizeMb");
  }
  next.privacy.ignoredApps = next.privacy.ignoredApps.map((a) => a.trim());
  if (
    next.privacy.ignoredApps.length > 1000 ||
    next.privacy.ignoredApps.some((a) => badName(a, 260)) ||
    hasDupes(next.privacy.ignoredApps)
  ) {
    next.privacy.ignoredApps = [...prev.privacy.ignoredApps];
    bad("privacy.ignoredApps");
  }
  next.ocr.language = next.ocr.language.trim();
  if (!/^[A-Za-z0-9-]{1,35}$/.test(next.ocr.language)) {
    next.ocr.language = prev.ocr.language;
    bad("ocr.language");
  }
  const b = next.window.settingsBounds;
  if (b && !(b.w >= 100 && b.w <= 100_000 && b.h >= 100 && b.h <= 100_000 && Math.abs(b.x) <= 100_000 && Math.abs(b.y) <= 100_000)) {
    next.window.settingsBounds = prev.window.settingsBounds;
    bad("window.settingsBounds");
  }
  return { next, errors };
}

function resolved(s: Settings): ResolvedTheme {
  if (s.general.theme !== "system") return s.general.theme;
  return typeof matchMedia !== "undefined" && matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

function settingsSet({ settings }: Record<string, unknown>): SettingsResult {
  const prev = currentSettings();
  const { next, errors } = validate(prev, settings as Settings);
  if (JSON.stringify(next) !== JSON.stringify(prev)) {
    state = next;
    persist(next);
    mockEmit<Settings>("settings:changed", next);
    if (next.general.theme !== prev.general.theme) mockEmit<ResolvedTheme>("theme://changed", resolved(next));
  }
  return { settings: structuredClone(next), errors };
}

export const settingsHandlers: Record<string, (args: Record<string, unknown>) => unknown> = {
  settings_get: () => structuredClone(currentSettings()),
  settings_set: settingsSet,
};
