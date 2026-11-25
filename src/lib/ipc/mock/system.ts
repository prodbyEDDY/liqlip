// Mock of capture, picker/window, hotkey-recording, about, OCR and import commands
// (owner: W1-E; W0 baseline). OCR/import (W2) are implemented here so the settings UI can
// be built; `?mock-unimpl=ocr,import` makes them reject "unimplemented" like the real
// app until W2.
//
// Shelf window in a browser tab (W1-E): there is no hotkey and no Rust window, so the mock
// plays `picker::summon` — the shelf's first `picker_target` call (made on mount, after its
// listeners are ready) emits `picker:layout` + `picker:open`; `picker_hide` and a pasting
// `clip_paste`/`text_paste` emit `picker:close`; Alt+V in the tab toggles the shelf like the
// global hotkey. Layout = `settings.picker.position/centerRow`, overridable per tab with
// `?mock-position=center|left|right|top|bottom` and `?mock-row=1` (centre "one row").
import type {
  AppMeta,
  BackupMode,
  CaptureStatus,
  ImportProbe,
  ImportProgress,
  ImportReport,
  MonitorInfo,
  OcrProgress,
  OcrStatus,
  PickerClose,
  PickerLayout,
  PickerOpen,
  Position,
  RunningApp,
  Settings,
  SettingsNavigate,
  TargetInfo,
} from "../types";
import { fail, flags, mockEmit, mockListen, now } from "./bus";
import { db, type Args } from "./clips";
import { currentSettings } from "./settings";

// ── capture state ────────────────────────────────────────────────────────────

const capture: CaptureStatus = { pausedUntil: null, ignoreNext: false, collectToStack: false };
let resumeTimer: ReturnType<typeof setTimeout> | undefined;

function emitCapture(): CaptureStatus {
  const s = { ...capture };
  mockEmit<CaptureStatus>("capture:status", s);
  return s;
}

/** Read by the capture simulator in clips.ts. */
export function captureFlags() {
  return {
    paused: capture.pausedUntil !== null,
    collectToStack: capture.collectToStack,
    consumeIgnoreNext(): boolean {
      if (!capture.ignoreNext) return false;
      capture.ignoreNext = false;
      emitCapture();
      return true;
    },
  };
}

function pause({ minutes }: Args): CaptureStatus {
  clearTimeout(resumeTimer);
  const m = minutes as number | null;
  if (m === null) {
    capture.pausedUntil = -1;
  } else {
    if (!(m > 0)) throw fail("invalid", "minutes");
    capture.pausedUntil = now() + m * 60_000;
    resumeTimer = setTimeout(resume, m * 60_000);
  }
  return emitCapture();
}

function resume(): CaptureStatus {
  clearTimeout(resumeTimer);
  capture.pausedUntil = null;
  return emitCapture();
}

// ── picker / window ──────────────────────────────────────────────────────────

const TARGET_ICON =
  "data:image/svg+xml;charset=utf-8," +
  encodeURIComponent(
    '<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"><rect x="2" y="2" width="60" height="60" rx="16" fill="#2b2b2b"/><text x="32" y="42" font-family="Segoe UI, Arial" font-size="30" font-weight="700" fill="#fff" text-anchor="middle">C</text></svg>',
  );

/** The window the shelf "would paste into" in the browser preview (`?mock-target=`). */
export function mockTarget(): TargetInfo | null {
  switch (flags.target) {
    case "none":
      return null;
    case "elevated":
      return { exe: "c:\\windows\\system32\\windowspowershell\\v1.0\\powershell.exe", name: "Windows PowerShell", title: "Administrator: Windows PowerShell", iconUrl: null, elevated: true };
    default:
      return { exe: "c:\\program files\\google\\chrome\\application\\chrome.exe", name: "Google Chrome", title: "Liqlip — Google Chrome", iconUrl: TARGET_ICON, elevated: false };
  }
}

// Panel sizes per position (SPEC §3.1 / ARCHITECTURE §7) + the shadow margin (BLEED).
const SIZES: Record<Position | "row", [number, number]> = {
  center: [760, 640],
  row: [760, 360],
  left: [400, 826],
  right: [400, 826],
  top: [800, 360],
  bottom: [800, 360],
};
const BLEED = 72;
const POSITIONS: readonly Position[] = ["center", "left", "right", "top", "bottom"];

function urlParam(name: string): string | null {
  if (typeof location === "undefined") return null;
  return new URLSearchParams(location.search).get(name);
}

const forcedPosition = ((p) => (p && (POSITIONS as readonly string[]).includes(p) ? (p as Position) : null))(urlParam("mock-position"));
const forcedRow = urlParam("mock-row") === "1";

/** The layout Rust would send for the current settings (or the URL override). */
export function mockLayout(s: Settings = currentSettings()): PickerLayout {
  const position = forcedPosition ?? s.picker.position;
  const centerRow = forcedPosition ? forcedRow : s.picker.centerRow;
  const [width, height] = SIZES[position === "center" && centerRow ? "row" : position];
  return { position, centerRow, width, height, bleed: BLEED };
}

const onPickerPage = () =>
  typeof location !== "undefined" && typeof window !== "undefined" && location.pathname.endsWith("picker.html");
let pickerBooted = false;
let pickerOpen = false;

function openPicker(): void {
  pickerOpen = true;
  mockEmit<PickerLayout>("picker:layout", mockLayout());
  mockEmit<PickerOpen>("picker:open", { mode: "summon", target: mockTarget(), interactive: true });
}

/** `picker::hide_now` / the Esc path: the shelf plays its exit animation. */
export function mockHidePicker(): void {
  if (!pickerOpen) return;
  pickerOpen = false;
  mockEmit<PickerClose>("picker:close", {});
}

