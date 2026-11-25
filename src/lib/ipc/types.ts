// ─────────────────────────────────────────────────────────────────────────────
// IPC CONTRACT — mirror of `src-tauri/src/model.rs` (docs/ARCHITECTURE.md §9).
// Hub file (coordinator). Changes are additive only, in BOTH files, and reported.
// Times are Unix milliseconds; ids are numbers. `T | null` ↔ Rust `Option<T>`;
// an optional field (`x?:`) ↔ `Option<T>` that is omitted when `None`.
// `tests/contract.test.mjs` checks this file against model.rs field by field.
// ─────────────────────────────────────────────────────────────────────────────

// §9.1 Enumerations

export type Kind = "text" | "link" | "code" | "color" | "contact" | "image" | "file";
export type Format = "text" | "html" | "rtf" | "png" | "files";
export type PasteMode = "auto" | "plain" | { format: Format };
export type Position = "center" | "left" | "right" | "top" | "bottom";
export type ChangeOrigin = "capture" | "user" | "import" | "retention";

/** All seven kinds, in pill/settings order. */
export const KINDS: readonly Kind[] = ["text", "link", "code", "color", "contact", "image", "file"];

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
export type PathKind = "data" | "logs";
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

export type HudKind = "copied" | "stack_empty" | "paused" | "elevated";

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
