// Pure logic of the settings window (no Svelte, no IPC, no i18n runtime) — covered by
// tests/settingsLogic.test.mjs. Components turn the returned i18n keys into text.
import type { CaptureStatus, HotkeySettings, SettingsError } from "$lib/ipc/types";
import type { TranslationKey } from "$lib/i18n/en";

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
  // Screenshot chords: "taken" is usually Windows itself (PrtSc → the Snipping Tool, SPEC §9.1).
  if (field.startsWith("hotkeys.shot")) return code === "taken" ? "settings.shot.error.hotkeyTaken" : "settings.error.hotkeyInvalid";
  if (field.startsWith("hotkeys.")) return code === "taken" ? "settings.error.hotkeyTaken" : "settings.error.hotkeyInvalid";
  if (field === "screenshot.fileName") return "settings.shot.error.fileName";
  if (field === "screenshot.folder") return "settings.shot.error.folder";
  // settings.rs: the autostart plugin failed → the previous value stays, `invalid`.
  if (field === "general.autostart") return "settings.error.autostart";
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

/** The name to save with an autosave: the draft when it is valid, otherwise the last saved
 *  name — so body edits are never held back (and lost on leaving) by a half-typed or empty
 *  name; the name field keeps showing its own error until it is fixed. */
export function snippetSaveName(draft: string, saved: string): string {
  return validateSnippetName(draft) === null ? draft.trim() : saved;
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
/** The global actions group of the Hotkeys section. */
export const HOTKEY_ACTIONS: readonly HotkeyAction[] = ["summon", "stackNext", "pauseToggle", "ignoreNext"];
/** The "Screenshots" group (SPEC §9.1, ARCHITECTURE §18.11 slot order). */
export const SHOT_HOTKEY_ACTIONS: readonly HotkeyAction[] = ["shotRegion", "shotScreen", "shotWindow", "shotRepeat", "shotText"];
/** Every slot the engine registers — a chord may be used by one of them only. */
export const ALL_HOTKEY_ACTIONS: readonly HotkeyAction[] = [...HOTKEY_ACTIONS, ...SHOT_HOTKEY_ACTIONS];

const isShotAction = (a: HotkeyAction) => (SHOT_HOTKEY_ACTIONS as readonly string[]).includes(a);

/** i18n key of an action's name (`settings.hotkeys.<action>`; screenshot slots live in the
 *  screenshot block: `settings.shot.hotkey.<action>`). */
export function hotkeyLabelKey(action: HotkeyAction): TranslationKey {
  return (isShotAction(action) ? `settings.shot.hotkey.${action}` : `settings.hotkeys.${action}`) as TranslationKey;
}

/** i18n key of an action's one-line explanation. */
export function hotkeyDescKey(action: HotkeyAction): TranslationKey {
  return `${hotkeyLabelKey(action)}.desc` as TranslationKey;
}

/** Defaults for "Reset to default". Kept equal to the Rust defaults by
 *  tests/settingsLogic.test.mjs (against tests/fixtures/settings.default.json). */
export const HOTKEY_DEFAULTS: Readonly<HotkeySettings> = {
  summon: "Alt + V",
  stackNext: "Alt + Shift + V",
  pauseToggle: "",
  ignoreNext: "",
  // W5 (ARCHITECTURE §18.10): screenshot slots — shown and validated by W6-F.
  shotRegion: "PrtSc",
  shotScreen: "Ctrl + PrtSc",
  shotWindow: "",
  shotRepeat: "",
  shotText: "",
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
  for (const other of ALL_HOTKEY_ACTIONS) {
    if (other !== action && (current[other] ?? "").trim() === c) return { kind: "duplicate", other };
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

/** A chord as Windows users read it ("Super" is the Win key). */
export function comboLabel(combo: string): string {
  return comboKeys(combo)
    .map((k) => (k === "Super" ? "Win" : k))
    .join(" + ");
}

/** Free alternatives offered when a chord is taken by another app (SPEC §4: "a clear error
 *  and a suggestion"). Ordered by preference; chords used by another Liqlip action or just
 *  rejected are left out. Actions that are unset by default get none. */
const HOTKEY_SUGGESTIONS: Readonly<Record<HotkeyAction, readonly string[]>> = {
  summon: ["Ctrl + Alt + V", "Alt + Shift + C", "Ctrl + Shift + Space"],
  stackNext: ["Ctrl + Alt + Shift + V", "Alt + Shift + B", "Ctrl + Alt + B"],
  pauseToggle: [],
  ignoreNext: [],
  // Screenshot slots (W6-F): PrtSc variants first — a global chord on a letter would take
  // that key from every app. Free variants exclude chords another Liqlip slot uses, so when
  // the Snipping Tool holds PrtSc the region slot is offered Shift + PrtSc etc.
  shotRegion: ["Shift + PrtSc", "Ctrl + Shift + PrtSc", "Alt + Shift + S"],
  shotScreen: ["Ctrl + Shift + PrtSc", "Shift + PrtSc", "Alt + Shift + F"],
  shotWindow: [],
  shotRepeat: [],
  shotText: [],
};

export function hotkeySuggestions(action: HotkeyAction, current: HotkeySettings, rejected: readonly string[] = [], max = 2): string[] {
  return HOTKEY_SUGGESTIONS[action]
    .filter((c) => !rejected.includes(c) && hotkeyIssue(c, action, current) === null && current[action] !== c)
    .slice(0, max);
}

// ── screenshots (SPEC §9.7, ARCHITECTURE §18.10) ─────────────────────────────

/** Mirrors `settings::validate` (Rust ≡ mock): ≤ 120 characters, not empty, none of the
 *  characters Windows reserves in file names (learn.microsoft.com, "Naming Files, Paths, and
 *  Namespaces"). */
export const SHOT_FILE_NAME_MAX = 120;
const RESERVED_NAME_CHARS = /[<>:"/\\|?*]/;
export const SHOT_PLACEHOLDERS = ["{date}", "{time}", "{app}", "{n}"] as const;

export type ShotNameIssue = "empty" | "tooLong" | "reserved" | "control";

export function shotNameIssue(template: string): ShotNameIssue | null {
  // Rust trims first, then checks (`bad_name`: `char::is_control` — C0, DEL and C1).
  const v = template.trim();
  if (!v) return "empty";
  if ([...v].length > SHOT_FILE_NAME_MAX) return "tooLong";
  if (/[\u0000-\u001f\u007f-\u009f]/.test(v)) return "control";
  if (RESERVED_NAME_CHARS.test(v)) return "reserved";
  return null;
}

const pad2 = (n: number) => String(n).padStart(2, "0");

/**
 * The file name a template gives (live preview of "File name"; the real one is made by
 * `screenshot/files.rs` with the same rules): `{date}` → `2026-11-26`, `{time}` → `143005`
 * (the Windows Snipping Tool's `Screenshot 2026-11-26 143005` style — no `:`, which Windows
 * forbids in names), `{app}` → the source app, `{n}` → the shot counter. Other text — as typed.
 */
export function shotFileName(template: string, at: Date, app: string, n: number): string {
  const vars: Record<string, string> = {
    date: `${at.getFullYear()}-${pad2(at.getMonth() + 1)}-${pad2(at.getDate())}`,
    time: `${pad2(at.getHours())}${pad2(at.getMinutes())}${pad2(at.getSeconds())}`,
    app,
    n: String(n),
  };
  return template.trim().replace(/\{(date|time|app|n)\}/g, (_, k: string) => vars[k]);
}

/** JPG quality is a whole number 60–100 (`settings::validate`). */
export const JPG_QUALITY_MIN = 60;
export const JPG_QUALITY_MAX = 100;
export function jpgQualityValid(q: number): boolean {
  return Number.isInteger(q) && q >= JPG_QUALITY_MIN && q <= JPG_QUALITY_MAX;
}
/** Quality presets of the dropdown (a custom valid value stays selectable: `withCurrent`). */
export const JPG_QUALITY_PRESETS: readonly number[] = [60, 70, 80, 90, 95, 100];

/** "Hide thumbnails after" presets in seconds (0 = never), §9.10: the setting allows 0–3600. */
export const SHOT_TIMEOUT_PRESETS: readonly number[] = [5, 10, 30, 60, 0];
export function shotTimeoutValid(s: number): boolean {
  return Number.isInteger(s) && s >= 0 && s <= 3600;
}

/** The Snipping Tool warning (SPEC §9.1) matters only while a screenshot slot is bare PrtSc —
 *  the one key Windows takes; `Ctrl + PrtSc` and others still reach Liqlip. */
export function snippingConflict(snippingKey: boolean, hotkeys: HotkeySettings | null | undefined): boolean {
  return snippingKey && !!hotkeys && SHOT_HOTKEY_ACTIONS.some((a) => (hotkeys[a] ?? "").trim() === "PrtSc");
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

/**
 * Message key for an import / backup error (W2-B `Err("<code>: …")`), or `null` → the generic
 * `errorMessage`. A cancelled file dialog (`invalid: cancelled`) is `"cancelled"`: say nothing.
 */
export function importErrorKey(err: unknown): TranslationKey | "cancelled" | null {
  const raw = String(err ?? "");
  const m = raw.match(/^\s*(?:Error:\s*)?([a-z_]+):\s*(.*)$/s);
  if (!m) return null;
  const [, code, detail] = m;
  const d = detail.toLowerCase();
  if (code === "busy") return "settings.import.error.busy";
  if (code !== "invalid") return null;
  if (d === "cancelled") return "cancelled";
  if (d.includes("newer")) return "settings.import.error.newer";
  if (d.includes("not a liqlip backup")) return "settings.import.error.notBackup";
  if (d.includes("liquid tools")) return "settings.import.error.lt";
  if (d.includes("backup")) return "settings.import.error.backupDamaged";
  return null;
}
