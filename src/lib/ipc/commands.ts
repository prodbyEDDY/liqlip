// Typed wrappers — ONE function per §9.6 command (docs/ARCHITECTURE.md §9.6, §15).
// Hub file (coordinator). Inside Tauri every call goes to `invoke`; in a plain browser it
// goes to the in-memory mock (`./mock`), which implements every command.
// Argument objects use the camelCase keys Tauri maps onto the snake_case Rust parameters.
// Rejections are strings "<code>: <detail>" (code: not_found | busy | invalid | failed |
// unimplemented) — see `./errors.ts`.
import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "./runtime";
import type {
  AppInfo,
  AppMeta,
  BackupMode,
  CaptureStatus,
  ClearResult,
  ClearScope,
  Clip,
  ClipDetail,
  CopyRequest,
  Counts,
  DragOutcome,
  FeedQuery,
  Folder,
  ImportProbe,
  ImportReport,
  MonitorInfo,
  OcrStatus,
  Page,
  PasteOutcome,
  PasteRequest,
  PathKind,
  RunningApp,
  Settings,
  SettingsResult,
  Snippet,
  SnippetInput,
  Stack,
  StorageStats,
  TargetInfo,
  TextPasteRequest,
  UndoResult,
} from "./types";

export type CommandArgs = Record<string, unknown>;

/** Invoke a command in Tauri, or the mock in a browser. Command names are kept in sync
 *  with `src-tauri/src/lib.rs` and the mock router by `tests/contract.test.mjs`. */
export async function call<T>(cmd: string, args: CommandArgs = {}): Promise<T> {
  if (isTauri()) return invoke<T>(cmd, args);
  const { mockInvoke } = await import("./mock");
  return mockInvoke<T>(cmd, args);
}

// ── settings (W0, implemented) ──────────────────────────────────────

/** `settings_get` — Current settings (defaults come from Rust, never from the UI). */
export const settingsGet = (): Promise<Settings> => call<Settings>("settings_get");

/** `settings_set` — Apply a full Settings object; rejected fields are listed in `errors`. */
export const settingsSet = (settings: Settings): Promise<SettingsResult> =>
  call<SettingsResult>("settings_set", { settings });

/** `app_info` — Version and data/log folders. */
export const appInfo = (): Promise<AppMeta> => call<AppMeta>("app_info");

/** `open_path` — Open the data or log folder in Explorer. */
export const openPath = (which: PathKind): Promise<void> => call<void>("open_path", { which });

/** `pick_exe` — Native "choose an .exe" dialog; `null` = cancelled. */
export const pickExe = (): Promise<string | null> => call<string | null>("pick_exe");

// ── store (W1-A) ───────────────────────────────────────────────────

/** `clips_list` — One page of the feed (filters, free text, opaque cursor). */
export const clipsList = (q: FeedQuery): Promise<Page> =>
  call<Page>("clips_list", { q });

/** `clips_counts` — Pill counters. */
export const clipsCounts = (): Promise<Counts> =>
  call<Counts>("clips_counts");

/** `clip_detail` — Full record for Quick Look. */
export const clipDetail = (id: number): Promise<ClipDetail> =>
  call<ClipDetail>("clip_detail", { id });

/** `clip_set_favorite` — Mark / unmark favorites (new favorites go to the end of the manual order). */
export const clipSetFavorite = (ids: number[], value: boolean): Promise<void> =>
  call<void>("clip_set_favorite", { ids, value });

/** `clip_move_favorite` — Manual order inside the Favorites pill; `beforeId: null` = move to the end. */
export const clipMoveFavorite = (id: number, beforeId: number | null): Promise<void> =>
  call<void>("clip_move_favorite", { id, beforeId });

/** `clip_set_folder` — Move to a folder (`null` = remove from folder). */
export const clipSetFolder = (ids: number[], folderId: number | null): Promise<UndoResult> =>
  call<UndoResult>("clip_set_folder", { ids, folderId });

/** `clip_rename` — User title (`null` = automatic). */
export const clipRename = (id: number, title: string | null): Promise<Clip> =>
  call<Clip>("clip_rename", { id, title });

/** `clip_update_text` — Edit the text in place (re-hash; drops html/rtf). */
export const clipUpdateText = (id: number, text: string): Promise<Clip> =>
  call<Clip>("clip_update_text", { id, text });

/** `clip_create_text` — New record from text (merge, transform → save); not routed through capture. */
export const clipCreateText = (text: string, folderId: number | null = null): Promise<Clip> =>
  call<Clip>("clip_create_text", { text, folderId });

/** `clips_delete` — Soft delete with an undo token (2 min). */
export const clipsDelete = (ids: number[]): Promise<UndoResult> =>
  call<UndoResult>("clips_delete", { ids });

/** `clips_undo` — Undo a delete / move / clear; `false` if the token expired. */
export const clipsUndo = (token: string): Promise<boolean> =>
  call<boolean>("clips_undo", { token });

