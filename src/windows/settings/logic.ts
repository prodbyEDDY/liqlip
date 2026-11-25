// Pure logic of the settings window (no Svelte, no IPC, no i18n runtime) — covered by
// tests/settingsLogic.test.mjs. Components turn the returned i18n keys into text.
import type { CaptureStatus, HotkeySettings, Settings, SettingsError } from "$lib/ipc/types";

// ── dotted paths ─────────────────────────────────────────────────────────────

/** Read `a.b.c` from an object. */
export function getPath(obj: unknown, path: string): unknown {
  let cur: unknown = obj;
  for (const key of path.split(".")) {
    if (cur === null || typeof cur !== "object") return undefined;
    cur = (cur as Record<string, unknown>)[key];
  }
  return cur;
}

/** Immutable `a.b.c = value`: clones only the objects along the path. */
export function setPath<T>(obj: T, path: string, value: unknown): T {
  const keys = path.split(".");
  const root = { ...(obj as Record<string, unknown>) };
  let cur = root;
  for (let i = 0; i < keys.length - 1; i++) {
    const next = cur[keys[i]];
    const copy = Array.isArray(next) ? [...next] : { ...(next as Record<string, unknown>) };
    cur[keys[i]] = copy;
    cur = copy as Record<string, unknown>;
  }
  cur[keys[keys.length - 1]] = value;
  return root as T;
}

/** True when an edit of `path` should clear an error on `field` (same, parent or child). */
export function pathsOverlap(path: string, field: string): boolean {
  return path === field || field.startsWith(path + ".") || path.startsWith(field + ".");
}

// ── field errors (SettingsResult.errors) ────────────────────────────────────

export type FieldErrorCode = SettingsError["code"] | "failed";

/** i18n key of the message shown under a field for a `settings_set` error.
 *  `failed` (the command itself rejected) is rendered from the command error instead. */
export function fieldErrorKey(field: string, code: SettingsError["code"]): string {
  if (field.startsWith("hotkeys.")) return code === "taken" ? "settings.error.hotkeyTaken" : "settings.error.hotkeyInvalid";
  if (field === "picker.pasteKeys") return "settings.error.pasteKeys";
  if (field === "privacy.ignoredApps") return "settings.error.ignoredApps";
  if (field === "picker.monitor") return "settings.error.monitor";
  if (field === "ocr.language") return "settings.error.ocrLanguage";
  return "settings.error.invalid";
}

/** Group `SettingsResult.errors` by field (last one wins). */
export function errorsByField(errors: SettingsError[]): Record<string, SettingsError["code"]> {
  const out: Record<string, SettingsError["code"]> = {};
  for (const e of errors) out[e.field] = e.code;
  return out;
}

// ── sizes and counts ─────────────────────────────────────────────────────────

export type SizeUnit = "bytes" | "kb" | "mb" | "gb";

/** Human-readable size in binary units (what Explorer shows): 1536 → { n: "1.5", unit: "kb" }.
 *  One decimal below 10, none above; locale decimal separator. */