function bootPicker(): void {
  if (pickerBooted || !onPickerPage()) return;
  pickerBooted = true;
  // Alt+V = the global summon hotkey (toggle). Capture phase: the shelf never sees it.
  window.addEventListener(
    "keydown",
    (e) => {
      if (e.code !== "KeyV" || !e.altKey || e.ctrlKey || e.shiftKey || e.metaKey) return;
      e.preventDefault();
      e.stopImmediatePropagation();
      if (pickerOpen) mockHidePicker();
      else openPicker();
    },
    true,
  );
  let last = JSON.stringify(mockLayout());
  mockListen<Settings>("settings:changed", (s) => {
    const next = mockLayout(s);
    if (JSON.stringify(next) === last) return;
    last = JSON.stringify(next);
    mockEmit<PickerLayout>("picker:layout", next);
  });
  openPicker();
}

const MONITORS: MonitorInfo[] = [
  { id: "\\\\.\\DISPLAY1", name: "DELL U2723QE", primary: true, width: 3840, height: 2160, scale: 1.5 },
  { id: "\\\\.\\DISPLAY2", name: "LG 24MP88", primary: false, width: 1920, height: 1080, scale: 1 },
];

// ── OCR / import (W2) ────────────────────────────────────────────────────────

function guard(group: "ocr" | "import", cmd: string): void {
  if (flags.unimpl.has(group)) throw fail("unimplemented", cmd);
}

function progress<T>(event: string, total: number, make: (done: number) => T, onDone: () => void): void {
  let done = 0;
  const step = () => {
    done = Math.min(total, done + Math.max(1, Math.round(total / 8)));
    mockEmit<T>(event, make(done));
    if (done < total) setTimeout(step, 250);
    else onDone();
  };
  setTimeout(step, 250);
}

const IMPORT_REPORT: ImportReport = { imported: 1187, skipped: 53, folders: 3, errors: 0 };

// ── router table ─────────────────────────────────────────────────────────────

export const systemHandlers: Record<string, (args: Args) => unknown> = {
  capture_status: () => ({ ...capture }),
  capture_pause: pause,
  capture_resume: resume,
  capture_ignore_next: () => {
    capture.ignoreNext = true;
    return emitCapture();
  },
  capture_set_collect: ({ value }) => {
    capture.collectToStack = Boolean(value);
    return emitCapture();
  },
  apps_running: (): RunningApp[] => {
    if (flags.error) throw fail("busy", "mock-error");
    return db().apps.map((a) => ({ exe: a.exe, name: a.name, iconUrl: a.iconUrl }));
  },

  picker_hide: () => mockHidePicker(),
  picker_make_interactive: () => undefined,
  picker_target: () => {
    bootPicker();
    return mockTarget();
  },
  settings_open: ({ section }) => {
    // Single page in the browser: only the settings page can react.
    if (typeof location !== "undefined" && location.pathname.endsWith("settings.html") && typeof section === "string") {
      mockEmit<SettingsNavigate>("settings:navigate", { section });
    }
  },
  monitors_list: () => {
    if (flags.error) throw fail("busy", "mock-error");
    return MONITORS.map((m) => ({ ...m }));
  },

  hotkey_record_start: () => undefined,
  hotkey_record_stop: () => undefined,

  app_info: (): AppMeta => ({
    version: "0.1.0",
    dataDir: "C:\\Users\\me\\AppData\\Roaming\\com.liqlip.app",
    logDir: "C:\\Users\\me\\AppData\\Roaming\\com.liqlip.app\\logs",
  }),
  open_path: () => undefined,
  pick_exe: () => "C:\\Program Files\\Example App\\example.exe",

  ocr_status: (): OcrStatus => {
    guard("ocr", "ocr_status");
    return { available: true, languages: ["en-US", "ru-RU"], pending: 0 };
  },
  ocr_reindex: () => {
    guard("ocr", "ocr_reindex");
    const total = db().clips.filter((c) => c.kind === "image" && c.deletedAt === null).length;
    progress<OcrProgress>("ocr:progress", Math.max(1, total), (done) => ({ done, total: Math.max(1, total) }), () => {});
  },
  clip_ocr: ({ id }) => {
    guard("ocr", "clip_ocr");
    const c = db().clips.find((x) => x.id === id && x.deletedAt === null);
    if (!c) throw fail("not_found", `clip ${String(id)}`);
    if (c.kind !== "image") throw fail("invalid", "not an image");
    return c.ocrText ?? "";
  },

  import_lt_probe: (): ImportProbe => {
    guard("import", "import_lt_probe");
    if (flags.error) throw fail("busy", "mock-error");
    return {
      found: true,
      path: "C:\\Users\\me\\AppData\\Roaming\\com.liquidtools.app\\clipboard\\clipboard.db",
      clips: 1240,
      favorites: 32,
      folders: 3,
    };
  },
  import_lt_run: () => {
    guard("import", "import_lt_run");
    return new Promise<ImportReport>((resolve) => {
      progress<ImportProgress>("import:progress", 1240, (done) => ({ phase: "clips", done, total: 1240 }), () => resolve({ ...IMPORT_REPORT }));
    });
  },
  backup_export: () => {
    guard("import", "backup_export");
    return "C:\\Users\\me\\Documents\\Liqlip backup 2026-11-26.liqlip";
  },
  backup_import: ({ mode }) => {
    guard("import", "backup_import");
    const m = mode as BackupMode;
    return { ...IMPORT_REPORT, skipped: m === "merge" ? 410 : 0 };
  },
};
