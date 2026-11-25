// `runAction(action, ids, opts)` — the ONE entry point for every action on records: keys,
// clicks, card buttons, drag-and-drop and the overlay layers (W2-C adds menu items and Quick
// Look by calling it). Mutations are optimistic (the click answers in the same frame) and
// roll back with a toast on failure; their `clips:changed` echo is ignored (`markOwn`).
// Paste results (`PasteOutcome`): `pasted` — Rust already hid the shelf; `copied` — a toast
// when the shelf stays visible (keepOpen); `failed` — a toast with the reason.
import {
  clipCopy,
  clipDragOut,
  clipMoveFavorite,
  clipOpen,
  clipOpenWith,
  clipPaste,
  clipReveal,
  clipSetFavorite,
  clipSetFolder,
  clipsDelete,
  snippetMarkUsed,
  stackAdd,
  textCopy,
  textPaste,
} from "$lib/ipc/commands";
import { errorMessage } from "$lib/ipc/errors";
import type { Clip, Kind, PasteMode, PasteOutcome, Snippet } from "$lib/ipc/types";
import { t, type TranslationKey } from "$lib/i18n/index.svelte";
import { feed } from "./feed.svelte";
import { selection } from "./selection.svelte";
import { shelf } from "./shelf.svelte";
import { toasts } from "./toasts.svelte";
import { folderIdOf } from "./pills";
import { overlays } from "./overlays.svelte";
import { separatorPref } from "./overlays/separator.svelte";
import { expandNow, noteWrite, type SnippetExpansion } from "./overlays/snippetText";

export type ActionName =
  | "paste" // into the target (copy when there is none)
  | "pastePlain" // CF_UNICODETEXT only
  | "pasteKeepOpen" // paste, keep the shelf open
  | "pasteAs" // opts.mode = { format }
  | "copy" // clipboard only; closes per settings.picker.closeOnCopy
  | "toggleFavorite"
  | "favorite" // set
  | "unfavorite" // clear
  | "delete" // soft delete + "Deleted N · Undo"
  | "moveToFolder" // opts.folderId
  | "removeFromFolder"
  | "rename" // opts.title (null = automatic) — F2 edits inline via shelf.startRename
  | "open" // link → browser, file → default app
  | "openWith" // files: "Open with…"
  | "reveal" // show in Explorer
  | "stack" // add to the paste stack
  | "dragOut"; // native drag out of the shelf

export interface ActionOptions {
  folderId?: number;
  title?: string | null;
  mode?: PasteMode;
  /** Separator text for several records (default: the remembered one, `separator.svelte`). */
  separator?: string;
}

const OPENABLE: readonly Kind[] = ["link", "file", "image"];
const REVEALABLE: readonly Kind[] = ["file", "image"];

let pasting = false;

function clipsOf(ids: readonly number[]): Clip[] {
  return ids.map((id) => feed.clip(id)).filter((c): c is Clip => c !== undefined);
}

function separator(ids: readonly number[], override?: string): string | undefined {
  return ids.length > 1 ? (override ?? separatorPref.text) : undefined;
}

/**
 * Toast for a paste/copy outcome (coordinator rules, W1-C acceptance): failures, `partial`,
 * `clipboard_busy`, `not_found` — always; `no_target` / `elevated` / `focus_lost` only while
 * the shelf stays visible (keepOpen, or no target with `closeOnCopy` off) — otherwise Rust
 * hides the shelf and shows the HUD.
 */
export function reportOutcome(o: PasteOutcome, keepOpen: boolean): void {
  if (o.result === "failed") {
    toasts.error(t(o.reason ? (`picker.outcome.${o.reason}` as TranslationKey) : "picker.outcome.failed"));
    if (o.reason === "not_found") void shelf.refreshMeta();
    return;
  }
  if (o.reason === "partial" || o.reason === "clipboard_busy" || o.reason === "not_found") {
    toasts.show(t(`picker.outcome.${o.result}_${o.reason}` as TranslationKey), { icon: "info" });
    return;
  }
  const closeOnCopy = shelf.settings?.picker.closeOnCopy ?? true;
  const visible = keepOpen || (o.reason === "no_target" && !closeOnCopy);
  if (!visible) return;
  if (o.result === "pasted") toasts.show(t("picker.outcome.pasted"));
  else toasts.show(t(o.reason ? (`picker.outcome.copied_${o.reason}` as TranslationKey) : "picker.outcome.copied"));
}

