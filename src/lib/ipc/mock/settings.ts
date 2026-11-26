// Mock of `settings_get` / `settings_set` (owner: W1-F). Defaults are the Rust ones —
// `tests/settingsDefaults.test.mjs` checks `MOCK_DEFAULTS` against
// `tests/fixtures/settings.default.json`, which a Rust test also checks. Validation mirrors
// `src-tauri/src/settings.rs::validate`; hotkey conflicts are simulated (`TAKEN`) so the
// "taken" UI can be built. Persisted in localStorage so every window page of the browser
// preview shares one state.
//
// Extra URL flags of this module (browser only; see docs/settings.md):
//   ?mock-lt=1                Liquid Tools is running and holds "Alt + V": the summon chord
//                             is reported "taken" even when unchanged (the startup case)
//   ?mock-settings-error=get  settings_get rejects ("busy") — the window's load-error state
//   ?mock-settings-error=set  settings_set rejects ("busy") — optimistic edits roll back
//
// Stable API for other mock modules: `currentSettings()`.
import type { ResolvedTheme, Settings, SettingsError, SettingsResult } from "../types";
import { fail, mockEmit } from "./bus";

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
  hotkeys: {
    summon: "Alt + V",
    stackNext: "Alt + Shift + V",
    pauseToggle: "",
    ignoreNext: "",
    shotRegion: "PrtSc",
    shotScreen: "Ctrl + PrtSc",
    shotWindow: "",
    shotRepeat: "",
    shotText: "",
  },
  storage: {
    retention: "3m",
    maxEntries: 5000,
    maxSizeMb: 1024,
    types: { text: true, link: true, code: true, color: true, contact: true, image: true, file: true, screenshot: true },
  },
  privacy: {
    respectMarkers: true,
    ignoredApps: ["keepass.exe", "keepassxc.exe", "1password.exe", "bitwarden.exe"],
    secretPatterns: true,
  },
  stack: { newlineAfterPaste: false },
  ocr: { enabled: true, language: "auto" },
  window: { settingsBounds: null },
  screenshot: {
    quick: false,
    magnifier: true,
    dimensions: true,
    sound: true,
    copy: true,
    save: true,
    folder: "",
    fileName: "Liqlip {date} {time}",
    format: "png",
    jpgQuality: 90,
    afterCapture: "overlay",
    corner: "left",
    timeout: 10,
  },
};

/** The hotkey fields in the Rust order (`settings.rs::validate`); the five `shot*` are W5. */
const HOTKEY_FIELDS = ["summon", "stackNext", "pauseToggle", "ignoreNext", "shotRegion", "shotScreen", "shotWindow", "shotRepeat", "shotText"] as const;

/** Characters Windows does not allow in a file name (Rust `FILE_NAME_FORBIDDEN`). */
const FILE_NAME_FORBIDDEN = /[<>:"/\\|?*]/;

/** Chords the mock pretends another app holds (RegisterHotKey would refuse them). The
 *  first four are reserved by Windows; the last two can be pressed in a browser to try the
 *  "taken" message. */
const TAKEN = new Set(["Alt + Tab", "Ctrl + Alt + Delete", "Super + L", "Ctrl + Shift + Escape", "Ctrl + Alt + T", "Alt + Q"]);

/** The chord Liquid Tools holds while it runs (`?mock-lt=1`). */
const LT_CHORD = "Alt + V";

function param(name: string): string | null {
  if (typeof location === "undefined") return null;
  return new URLSearchParams(location.search).get(name);
}

const ltRunning = () => param("mock-lt") === "1";
const settingsError = () => param("mock-settings-error");

const STORAGE_KEY = "liqlip.mock.settings";

function load(): Settings {
  try {
    const raw = typeof localStorage !== "undefined" ? localStorage.getItem(STORAGE_KEY) : null;
    if (raw) {
      const parsed = JSON.parse(raw) as Partial<Settings>;
      // Per-section merge onto the defaults (lenient, like the Rust loader); a nested object
      // (`storage.types`) merges key by key, so a key added later (`screenshot`, W5) keeps its default.
      const merged = structuredClone(MOCK_DEFAULTS) as unknown as Record<string, unknown>;
      const isObj = (x: unknown): x is Record<string, unknown> => !!x && typeof x === "object" && !Array.isArray(x);
      for (const [k, v] of Object.entries(parsed as Record<string, unknown>)) {
        const base = merged[k];
        if (isObj(base) && isObj(v)) {
          const sec: Record<string, unknown> = { ...base, ...v };
          for (const [f, fv] of Object.entries(base)) {
            const vf = v[f];
            if (isObj(fv) && isObj(vf)) sec[f] = { ...fv, ...vf };
          }
          merged[k] = sec;
        } else {
          merged[k] = v ?? base;
        }
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
  for (const k of HOTKEY_FIELDS) {
    hk[k] = hk[k].trim();
    if ([...hk[k]].length > 64) {
      hk[k] = prev.hotkeys[k];
      bad(`hotkeys.${k}`);
    } else if (hk[k] !== prev.hotkeys[k] && TAKEN.has(hk[k])) {
      hk[k] = prev.hotkeys[k];
      bad(`hotkeys.${k}`, "taken");
    } else if (hk[k] === LT_CHORD && ltRunning()) {
      // Registration fails whether or not the value changed; an unchanged value stays.
      if (prev.hotkeys[k] !== LT_CHORD) hk[k] = prev.hotkeys[k];
      bad(`hotkeys.${k}`, "taken");
    }
  }
  // Conflicts across all nine slots: the engine reports them once W6-A registers the five
  // screenshot slots (ARCHITECTURE §18.11) — the mock already does, so the UI can be built.
  const used = new Map<string, string>();
  for (const k of HOTKEY_FIELDS) {
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
  const sh = next.screenshot;
  sh.folder = sh.folder.trim();
  if ([...sh.folder].length > 260 || /[\u0000-\u001f]/.test(sh.folder)) {
    sh.folder = prev.screenshot.folder;
    bad("screenshot.folder");
  }
  sh.fileName = sh.fileName.trim();
  if (badName(sh.fileName, 120) || FILE_NAME_FORBIDDEN.test(sh.fileName)) {
    sh.fileName = prev.screenshot.fileName;
    bad("screenshot.fileName");
  }
  if (!(Number.isInteger(sh.jpgQuality) && sh.jpgQuality >= 60 && sh.jpgQuality <= 100)) {
    sh.jpgQuality = prev.screenshot.jpgQuality;
    bad("screenshot.jpgQuality");
  }
  if (!(Number.isInteger(sh.timeout) && sh.timeout >= 0 && sh.timeout <= 3600)) {
    sh.timeout = prev.screenshot.timeout;
    bad("screenshot.timeout");
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
  if (settingsError() === "set") throw fail("busy", "mock-settings-error");
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
  settings_get: () => {
    if (settingsError() === "get") throw fail("busy", "mock-settings-error");
    return structuredClone(currentSettings());
  },
  settings_set: settingsSet,
};