/** `clips_delete_recent` — Hard-delete everything copied in the last N minutes (no undo); returns the count. */
export const clipsDeleteRecent = (minutes: number): Promise<number> =>
  call<number>("clips_delete_recent", { minutes });

/** `clips_clear` — 'history' keeps favorites/folders/snippets (undoable); 'all' removes everything but snippets. */
export const clipsClear = (scope: ClearScope): Promise<ClearResult> =>
  call<ClearResult>("clips_clear", { scope });

/** `folders_list` — Folders in manual order, with counts. */
export const foldersList = (): Promise<Folder[]> =>
  call<Folder[]>("folders_list");

/** `folder_create` — Create a folder (`invalid: exists` on a case-insensitive name clash). */
export const folderCreate = (name: string): Promise<Folder> =>
  call<Folder>("folder_create", { name });

/** `folder_rename` — Rename a folder. */
export const folderRename = (id: number, name: string): Promise<Folder> =>
  call<Folder>("folder_rename", { id, name });

/** `folder_move` — Reorder folders; `beforeId: null` = move to the end. */
export const folderMove = (id: number, beforeId: number | null): Promise<void> =>
  call<void>("folder_move", { id, beforeId });

/** `folder_delete` — Delete a folder; its records stay in history unless `withClips`. */
export const folderDelete = (id: number, withClips: boolean): Promise<UndoResult> =>
  call<UndoResult>("folder_delete", { id, withClips });

/** `snippets_list` — Snippets matching `text` ("" = all), manual order. */
export const snippetsList = (text: string): Promise<Snippet[]> =>
  call<Snippet[]>("snippets_list", { text });

/** `snippet_save` — Create (no id) or update a snippet. */
export const snippetSave = (input: SnippetInput): Promise<Snippet> =>
  call<Snippet>("snippet_save", { input });

/** `snippet_delete` — Delete a snippet. */
export const snippetDelete = (id: number): Promise<void> =>
  call<void>("snippet_delete", { id });

/** `snippet_mark_used` — Bump a snippet's use counter. */
export const snippetMarkUsed = (id: number): Promise<void> =>
  call<void>("snippet_mark_used", { id });

/** `snippet_move` — Manual snippet order (W2-B); `beforeId: null` = move to the end. */
export const snippetMove = (id: number, beforeId: number | null): Promise<void> =>
  call<void>("snippet_move", { id, beforeId });

/** `apps_list` — Source applications with record counts. */
export const appsList = (): Promise<AppInfo[]> =>
  call<AppInfo[]>("apps_list");

/** `storage_stats` — Database / media sizes. */
export const storageStats = (): Promise<StorageStats> =>
  call<StorageStats>("storage_stats");

// ── paste (W1-C) ───────────────────────────────────────────────────

/** `clip_paste` — Paste records into the target window (or copy when there is none). */
export const clipPaste = (req: PasteRequest): Promise<PasteOutcome> =>
  call<PasteOutcome>("clip_paste", { req });

/** `clip_copy` — Put records on the clipboard only. */
export const clipCopy = (req: CopyRequest): Promise<PasteOutcome> =>
  call<PasteOutcome>("clip_copy", { req });

/** `text_paste` — Paste arbitrary text (snippets, transforms); `caretBack` = ← presses after. */
export const textPaste = (req: TextPasteRequest): Promise<PasteOutcome> =>
  call<PasteOutcome>("text_paste", { req });

/** `text_copy` — Put arbitrary text on the clipboard. */
export const textCopy = (text: string): Promise<void> =>
  call<void>("text_copy", { text });

/** `clip_open` — Open a link in the browser / a file with its default app. */
export const clipOpen = (id: number): Promise<void> =>
  call<void>("clip_open", { id });

/** `clip_open_with` — "Open with…" dialog for a file record. */
export const clipOpenWith = (id: number): Promise<void> =>
  call<void>("clip_open_with", { id });

/** `clip_reveal` — Show the file (or the stored image) in Explorer. */
export const clipReveal = (id: number): Promise<void> =>
  call<void>("clip_reveal", { id });

/** `clip_drag_out` — Native OLE drag of records out of the shelf. */
export const clipDragOut = (ids: number[]): Promise<DragOutcome> =>
  call<DragOutcome>("clip_drag_out", { ids });

/** `stack_get` — Current paste stack (safe read; empty until W1-C). */
export const stackGet = (): Promise<Stack> =>
  call<Stack>("stack_get");

/** `stack_add` — Append records to the paste stack. */
export const stackAdd = (ids: number[]): Promise<Stack> =>
  call<Stack>("stack_add", { ids });

/** `stack_remove` — Remove one stack item. */
export const stackRemove = (key: string): Promise<Stack> =>
  call<Stack>("stack_remove", { key });