/** Paste through Rust's flow (ARCHITECTURE §6): it knows the live target, hides the shelf,
 *  copies instead when there is no target (per `closeOnCopy`) and honours `alwaysPlain`. */
async function paste(ids: number[], mode: PasteMode, keepOpen: boolean, sep?: string): Promise<void> {
  if (ids.length === 0 || pasting) return;
  pasting = true;
  try {
    const joint = separator(ids, sep);
    const o = await clipPaste({ ids, mode, separator: joint, keepOpen: keepOpen || undefined });
    if (o.result !== "failed") noteWrite({ ids, separator: joint });
    reportOutcome(o, keepOpen);
  } catch (e) {
    toasts.error(errorMessage(e));
  } finally {
    pasting = false;
  }
}

async function copy(ids: number[], mode: PasteMode = "auto", sep?: string): Promise<void> {
  if (ids.length === 0) return;
  try {
    const joint = separator(ids, sep);
    const o = await clipCopy({ ids, mode, separator: joint });
    if (o.result === "failed") return reportOutcome(o, true);
    noteWrite({ ids, separator: joint });
    if (shelf.settings?.picker.closeOnCopy ?? true) {
      await shelf.hide();
      return;
    }
    if (o.reason === "partial") reportOutcome(o, true);
    else toasts.show(ids.length > 1 ? t("picker.outcome.copiedMany", { n: ids.length }) : t("picker.outcome.copied"));
  } catch (e) {
    toasts.error(errorMessage(e));
  }
}

async function setFavorite(ids: number[], value: boolean): Promise<void> {
  const list = clipsOf(ids).filter((c) => c.favorite !== value);
  if (list.length === 0) return;
  const changed = list.map((c) => c.id);
  shelf.markOwn(changed);
  const leaveList = !value && shelf.pill === "favorites";
  const removed = leaveList ? shelf.dropRows(changed) : [];
  if (!leaveList) for (const c of list) feed.patch(c.id, { favorite: value });
  try {
    await clipSetFavorite(changed, value);
  } catch (e) {
    if (leaveList) feed.restore(removed);
    else for (const c of list) feed.patch(c.id, { favorite: c.favorite });
    toasts.error(errorMessage(e));
  }
}

async function remove(ids: number[]): Promise<void> {
  shelf.markOwn(ids);
  const removed = shelf.dropRows(ids);
  if (removed.length === 0) return;
  selection.clearMulti();
  try {
    const { undoToken } = await clipsDelete(removed.map((r) => r.clip.id));
    toasts.rememberUndo(undoToken);
    toasts.show(t("picker.toast.deleted", { n: removed.length }), {
      icon: "trash",
      action: { label: t("common.undo"), run: () => void shelf.undo() },
    });
  } catch (e) {
    feed.restore(removed);
    toasts.error(errorMessage(e));
  }
}

async function setFolder(ids: number[], folderId: number | null): Promise<void> {
  const list = clipsOf(ids).filter((c) => c.folderId !== folderId);
  if (list.length === 0) return;
  const changed = list.map((c) => c.id);
  shelf.markOwn(changed);
  const shown = folderIdOf(shelf.pill);
  const leaveList = shown !== null && shown !== folderId;
  const removed = leaveList ? shelf.dropRows(changed) : [];
  if (!leaveList) for (const c of list) feed.patch(c.id, { folderId });
  try {
    const { undoToken } = await clipSetFolder(changed, folderId);
    toasts.rememberUndo(undoToken);
    const message =
      folderId === null ? t("picker.toast.removedFromFolder") : t("picker.toast.moved", { name: shelf.folderName(folderId) });
    toasts.show(message, {
      icon: folderId === null ? "folderMinus" : "folder",
      action: { label: t("common.undo"), run: () => void shelf.undo() },
    });
  } catch (e) {
    if (leaveList) feed.restore(removed);
    else for (const c of list) feed.patch(c.id, { folderId: c.folderId });
    toasts.error(errorMessage(e));
  }
  void shelf.refreshMeta();
}

