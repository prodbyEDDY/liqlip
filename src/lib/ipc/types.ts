// ─────────────────────────────────────────────────────────────────────────────
// IPC CONTRACT — mirror of `src-tauri/src/model.rs` (docs/ARCHITECTURE.md §9).
// Hub file (coordinator). Changes are additive only, in BOTH files, and reported.
// Times are Unix milliseconds; ids are numbers. `T | null` ↔ Rust `Option<T>`;
// an optional field (`x?:`) ↔ `Option<T>` that is omitted when `None`.
// `tests/contract.test.mjs` checks this file against model.rs field by field.
// ─────────────────────────────────────────────────────────────────────────────

// §9.1 Enumerations

/** `screenshot` — W5 (additive, ARCHITECTURE §18.5): behaves like `image` everywhere a record is
 *  pasted, copied, dragged out, recognised or previewed. */
export type Kind = "text" | "link" | "code" | "color" | "contact" | "image" | "file" | "screenshot";
export type Format = "text" | "html" | "rtf" | "png" | "files";
export type PasteMode = "auto" | "plain" | { format: Format };
export type Position = "center" | "left" | "right" | "top" | "bottom";
export type ChangeOrigin = "capture" | "user" | "import" | "retention";

/** All eight kinds, in pill/settings order (screenshots right after images, SPEC §9.6). */
export const KINDS: readonly Kind[] = ["text", "link", "code", "color", "contact", "image", "screenshot", "file"];

/** A picture record: `image` or `screenshot` (mirror of Rust `Kind::is_image`, §18.5). */
export const isImageKind = (k: Kind): boolean => k === "image" || k === "screenshot";

// §9.2 Records

export interface AppInfo {
  id: number;
  exe: string;
  name: string;
  iconUrl: string | null;
  clipCount?: number;
}

export interface ClipColor {
  hex: string;
  rgb: string;
  hsl: string;
}

export interface ClipMeta {
  /** link; html SourceURL */
  url?: string;
  host?: string;
  /** color */
  color?: ClipColor;
  contact?: "email" | "phone";
  /** file: extension of the first file */
  ext?: string;
  /** text truncated to 2 MB */
  truncated?: boolean;
  /** file: parent directory of the first file — the path tail on the card (FIX-08, W3) */
  dir?: string;
  /** screenshot / edited image (W5, §18.10): the last saved file and whether it was annotated */
  shot?: ShotMeta;
}

/** `ClipMeta.shot` (W5, ARCHITECTURE §18.10). */
export interface ShotMeta {
  /** last file written to the screenshot folder (`null` — never saved) */
  savedPath: string | null;
  /** the picture carries annotations (an `AnnotDoc` with objects is stored) */
  edited: boolean;
}

export type OcrState = "none" | "pending" | "done" | "error";

export interface Clip {
  id: number;
  kind: Kind;
  title: string | null;
  preview: string;
  appId: number | null;
  createdAt: number;
  copiedAt: number;
  copyCount: number;
  usedAt: number | null;
  useCount: number;
  favorite: boolean;
  folderId: number | null;
  size: number;
  width: number | null;
  height: number | null;
  charCount: number | null;
  fileCount: number | null;
  /** http://liq.localhost/thumb/<id>?v=<n> for image */
  thumbUrl: string | null;
  /** available for "Paste as…" */
  formats: Format[];
  meta: ClipMeta;
  ocr: OcrState;
}

export interface ClipFile {
  path: string;
  exists: boolean;
  size: number | null;
}

export interface ClipDetail extends Clip {
  /** full text */
  text: string | null;
  wordCount: number | null;
  lineCount: number | null;
  files: ClipFile[] | null;
  /** http://liq.localhost/media/<id> */
  imageUrl: string | null;
  ocrText: string | null;
}

export interface Folder {
  id: number;
  name: string;
  position: number;
  count: number;
}

export interface Snippet {
  id: number;
  name: string;
  body: string;
  keyword: string | null;
  position: number;
  createdAt: number;
  updatedAt: number;
  useCount: number;
}