/** `stack_move` — Reorder the stack; `beforeKey: null` = move to the end. */
export const stackMove = (key: string, beforeKey: string | null): Promise<Stack> =>
  call<Stack>("stack_move", { key, beforeKey });

/** `stack_reverse` — Reverse the stack order. */
export const stackReverse = (): Promise<Stack> =>
  call<Stack>("stack_reverse");

/** `stack_clear` — Empty the stack. */
export const stackClear = (): Promise<Stack> =>
  call<Stack>("stack_clear");

/** `stack_paste_all` — Paste the whole stack joined by `separator`. */
export const stackPasteAll = (separator: string): Promise<PasteOutcome> =>
  call<PasteOutcome>("stack_paste_all", { separator });

/** `stack_paste_next` — Paste the top item into the active window and pop it. */
export const stackPasteNext = (): Promise<PasteOutcome> =>
  call<PasteOutcome>("stack_paste_next");

// ── capture (W1-B) ─────────────────────────────────────────────────

/** `capture_status` — Recording state (safe read; "recording" until W1-B). */
export const captureStatus = (): Promise<CaptureStatus> =>
  call<CaptureStatus>("capture_status");

/** `capture_pause` — Pause recording for N minutes (`null` = until resumed). */
export const capturePause = (minutes: number | null): Promise<CaptureStatus> =>
  call<CaptureStatus>("capture_pause", { minutes });

/** `capture_resume` — Resume recording. */
export const captureResume = (): Promise<CaptureStatus> =>
  call<CaptureStatus>("capture_resume");

/** `capture_ignore_next` — Skip the next copy (one-shot). */
export const captureIgnoreNext = (): Promise<CaptureStatus> =>
  call<CaptureStatus>("capture_ignore_next");

/** `capture_set_collect` — "Collect copies into the paste stack" mode. */
export const captureSetCollect = (value: boolean): Promise<CaptureStatus> =>
  call<CaptureStatus>("capture_set_collect", { value });

/** `apps_running` — Running applications with windows (ignored-apps picker). */
export const appsRunning = (): Promise<RunningApp[]> =>
  call<RunningApp[]>("apps_running");

// ── picker (W1-D) ──────────────────────────────────────────────────

/** `picker_hide` — Close the shelf (Esc); `restoreFocus` returns focus to the target. */
export const pickerHide = (restoreFocus: boolean): Promise<void> =>
  call<void>("picker_hide", { restoreFocus });

/** `picker_make_interactive` — Hover-opened shelf → focusable + active (click into search). */
export const pickerMakeInteractive = (): Promise<void> =>
  call<void>("picker_make_interactive");

/** `picker_target` — The window the shelf will paste into (safe read; `null` until W1-C). */
export const pickerTarget = (): Promise<TargetInfo | null> =>
  call<TargetInfo | null>("picker_target");

/** `settings_open` — Show the settings window (+ `settings:navigate`). */
export const settingsOpen = (section: string | null = null): Promise<void> =>
  call<void>("settings_open", { section });

/** `monitors_list` — Monitors for the "Monitor" setting. */
export const monitorsList = (): Promise<MonitorInfo[]> =>
  call<MonitorInfo[]>("monitors_list");

// ── hotkeys (W1-D) ─────────────────────────────────────────────────

/** `hotkey_record_start` — Start native chord recording (result via `hotkeys:captured`). */
export const hotkeyRecordStart = (): Promise<void> =>
  call<void>("hotkey_record_start");

/** `hotkey_record_stop` — Stop native chord recording. */
export const hotkeyRecordStop = (): Promise<void> =>
  call<void>("hotkey_record_stop");

// ── ocr (W2-A) ─────────────────────────────────────────────────────

/** `ocr_status` — OCR availability, installed languages, queue length. */
export const ocrStatus = (): Promise<OcrStatus> =>
  call<OcrStatus>("ocr_status");

/** `ocr_reindex` — Re-run OCR on every image in the background (`ocr:progress`). */
export const ocrReindex = (): Promise<void> =>
  call<void>("ocr_reindex");

/** `clip_ocr` — Recognized text of one image record. */
export const clipOcr = (id: number): Promise<string> =>
  call<string>("clip_ocr", { id });

// ── import (W2-B) ──────────────────────────────────────────────────

/** `import_lt_probe` — Look for a Liquid Tools clipboard database. */
export const importLtProbe = (): Promise<ImportProbe> =>
  call<ImportProbe>("import_lt_probe");

/** `import_lt_run` — Import Liquid Tools history (idempotent; `import:progress`). */
export const importLtRun = (): Promise<ImportReport> =>
  call<ImportReport>("import_lt_run");

/** `backup_export` — Export a `.liqlip` backup; returns the path (`null` = cancelled). */
export const backupExport = (): Promise<string | null> =>
  call<string | null>("backup_export");

/** `backup_import` — Restore a backup (replace or merge). */
export const backupImport = (mode: BackupMode): Promise<ImportReport> =>
  call<ImportReport>("backup_import", { mode });