async function openLike(ids: number[], kinds: readonly Kind[], run: (id: number) => Promise<void>): Promise<void> {
  const c = clipsOf(ids)[0];
  if (!c) return;
  if (!kinds.includes(c.kind)) {
    toasts.error(t("picker.toast.cannotOpen"));
    return;
  }
  try {
    await run(c.id);
  } catch (e) {
    toasts.error(errorMessage(e));
  }
}

/** Run an action on records (ids in the order they should be pasted / acted on). */
export async function runAction(action: ActionName, ids: number[], opts: ActionOptions = {}): Promise<void> {
  if (ids.length === 0) return;
  switch (action) {
    case "paste":
      return paste(ids, "auto", false, opts.separator);
    case "pastePlain":
      return paste(ids, "plain", false, opts.separator);
    case "pasteKeepOpen":
      return paste(ids, "auto", true, opts.separator);
    case "pasteAs":
      return paste(ids, opts.mode ?? "auto", false, opts.separator);
    case "copy":
      return copy(ids, opts.mode ?? "auto", opts.separator);
    case "toggleFavorite": {
      const list = clipsOf(ids);
      return setFavorite(ids, list.some((c) => !c.favorite));
    }
    case "favorite":
      return setFavorite(ids, true);
    case "unfavorite":
      return setFavorite(ids, false);
    case "delete":
      return remove(ids);
    case "moveToFolder":
      if (opts.folderId === undefined) return;
      return setFolder(ids, opts.folderId);
    case "removeFromFolder":
      return setFolder(ids, null);
    case "rename":
      if (opts.title === undefined) return shelf.startRename(ids[0]);
      return shelf.commitRename(ids[0], opts.title ?? "");
    case "open":
      return openLike(ids, OPENABLE, clipOpen);
    case "openWith":
      return openLike(ids, ["file"], clipOpenWith);
    case "reveal":
      return openLike(ids, REVEALABLE, clipReveal);
    case "stack":
      try {
        const s = await stackAdd(ids);
        shelf.stack = s;
        toasts.show(t("picker.toast.stacked", { n: s.items.length }), {
          icon: "stack",
          action: { label: t("picker.stack.show"), icon: "stack", run: () => overlays.open("stack") },
        });
      } catch (e) {
        toasts.error(errorMessage(e));
      }
      return;
    case "dragOut":
      try {
        await clipDragOut(ids);
      } catch (e) {
        toasts.error(errorMessage(e));
      }
      return;
  }
}

/** Click / Enter on a card: paste when clicking pastes and there is a target, else copy. */
export function activate(id: number, byClick: boolean): Promise<void> {
  const ids = selection.multi && selection.has(id) ? shelf.targets() : [id];
  const clickPastes = (shelf.settings?.picker.clickAction ?? "paste") === "paste";
  if (byClick && !clickPastes) return runAction("copy", ids);
  return runAction("paste", ids);
}

/** Manual order inside the Favorites pill (drag a card before another). */
export async function moveFavorite(id: number, beforeId: number | null): Promise<void> {
  if (id === beforeId) return;
  shelf.markOwn([id]);
  const before = feed.items;
  feed.move(id, beforeId);
  try {
    await clipMoveFavorite(id, beforeId);
  } catch (e) {
    feed.items = before;
    toasts.error(errorMessage(e));
  }
}

// ── text (transform results, colour values, OCR text, paths) ──

