// Mock of capture, picker/window, hotkey-recording, about, OCR and import commands
// (owner: W1-E; W0 baseline). OCR/import (W2) are implemented here so the settings UI can
// be built (`import_*` / `backup_*` follow the real W2-B commands); `?mock-unimpl=ocr,import`
// makes them reject "unimplemented".
//
// Shelf window in a browser tab (W1-E): there is no hotkey and no Rust window, so the mock
// plays `picker::summon` — the shelf's first `picker_target` call (made on mount, after its
// listeners are ready) emits `picker:layout` + `picker:open`; `picker_hide` and a pasting
// `clip_paste`/`text_paste` emit `picker:close`; Alt+V in the tab toggles the shelf like the
// global hotkey. Layout = `settings.picker.position/centerRow`, overridable per tab with
// `?mock-position=center|left|right|top|bottom` and `?mock-row=1` (centre "one row").
//
// OCR (W2-A): a mock queue over the seeded images and screenshots (see the OCR block below);
// `?mock-ocr=nolang` plays a PC without OCR language packs.
import type {
  AppMeta,
  BackupMode,
  CaptureStatus,
  ClipsChanged,
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
import { isImageKind } from "../types";
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

// Import / backup (W2-B): the same phases, events and report shapes as the Rust commands
// (ARCHITECTURE §16): `import:progress` folders → clips (backups: media → clips), then
// `clips:changed { origin: 'import', change: 'added' }` (replace: 'reset') without ids. The
// mock has no file dialogs: export "saves" `Documents\Liqlip-<today>.liqlip`, restore
// pretends a backup was picked. `?mock-unimpl=import` rejects all four as "unimplemented";
// `?mock-error=1` makes the probe fail like an unreadable database.
const LT_PROBE: ImportProbe = {
  found: true,
  path: "C:\\Users\\me\\AppData\\Roaming\\com.liquidtools.app\\clipboard\\clipboard.db",
  clips: 1240,
  favorites: 32,
  folders: 3,
};
// imported + skipped + errors = probe.clips; a re-run imports 0 and skips the rest.
const IMPORT_REPORT: ImportReport = { imported: 1185, skipped: 53, folders: 3, errors: 2 };
let ltImported = false;
let importBusy = false;

/** Phases one after another (`[phase, total]`), then `done()`; `busy:` while one runs. */
function phased<T>(phases: [string, number][], done: () => T): Promise<T> {
  if (importBusy) return Promise.reject(fail("busy", "an import or a backup is already running"));
  importBusy = true;
  return new Promise<T>((resolve) => {
    const next = (i: number) => {
      if (i >= phases.length) {
        importBusy = false;
        resolve(done());
        return;
      }
      const [phase, total] = phases[i];
      mockEmit<ImportProgress>("import:progress", { phase, done: 0, total });
      progress<ImportProgress>("import:progress", total, (n) => ({ phase, done: n, total }), () => next(i + 1));
    };
    next(0);
  });
}

function today(): string {
  const d = new Date(now());
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

// OCR (W2-A) — mirrors src-tauri/src/ocr (ARCHITECTURE §5a): the queue is the `pending`
// images; "recognising" one takes OCR_STEP_MS, writes `done` + text and emits
// `clips:changed { origin: 'user', change: 'updated', ids }`; a reindex sends
// `ocr:progress` from {0, total} to done == total ({0, 0} without images) and is never
// doubled; OCR turned off stops the queue (texts stay). `?mock-ocr=nolang` plays a PC
// without OCR language packs (`available: false`, reindex/recognition → "failed").

const OCR_LANGUAGES = ["en-US", "ru-RU"];
const OCR_STEP_MS = flags.latency ? 160 : 1;
/** Deterministic "recognised" texts for seeded images; some images have no text (""). */
const OCR_SAMPLES = [
  "Total due: $1,240.00\nInvoice #2026-118\nPay by 30 Nov 2026",
  "Ошибка 0x80070005: отказано в доступе\nПовторите попытку или обратитесь к администратору",
  "Build succeeded · 0 warnings · 12.4 s",
  "",
  "Meeting notes — Q4 roadmap\nShip OCR search · Import from Liquid Tools · Installer",
  "Пароль Wi-Fi гостевой сети: см. на ресепшене\nЭтаж 3, переговорная «Север»",
  "",
  "SELECT id, title FROM clips WHERE kind = 'image' ORDER BY copied_at DESC;",
];

const ocrNoLang = urlParam("mock-ocr") === "nolang";
let ocrReindex: { done: number; total: number } | null = null;
let ocrTimer: ReturnType<typeof setTimeout> | undefined;

const liveImages = () => db().clips.filter((c) => isImageKind(c.kind) && c.deletedAt === null);
const ocrPending = () => liveImages().filter((c) => c.ocr === "pending").length;
const ocrSample = (id: number) => OCR_SAMPLES[id % OCR_SAMPLES.length];

function emitOcrChanged(ids: number[]): void {
  mockEmit<ClipsChanged>("clips:changed", { origin: "user", change: "updated", ids });
}

/** One queue step: the newest pending image becomes `done`; reindex progress follows. */
function ocrStep(): void {
  ocrTimer = undefined;
  if (!currentSettings().ocr.enabled || ocrNoLang) return; // stopped; resumes on settings:changed
  const next = liveImages()
    .filter((c) => c.ocr === "pending")
    .sort((a, b) => b.copiedAt - a.copiedAt || b.id - a.id)[0];
  if (next) {
    next.ocr = "done";
    next.ocrText = next.ocrText ?? ocrSample(next.id);
    emitOcrChanged([next.id]);
  }
  if (ocrReindex) {
    const remaining = ocrPending();
    if (next) ocrReindex.done += 1;
    ocrReindex.total = Math.max(ocrReindex.total, ocrReindex.done + remaining);
    if (remaining === 0) {
      mockEmit<OcrProgress>("ocr:progress", { done: ocrReindex.total, total: ocrReindex.total });
      ocrReindex = null;
    } else {
      mockEmit<OcrProgress>("ocr:progress", { ...ocrReindex });
    }
  }
  if (ocrPending() > 0) ocrTimer = setTimeout(ocrStep, OCR_STEP_MS);
}

function ocrPump(): void {
  if (ocrTimer === undefined && ocrPending() > 0 && currentSettings().ocr.enabled && !ocrNoLang) {
    ocrTimer = setTimeout(ocrStep, OCR_STEP_MS);
  }
}

let ocrListening = false;
function ocrListen(): void {
  if (ocrListening) return;
  ocrListening = true;
  // OCR turned back on → never-queued images join the queue and it resumes (Rust: catch-up).
  let enabled = currentSettings().ocr.enabled;
  mockListen<Settings>("settings:changed", (s) => {
    if (s.ocr.enabled && !enabled) {
      for (const c of liveImages()) if (c.ocr === "none") c.ocr = "pending";
      ocrPump();
    }
    enabled = s.ocr.enabled;
  });
}

function ocrStatus(): OcrStatus {
  guard("ocr", "ocr_status");
  ocrListen();
  ocrPump();
  return ocrNoLang ? { available: false, languages: [], pending: ocrPending() } : { available: true, languages: [...OCR_LANGUAGES], pending: ocrPending() };
}

function ocrReindexCmd(): void {
  guard("ocr", "ocr_reindex");
  ocrListen();
  if (!currentSettings().ocr.enabled) throw fail("invalid", "text recognition is turned off");
  if (ocrReindex) {
    mockEmit<OcrProgress>("ocr:progress", { ...ocrReindex });
    return;
  }
  if (ocrNoLang) throw fail("failed", "no OCR language is installed");
  for (const c of liveImages()) c.ocr = "pending";
  const total = ocrPending();
  if (total === 0) {
    mockEmit<OcrProgress>("ocr:progress", { done: 0, total: 0 });
    return;
  }
  ocrReindex = { done: 0, total };
  mockEmit<OcrProgress>("ocr:progress", { done: 0, total });
  clearTimeout(ocrTimer);
  ocrTimer = setTimeout(ocrStep, OCR_STEP_MS);
}

async function clipOcr({ id }: Args): Promise<string> {
  guard("ocr", "clip_ocr");
  const find = () => db().clips.find((x) => x.id === id && x.deletedAt === null);
  const c = find();
  if (!c) throw fail("not_found", `clip ${String(id)}`);
  if (!isImageKind(c.kind)) throw fail("invalid", `clip ${String(id)} is not an image`);
  if (c.ocr === "done" && c.ocrText !== null) return c.ocrText;
  if (!currentSettings().ocr.enabled) throw fail("invalid", "text recognition is turned off");
  if (ocrNoLang) throw fail("failed", "no OCR language is installed");
  // Recognised "now", outside the queue (the real one takes ~0.1–2 s).
  if (flags.latency) await new Promise((r) => setTimeout(r, 350));
  if (!find()) throw fail("not_found", `clip ${String(id)}`); // deleted meanwhile → dropped
  c.ocr = "done";
  c.ocrText = c.ocrText ?? ocrSample(c.id);
  emitOcrChanged([c.id]);
  return c.ocrText;
}

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
  open_path: ({ which }) => {
    if (which !== "data" && which !== "logs" && which !== "shots") throw fail("invalid", `which ${String(which)}`);
  },
  pick_exe: () => "C:\\Program Files\\Example App\\example.exe",
  // INT-5: the screenshot folder (`?mock-shot-dialog=cancel` — the dialog is cancelled)
  pick_folder: () =>
    typeof location !== "undefined" && new URLSearchParams(location.search).get("mock-shot-dialog") === "cancel" ? null : "C:\\Users\\me\\Pictures\\Shots",

  ocr_status: ocrStatus,
  ocr_reindex: ocrReindexCmd,
  clip_ocr: clipOcr,

  import_lt_probe: (): ImportProbe => {
    guard("import", "import_lt_probe");
    if (flags.error) throw fail("failed", "mock-error");
    return { ...LT_PROBE };
  },
  import_lt_run: () => {
    guard("import", "import_lt_run");
    const report: ImportReport = ltImported
      ? { imported: 0, skipped: LT_PROBE.clips - IMPORT_REPORT.errors, folders: 0, errors: IMPORT_REPORT.errors }
      : { ...IMPORT_REPORT };
    return phased<ImportReport>(
      [
        ["folders", LT_PROBE.folders],
        ["clips", LT_PROBE.clips],
      ],
      () => {
        ltImported = true;
        mockEmit<ClipsChanged>("clips:changed", { origin: "import", change: "added" });
        return report;
      },
    );
  },
  backup_export: () => {
    guard("import", "backup_export");
    return phased<string>(
      [
        ["clips", 1],
        ["media", 120],
      ],
      () => `C:\\Users\\me\\Documents\\Liqlip-${today()}.liqlip`,
    );
  },
  backup_import: ({ mode }) => {
    guard("import", "backup_import");
    const merge = (mode as BackupMode) === "merge";
    const phases: [string, number][] = merge
      ? [
          ["media", 120],
          ["folders", 3],
          ["clips", 1240],
        ]
      : [
          ["media", 120],
          ["clips", 1],
        ];
    return phased<ImportReport>(phases, () => {
      mockEmit<ClipsChanged>("clips:changed", { origin: "import", change: merge ? "added" : "reset" });
      return merge ? { imported: 830, skipped: 410, folders: 1, errors: 0 } : { imported: 1240, skipped: 0, folders: 3, errors: 0 };
    });
  },
};