export interface SnippetInput {
  id?: number;
  name: string;
  body: string;
  keyword?: string | null;
}

// §9.3 Feed queries

export interface Filter {
  kinds?: Kind[];
  favorites?: boolean;
  folderId?: number;
  appIds?: number[];
  since?: number;
  until?: number;
}

export interface FeedQuery {
  /** free text WITHOUT tokens (the front parses tokens: picker/query.ts) */
  text: string;
  /** minus-words */
  exclude: string[];
  filter: Filter;
  /** opaque */
  cursor: string | null;
  /** 40 */
  limit: number;
}

export interface Page {
  items: Clip[];
  nextCursor: string | null;
}

export interface Counts {
  all: number;
  kinds: Record<Kind, number>;
  favorites: number;
  snippets: number;
  folders: Record<number, number>;
}

// §9.4 Settings

export type Language = "system" | "en" | "ru";
export type ThemePref = "system" | "light" | "dark";
export type RememberSearch = "never" | "60s" | "always";
export type MergeSeparator = "newline" | "space" | "tab" | "none";
export type PasteChord = "ctrl+v" | "shift+insert" | "ctrl+shift+v";
export type Retention = "1d" | "1w" | "1m" | "3m" | "6m" | "1y" | "forever";

export interface PasteKey {
  exe: string;
  chord: PasteChord;
}