/** Paste a text from the shelf (`text_paste`, through Rust's flow like a record). */
export async function pasteText(text: string, opts: { plain?: boolean; caretBack?: number; keepOpen?: boolean } = {}): Promise<boolean> {
  if (pasting) return false;
  pasting = true;
  try {
    const plain = opts.plain ?? true;
    const o = await textPaste({ text, plain, caretBack: opts.caretBack || undefined, keepOpen: opts.keepOpen || undefined });
    if (o.result !== "failed") noteWrite({ text });
    reportOutcome(o, opts.keepOpen ?? false);
    return o.result !== "failed";
  } catch (e) {
    toasts.error(errorMessage(e));
    return false;
  } finally {
    pasting = false;
  }
}

/** Copy a text; the shelf closes per `closeOnCopy`, otherwise `message` (or "Copied") toasts. */
export async function copyText(text: string, message?: string): Promise<boolean> {
  try {
    await textCopy(text);
    noteWrite({ text });
    if (shelf.settings?.picker.closeOnCopy ?? true) await shelf.hide();
    else toasts.show(message ?? t("picker.outcome.copied"));
    return true;
  } catch (e) {
    toasts.error(errorMessage(e));
    return false;
  }
}

// ── snippets: placeholders expand on paste / copy (`overlays/snippetText.ts`) ──

/**
 * Expand a snippet, or explain why not: a placeholder that can't be filled (unknown name, bad
 * format…) or an unreadable history for {clipboard} is shown as a toast with "Paste anyway"
 * (`proceed` runs with the text as expanded) — never silently pasted or swallowed.
 */
async function expandOrAsk(s: Snippet, proceed: (x: SnippetExpansion) => void): Promise<SnippetExpansion | null> {
  let x: SnippetExpansion;
  try {
    x = await expandNow(s.body);
  } catch (e) {
    toasts.error(errorMessage(e));
    return null;
  }
  const problem = x.clipboardFailed
    ? t("picker.snippet.clipboardFailed")
    : x.issues.length
      ? t(`settings.snippets.issue.${x.issues[0].issue}` as TranslationKey, { ph: x.issues[0].raw })
      : null;
  if (!problem) return x;
  toasts.show(problem, { error: true, duration: 8_000, action: { label: t("picker.snippet.pasteAnyway"), icon: "paste", run: () => proceed(x) } });
  return null;
}

async function pasteExpanded(s: Snippet, x: SnippetExpansion, plain: boolean, keepOpen: boolean): Promise<void> {
  if (pasting) return;
  pasting = true;
  toasts.dismiss();
  try {
    const o = await textPaste({
      text: x.text,
      plain: plain || (shelf.settings?.picker.alwaysPlain ?? false),
      caretBack: x.caretBack || undefined,
      keepOpen: keepOpen || undefined,
    });
    if (o.result !== "failed") {
      noteWrite({ text: x.text });
      void snippetMarkUsed(s.id).catch(() => {});
    }
    reportOutcome(o, keepOpen);
  } catch (e) {
    toasts.error(errorMessage(e));
  } finally {
    pasting = false;
  }
}

/** Paste a snippet with its placeholders expanded; the caret lands on {cursor}. */
export async function pasteSnippet(s: Snippet, plain: boolean, keepOpen = false): Promise<void> {
  if (pasting) return;
  const x = await expandOrAsk(s, (x) => void pasteExpanded(s, x, plain, keepOpen));
  if (x) await pasteExpanded(s, x, plain, keepOpen);
}

async function copyExpanded(s: Snippet, x: SnippetExpansion): Promise<void> {
  toasts.dismiss();
  if (await copyText(x.text)) void snippetMarkUsed(s.id).catch(() => {});
}

/** Copy a snippet with its placeholders expanded ({cursor} has no meaning here). */
export async function copySnippet(s: Snippet): Promise<void> {
  const x = await expandOrAsk(s, (x) => void copyExpanded(s, x));
  if (x) await copyExpanded(s, x);
}