export function formatBytes(bytes: number, locale = "en"): { n: string; unit: SizeUnit } {
  const b = Number.isFinite(bytes) && bytes > 0 ? bytes : 0;
  const units: SizeUnit[] = ["bytes", "kb", "mb", "gb"];
  let i = 0;
  let v = b;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  if (i === 0) return { n: new Intl.NumberFormat(locale).format(Math.round(v)), unit: "bytes" };
  const digits = v < 10 ? 1 : 0;
  // Never show "1024 KB": rounding up to the next unit boundary promotes the unit.
  if (Number(v.toFixed(digits)) >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  const n = new Intl.NumberFormat(locale, { maximumFractionDigits: v < 10 ? 1 : 0, minimumFractionDigits: 0 }).format(v);
  return { n, unit: units[i] };
}

/** Grouped integer: 1240 → "1,240" (en) / "1 240" (ru). */
export function formatCount(n: number, locale = "en"): string {
  return new Intl.NumberFormat(locale, { maximumFractionDigits: 0 }).format(Math.max(0, Math.round(n || 0)));
}

// ── names ────────────────────────────────────────────────────────────────────

export const FOLDER_NAME_MAX = 64;
export const SNIPPET_NAME_MAX = 100;
/** Same cap as captured text (content.rs TEXT_MAX_BYTES). */
export const SNIPPET_BODY_MAX_BYTES = 2 * 1024 * 1024;

export type NameIssue = "empty" | "tooLong" | "control" | "exists";

const CONTROL = /[\u0000-\u001f\u007f]/;

/** Folder name rules (mirror the store: trimmed, 1–64 chars, no control chars, unique
 *  case-insensitively). `exceptId` = the folder being renamed. */
export function validateFolderName(
  name: string,
  existing: { id: number; name: string }[] = [],
  exceptId?: number,
): NameIssue | null {
  const n = name.trim();
  if (!n) return "empty";
  if ([...n].length > FOLDER_NAME_MAX) return "tooLong";
  if (CONTROL.test(n)) return "control";
  const lower = n.toLocaleLowerCase();
  if (existing.some((f) => f.id !== exceptId && f.name.trim().toLocaleLowerCase() === lower)) return "exists";
  return null;
}

/** Snippet name rules: trimmed, 1–100 chars, no control chars (duplicates are allowed). */
export function validateSnippetName(name: string): NameIssue | null {
  const n = name.trim();
  if (!n) return "empty";
  if ([...n].length > SNIPPET_NAME_MAX) return "tooLong";
  if (CONTROL.test(n)) return "control";
  return null;
}

/** Snippet body: anything up to 2 MB of UTF-8. */
export function validateSnippetBody(body: string): "tooLong" | null {
  return new TextEncoder().encode(body).length > SNIPPET_BODY_MAX_BYTES ? "tooLong" : null;
}

// ── executables (ignored apps, paste keys) ───────────────────────────────────

/** "C:\\Program Files\\KeePass\\KeePass.exe" → "keepass.exe" (what the lists store:
 *  capture/paste match by file name or full path, case-insensitively). */
export function exeFileName(pathOrName: string): string {
  const s = pathOrName.trim().replace(/^"+|"+$/g, "");
  const base = s.split(/[\\/]/).pop() ?? s;
  return base.toLocaleLowerCase();
}

export function sameExe(a: string, b: string): boolean {
  return exeFileName(a) === exeFileName(b);
}

/** "keepassxc.exe" → "keepassxc" (fallback display name). */
export function exeStem(exe: string): string {
  return exeFileName(exe).replace(/\.exe$/i, "");
}

// ── hotkeys ──────────────────────────────────────────────────────────────────

export type HotkeyAction = keyof HotkeySettings;
export const HOTKEY_ACTIONS: readonly HotkeyAction[] = ["summon", "stackNext", "pauseToggle", "ignoreNext"];

/** Defaults for "Reset to default". Kept equal to the Rust defaults by
 *  tests/settingsLogic.test.mjs (against tests/fixtures/settings.default.json). */
export const HOTKEY_DEFAULTS: Readonly<HotkeySettings> = {
  summon: "Alt + V",
  stackNext: "Alt + Shift + V",
  pauseToggle: "",
  ignoreNext: "",
};

const MODIFIERS = new Set(["Ctrl", "Alt", "Shift", "Super"]);
const SIDED = new Set(["LCtrl", "RCtrl", "LAlt", "RAlt", "LShift", "RShift", "LWin", "RWin"]);
const STANDALONE = /^(F([1-9]|1[0-9]|2[0-4])|Pause|ScrollLock|PrtSc)$/;

export type HotkeyIssue = { kind: "duplicate"; other: HotkeyAction } | { kind: "needsModifier" } | { kind: "needsKey" };

/** Front-side checks before `settings_set` (the engine still has the last word):
 *  another Liqlip action uses it · a typing key without Ctrl/Alt/Win (would break typing) ·
 *  modifiers only. */
export function hotkeyIssue(combo: string, action: HotkeyAction, current: HotkeySettings): HotkeyIssue | null {
  const c = combo.trim();
  if (!c) return null;
  for (const other of HOTKEY_ACTIONS) {
    if (other !== action && current[other].trim() === c) return { kind: "duplicate", other };
  }
  const parts = c.split("+").map((p) => p.trim()).filter(Boolean);
  const key = parts[parts.length - 1] ?? "";
  if (SIDED.has(key) || MODIFIERS.has(key)) return { kind: "needsKey" };
  const strong = parts.slice(0, -1).some((p) => p === "Ctrl" || p === "Alt" || p === "Super");
  if (!strong && !STANDALONE.test(key)) return { kind: "needsModifier" };
  return null;
}

/** "Alt + Shift + V" → ["Alt", "Shift", "V"]. */
export function comboKeys(combo: string): string[] {
  return combo
    .split(" + ")
    .map((k) => k.trim())
    .filter(Boolean);
}

// ── capture pause ────────────────────────────────────────────────────────────

export type PauseState =
  | { kind: "recording" }
  | { kind: "indefinite" }
  | { kind: "until"; at: number; minutesLeft: number };

export function pauseState(status: CaptureStatus | null, now: number): PauseState {
  const p = status?.pausedUntil ?? null;
  if (p === null) return { kind: "recording" };
  if (p < 0) return { kind: "indefinite" };
  if (p <= now) return { kind: "recording" };
  return { kind: "until", at: p, minutesLeft: Math.max(1, Math.ceil((p - now) / 60_000)) };
}

// ── option lists ─────────────────────────────────────────────────────────────

export interface Option {
  value: string;
  label: string;
}

/** Keep a value that is not among the presets selectable (e.g. 3000 set in settings.json). */
export function withCurrent(options: Option[], value: string, label: (v: string) => string): Option[] {
  return options.some((o) => o.value === value) ? options : [...options, { value, label: label(value) }];
}

// ── navigation ───────────────────────────────────────────────────────────────

/** Move within `n` items with wrap-around. */
export function stepIndex(i: number, delta: number, n: number): number {
  if (n <= 0) return 0;
  return (((i + delta) % n) + n) % n;
}

/** A known section key from `?section=` / `settings:navigate`, else `null`. */
export function resolveSection<K extends string>(raw: string | null | undefined, keys: readonly K[]): K | null {
  if (!raw) return null;
  const k = raw.trim().toLowerCase();
  return (keys as readonly string[]).includes(k) ? (k as K) : null;
}

/** Move `id` before `beforeId` (`null` = to the end) in an ordered id list. */
export function moveBefore<T>(ids: T[], id: T, beforeId: T | null): T[] {
  const rest = ids.filter((x) => x !== id);
  const at = beforeId === null ? rest.length : rest.indexOf(beforeId);
  if (at < 0) return ids;
  rest.splice(at, 0, id);
  return rest;
}

/** The settings object with only the sections a user sees (used to compare snapshots). */
export function sameSettings(a: Settings | null, b: Settings | null): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}