export interface Bounds {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface GeneralSettings {
  language: Language;
  theme: ThemePref;
  autostart: boolean;
  onboardingDone: boolean;
}

export interface PickerSettings {
  position: Position;
  centerRow: boolean;
  /** "cursor" | "primary" | monitor device name (MonitorInfo.id) */
  monitor: "cursor" | "primary" | (string & {});
  hoverOpen: boolean;
  hints: boolean;
  favoritesFirst: boolean;
  clickAction: "paste" | "copy";
  closeOnCopy: boolean;
  alwaysPlain: boolean;
  bumpOnPaste: boolean;
  rememberSearch: RememberSearch;
  mergeSeparator: MergeSeparator;
  pasteKeys: PasteKey[];
}

/** "" = not set. Notation as in Liquid Tools: "Alt + V". */
export interface HotkeySettings {
  summon: string;
  stackNext: string;
  pauseToggle: string;
  ignoreNext: string;
  /** W5 (§18.10): capture a region — default "PrtSc" */
  shotRegion: string;
  /** capture the monitor under the cursor — default "Ctrl + PrtSc" */
  shotScreen: string;
  /** capture the active window — default "" */
  shotWindow: string;
  /** repeat the last region — default "" */
  shotRepeat: string;
  /** copy text from the screen (OCR) — default "" */
  shotText: string;
}

export interface StorageSettings {
  retention: Retention;
  /** 0 = unlimited */
  maxEntries: number;
  /** 0 = unlimited */
  maxSizeMb: number;
  types: Record<Kind, boolean>;
}

export interface PrivacySettings {
  respectMarkers: boolean;
  ignoredApps: string[];
  secretPatterns: boolean;
}

export interface StackSettings {
  newlineAfterPaste: boolean;
}

export interface OcrSettings {
  enabled: boolean;
  /** "auto" | BCP-47 language tag of an installed OCR language */
  language: "auto" | (string & {});
}

export interface WindowSettings {
  settingsBounds: Bounds | null;
}

export type ShotFormat = "png" | "jpg";
export type ShotAfterCapture = "overlay" | "editor" | "none";
export type ShotCorner = "left" | "right";

/** `settings.screenshot` (W5, ARCHITECTURE §18.10; SPEC §9.7). */
export interface ScreenshotSettings {
  /** take the shot as soon as the region is released (skip the frame stage) — false */
  quick: boolean;
  /** magnifier at the cursor — true */
  magnifier: boolean;
  /** live W × H while selecting — true */
  dimensions: boolean;
  /** shutter click — true */
  sound: boolean;
  /** "Done" copies to the clipboard — true */
  copy: boolean;
  /** "Done" saves to the folder — true */
  save: boolean;
  /** "" = %USERPROFILE%\Pictures\Liqlip (FOLDERID_Pictures) */
  folder: string;
  /** file name template: {date} {time} {app} {n} — "Liqlip {date} {time}" */
  fileName: string;
  /** file format in the folder (history and clipboard are always PNG) — png */
  format: ShotFormat;
  /** 60–100 — 90 */
  jpgQuality: number;
  /** what follows a shot: the thumbnail stack (QAO), the editor window or nothing — overlay */
  afterCapture: ShotAfterCapture;
  /** QAO corner of the work area — left */
  corner: ShotCorner;
  /** QAO auto-hide, seconds; 0 = never — 10 */
  timeout: number;
}

export interface Settings {
  version: 1;
  general: GeneralSettings;
  picker: PickerSettings;
  hotkeys: HotkeySettings;
  storage: StorageSettings;
  privacy: PrivacySettings;
  stack: StackSettings;
  ocr: OcrSettings;
  window: WindowSettings;
  /** W5 (additive; older settings.json files get the defaults) */
  screenshot: ScreenshotSettings;
}

export interface SettingsError {
  /** dotted path, e.g. "hotkeys.summon" */
  field: string;
  code: "taken" | "invalid";
}

export interface SettingsResult {
  settings: Settings;
  errors: SettingsError[];
}

// §9.5 Other types

export interface TargetInfo {
  exe: string;
  name: string;
  title: string;
  iconUrl: string | null;
  elevated: boolean;
}

export interface PasteRequest {
  ids: number[];
  mode: PasteMode;
  separator?: string;
  keepOpen?: boolean;
}

export interface TextPasteRequest {
  text: string;
  plain: boolean;
  caretBack?: number;
  keepOpen?: boolean;
}

export type PasteResult = "pasted" | "copied" | "failed";
/** `partial` (W1-C): some records were left out (images in a joined text paste, non-files in a file paste). */
export type PasteReason = "no_target" | "elevated" | "focus_lost" | "clipboard_busy" | "not_found" | "stack_empty" | "partial";

export interface PasteOutcome {
  result: PasteResult;
  reason?: PasteReason;
}

export interface CaptureStatus {
  /** null = recording; -1 = paused until resumed; otherwise resume time (ms) */
  pausedUntil: number | null;
  ignoreNext: boolean;
  collectToStack: boolean;
}

export interface StackItem {
  key: string;
  clipId: number;
  kind: Kind;
  preview: string;
}

export interface Stack {
  items: StackItem[];
}

export interface StorageStats {
  clips: number;
  dbBytes: number;
  mediaBytes: number;
}

export interface ImportProbe {
  found: boolean;
  path: string | null;
  clips: number;
  favorites: number;
  folders: number;
}

export interface ImportReport {
  imported: number;
  skipped: number;
  folders: number;
  errors: number;
}

// Named shapes for the inline types of §9.6 (W0, additive)

/** clip_copy argument (`{ ids, mode, separator? }`). */
export interface CopyRequest {
  ids: number[];
  mode: PasteMode;
  separator?: string;
}

/** `{ undoToken }` — clip_set_folder, clips_delete, folder_delete. */
export interface UndoResult {
  undoToken: string;
}

/** `{ undoToken: string | null }` — clips_clear (undo only for 'history'). */
export interface ClearResult {
  undoToken: string | null;
}

export type ClearScope = "history" | "all";
export type DragOutcome = "dropped" | "cancelled";
/** `shots` — the screenshot folder (INT-5, additive). */
export type PathKind = "data" | "logs" | "shots";
export type BackupMode = "replace" | "merge";

/** apps_running item. */
export interface RunningApp {
  exe: string;
  name: string;
  iconUrl: string | null;
}

/** monitors_list item; `id` (device name) is what goes into settings.picker.monitor. */
export interface MonitorInfo {
  id: string;
  name: string;
  primary: boolean;
  width: number;
  height: number;
  scale: number;
}

/** app_info result. */
export interface AppMeta {
  version: string;
  dataDir: string;
  logDir: string;
}

/** ocr_status result. */
export interface OcrStatus {
  available: boolean;
  languages: string[];
  pending: number;
}

// §9.7 Event payloads (W0 names for the inline shapes)

export type ClipsChange = "added" | "updated" | "deleted" | "reset";

export interface ClipsChanged {
  origin: ChangeOrigin;
  change: ClipsChange;
  ids?: number[];
}

export interface PickerOpen {
  mode: "summon" | "hover";
  target: TargetInfo | null;
  interactive: boolean;
}

/** `{}` */
export type PickerClose = Record<string, never>;

/** Panel size in logical px (without bleed) + transparent shadow margin `bleed`. */
export interface PickerLayout {
  position: Position;
  centerRow: boolean;
  width: number;
  height: number;
  bleed: number;
}

export interface SettingsNavigate {
  section: string;
}

/** `combo` = recorded chord in "Alt + V" notation; "" = recording cancelled (Esc). */
export interface HotkeysCaptured {
  combo: string;
}

/** `resumed` / `skip_next` — W1-D, additive: feedback for the pause / skip hotkeys.
 *  `countdown` (text = seconds left) / `shot_failed` / `text_copied` / `text_empty` — W5, additive (§18.10). */
export type HudKind = "copied" | "stack_empty" | "paused" | "elevated" | "resumed" | "skip_next" | "countdown" | "shot_failed" | "text_copied" | "text_empty" | "text_unavailable" | "save_failed";

export interface HudShow {
  kind: HudKind;
  text?: string;
}

export interface OcrProgress {
  done: number;
  total: number;
}

export interface ImportProgress {
  phase: string;
  done: number;
  total: number;
}

export type ResolvedTheme = "light" | "dark";

// §9.10 Screenshots (W5, ARCHITECTURE §18.10). Geometry is in PHYSICAL pixels; `AnnotDoc`
// coordinates are pixels of the source picture. Rust keeps `AnnotDoc` opaque
// (`serde_json::Value`): it stores and passes the document, never interprets it.

export type ShotMode = "region" | "screen" | "window" | "repeat" | "text";
export type ShotAction = "done" | "copy" | "save" | "save_as" | "pin" | "text";
/** The modes that open the capture overlay. */
export type ShotSessionMode = "region" | "text";
/** What the overlay reports while a session is open (`shot_stage`). */
export type ShotStage = "select" | "frame";
export type ShotEditorAction = "done" | "copy" | "save_as" | "pin";
export type ShotAddedError = "save_failed";
/** `open_windows_settings` pages (`ms-settings:` URIs). */
export type WindowsSettingsPage = "keyboard";

/** A rectangle: physical px of a monitor frame (`ShotCommit.rect`, `lastRegion`) or source px (`AnnotDoc`). */
export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** One monitor of a session: physical px of the virtual screen + its frozen frame. */
export interface MonitorFrame {
  index: number;
  /** device name (as `MonitorInfo.id`) */
  id: string;
  x: number;
  y: number;
  width: number;
  height: number;
  scale: number;
  /** `http://liq.localhost/shot/<sessionId>/<index>.bmp` */
  frameUrl: string;
}

/** A window under the overlay (z-order: first = topmost), physical px of the monitor frame. */
export interface WinRect {
  x: number;
  y: number;
  w: number;
  h: number;
  children: WinRect[];
}

export interface ShotSession {
  id: number;
  mode: ShotSessionMode;
  monitor: MonitorFrame;
  windows: WinRect[];
  /** last region on this monitor (`R` in the overlay) */
  lastRegion: Rect | null;
  quick: boolean;
  magnifier: boolean;
  dimensions: boolean;
}

/** JSON part of the `shot_commit` body (§18.4): `[u32 LE N][N bytes JSON][RGBA width*height*4 | 0 bytes]`. */
export interface ShotCommit {
  sessionId: number;
  rect: Rect;
  action: ShotAction;
  doc: AnnotDoc | null;
  /** RGBA of the body (0 — no pixels: Rust crops the frame itself) */
  width: number;
  height: number;
}

export interface ShotCommitResult {
  clipId: number | null;
  savedPath: string | null;
  /** `action: 'text'` — the recognised text */
  text: string | null;
}

export interface ShotEditorPayload {
  clipId: number;
  name: string;
  /** `http://liq.localhost/base/<clipId>?v=<n>` — the picture without annotations */
  baseUrl: string;
  width: number;
  height: number;
  doc: AnnotDoc | null;
  dirtyGuard: boolean;
}

/** JSON part of the `shot_editor_commit` body (same framing as `ShotCommit`). */
export interface ShotEditorCommit {
  clipId: number;
  action: ShotEditorAction;
  doc: AnnotDoc;
  width: number;
  height: number;
}

export interface PinPayload {
  clipId: number;
  /** `http://liq.localhost/media/<clipId>?v=<n>` */
  imageUrl: string;
  width: number;
  height: number;
}

export interface ShotStatus {
  /** Windows sends PrtSc to the Snipping Tool (`PrintScreenKeyForSnippingEnabled`) */
  snippingKey: boolean;
  /** registration errors of the five screenshot hotkeys (`hotkeys.shot*`) */
  hotkeyErrors: SettingsError[];
}

// Event payloads (§18.10)

/** `shot:end` */
export interface ShotEnd {
  sessionId: number;
}

/** `shot:added` — `clip: null` when screenshots are not kept in history (`tempPath` then). */
export interface ShotAdded {
  clip: Clip | null;
  savedPath: string | null;
  tempPath: string | null;
  /** device name of the monitor the shot was taken on (QAO corner) */
  monitor: string;
  error: ShotAddedError | null;
}

// Annotation document v1 (ARCHITECTURE §18.6) — stored in `shot_docs.doc`; TS-only shape
// (`tests/contract.test.mjs` skips it: Rust holds it as `serde_json::Value`).

export type Pt = [number, number];
/** Thickness step: px = TABLE[size] * doc.unit, TABLE = [2, 3, 5, 8, 12]. */
export type Size = 1 | 2 | 3 | 4 | 5;
interface Obj {
  id: string;
  /** #rrggbb */
  color: string;
}
/** `counter.n = null` — automatic (previous + 1); `image.src` — `data:image/png;base64,…`.
 *  `arrow.bend` — signed offset (source px) of the curve's midpoint from the chord's midpoint,
 *  perpendicular to the chord (0 — straight). `rect.round` — rounded corners (INT-5, additive,
 *  optional: a document without it draws square corners; ignored on an ellipse). */
export type AnnotObject =
  | (Obj & { kind: "arrow"; a: Pt; b: Pt; bend: number; style: "straight" | "curved" | "double"; size: Size })
  | (Obj & { kind: "line"; a: Pt; b: Pt; size: Size })
  | (Obj & { kind: "rect" | "ellipse"; r: Rect; fill: boolean; size: Size; round?: boolean })
  | (Obj & { kind: "pen" | "marker"; points: Pt[]; size: Size })
  | (Obj & { kind: "text"; at: Pt; width: number | null; text: string; fontSize: number; style: "plain" | "badge" })
  | (Obj & { kind: "counter"; at: Pt; n: number | null; size: Size })
  | (Obj & { kind: "redact"; r: Rect; mode: "solid" | "pixelate" | "blur"; size: Size })
  | (Omit<Obj, "color"> & { kind: "spotlight"; r: Rect; shape: "rect" | "ellipse" })
  | (Omit<Obj, "color"> & { kind: "image"; r: Rect; src: string });

export interface AnnotDoc {
  v: 1;
  /** size of the source picture */
  w: number;
  h: number;
  /** source px per logical UI px when created (monitor scale; 1 — a picture from the clipboard) */
  unit: number;
  /** non-destructive crop (editor window) */
  crop: Rect | null;
  /** order inside a layer = z-order */
  objects: AnnotObject[];
}
