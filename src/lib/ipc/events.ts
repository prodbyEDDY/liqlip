// Typed subscriptions — ONE function per §9.7 event (docs/ARCHITECTURE.md §9.7).
// Hub file (coordinator). Inside Tauri they use `listen`; in a plain browser they
// subscribe to the mock event bus (`./mock/bus.ts`). Each returns a disposer.
import { listen } from "@tauri-apps/api/event";
import { isTauri } from "./runtime";
import type {
  CaptureStatus,
  ClipsChanged,
  HotkeysCaptured,
  HudShow,
  ImportProgress,
  OcrProgress,
  PickerClose,
  PickerLayout,
  PickerOpen,
  ResolvedTheme,
  Settings,
  SettingsNavigate,
  Stack,
} from "./types";

export type Unlisten = () => void;

/** Event names (mirror `src-tauri/src/events.rs`). */
export const EVENTS = {
  clipsChanged: "clips:changed",
  pickerOpen: "picker:open",
  pickerClose: "picker:close",
  pickerLayout: "picker:layout",
  captureStatus: "capture:status",
  stackChanged: "stack:changed",
  settingsChanged: "settings:changed",
  settingsNavigate: "settings:navigate",
  hotkeysCaptured: "hotkeys:captured",
  hudShow: "hud:show",
  ocrProgress: "ocr:progress",
  importProgress: "import:progress",
  themeChanged: "theme://changed",
} as const;

export type EventName = (typeof EVENTS)[keyof typeof EVENTS];

export async function on<T>(event: EventName, cb: (payload: T) => void): Promise<Unlisten> {
  if (isTauri()) return listen<T>(event, (e) => cb(e.payload));
  const { mockListen } = await import("./mock/bus");
  return mockListen<T>(event, cb);
}

/** store/capture → picker, settings. Show "New items" only for `origin: 'capture'`. */
export const onClipsChanged = (cb: (p: ClipsChanged) => void) => on(EVENTS.clipsChanged, cb);
/** picker → picker */
export const onPickerOpen = (cb: (p: PickerOpen) => void) => on(EVENTS.pickerOpen, cb);
/** picker → picker (release rows 260 ms later) */
export const onPickerClose = (cb: (p: PickerClose) => void) => on(EVENTS.pickerClose, cb);
/** picker → picker */
export const onPickerLayout = (cb: (p: PickerLayout) => void) => on(EVENTS.pickerLayout, cb);
/** capture → picker, settings */
export const onCaptureStatus = (cb: (p: CaptureStatus) => void) => on(EVENTS.captureStatus, cb);
/** paste → picker */
export const onStackChanged = (cb: (p: Stack) => void) => on(EVENTS.stackChanged, cb);
/** settings → all windows */
export const onSettingsChanged = (cb: (p: Settings) => void) => on(EVENTS.settingsChanged, cb);
/** picker (`settings_open`) → settings */
export const onSettingsNavigate = (cb: (p: SettingsNavigate) => void) => on(EVENTS.settingsNavigate, cb);
/** hotkeys → settings; `combo === ""` = recording cancelled */
export const onHotkeysCaptured = (cb: (p: HotkeysCaptured) => void) => on(EVENTS.hotkeysCaptured, cb);
/** picker/paste → hud */
export const onHudShow = (cb: (p: HudShow) => void) => on(EVENTS.hudShow, cb);
/** ocr → settings */
export const onOcrProgress = (cb: (p: OcrProgress) => void) => on(EVENTS.ocrProgress, cb);
/** import → settings */
export const onImportProgress = (cb: (p: ImportProgress) => void) => on(EVENTS.importProgress, cb);
/** theme → all windows (the resolved theme) */
export const onThemeChanged = (cb: (p: ResolvedTheme) => void) => on(EVENTS.themeChanged, cb);
