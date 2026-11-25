// Actions the shelf layers add on top of `runAction` (actions.svelte.ts): OCR copy, new
// records from text (transform results, merges), paths and colour values, "More from this
// app", folder creation from the menu, and the confirmations of irreversible steps. Every
// failure ends in a toast with a readable reason — nothing fails silently.
import {
  clipCreateText,
  clipDetail,
  clipOcr,
  clipsDeleteRecent,
  clipsList,
  folderCreate,
  settingsOpen,
  snippetDelete,
} from "$lib/ipc/commands";
import { errorCode, errorMessage } from "$lib/ipc/errors";
import type { Clip, Snippet } from "$lib/ipc/types";
import { getLocale, t, type TranslationKey } from "$lib/i18n/index.svelte";
import { validateFolderName } from "../../settings/logic";
import { feed } from "../feed.svelte";
import { overlays } from "../overlays.svelte";
import { shelf } from "../shelf.svelte";
import { toasts } from "../toasts.svelte";
import { copyText, runAction } from "../actions.svelte";
import { pluralCategory } from "../format";
import { serializeToken } from "../query";
import { recordsText } from "./snippetText";
import { separatorPref } from "./separator.svelte";
import type { ConfirmCtx } from "./ConfirmDialog.svelte";

/** "3 items" / "3 записи" (plural-aware). */
export function itemsText(n: number): string {
  return t(`picker.count.items.${pluralCategory(n, getLocale())}` as TranslationKey, { n });
}

/** Show the feed's newest records (where a just-created record sits). */
function showNewest(): void {
  const before = shelf.sourceKey;
  shelf.setPill("all");
  shelf.clearSearch();
  if (shelf.sourceKey === before) shelf.showNew();
}

/** A record created from text (`clip_create_text`): toast with "Show". */
function announceCreated(message: string): void {
  toasts.show(message, { icon: "check", action: { label: t("picker.toast.show"), icon: "eye", run: showNewest } });
}

/** "Save as a new item" (transform results). */
export async function saveAsNew(text: string): Promise<Clip | null> {
  try {
    const clip = await clipCreateText(text);
    announceCreated(t("picker.toast.created"));
    return clip;
  } catch (e) {
    toasts.error(errorMessage(e));
    return null;
  }
}

/** "Merge into a new item": the selection's text joined by the remembered separator. */
export async function mergeIntoNew(ids: number[]): Promise<void> {
  try {
    const { text, skipped } = await recordsText(ids, separatorPref.text);
    if (!text) {
      toasts.error(t("picker.toast.mergeNothing"));
      return;
    }
    await clipCreateText(text);
    announceCreated(t(skipped ? "picker.toast.mergeSkipped" : "picker.toast.merged"));
  } catch (e) {
    toasts.error(errorMessage(e));
  }
}

/** "Copy path(s)" of a file record (one path per line). */
export async function copyPaths(id: number): Promise<void> {
  try {
    const d = await clipDetail(id);
    const paths = (d.files ?? []).map((f) => f.path);
    if (paths.length === 0) {
      toasts.error(t("picker.outcome.not_found"));
      return;
    }
    await copyText(paths.join("\r\n"), t(paths.length > 1 ? "picker.toast.pathsCopied" : "picker.toast.pathCopied"));
  } catch (e) {
    toasts.error(errorMessage(e));
  }
}

/** "Copy colour as HEX / RGB / HSL", and the value chips of Quick Look. */
export function copyColorValue(value: string): Promise<boolean> {
  return copyText(value, t("picker.toast.colorCopied", { value }));
}

/**
 * "Copy text from image" (SPEC §3.12): `clip_ocr` → `text_copy`. `""` = the image has no
 * text; `busy:` = recognition is saturated → "try again"; recognition off → a pointer to
 * Settings. Recognizing can take a moment, so a toast says so.
 */
export async function copyImageText(clip: Pick<Clip, "id" | "ocr">): Promise<void> {
  if (clip.ocr !== "done") toasts.show(t("picker.ocr.running"), { icon: "ocr", duration: 12_000 });
  let text: string;
  try {
    text = await clipOcr(clip.id);
  } catch (e) {
    const code = errorCode(e);
    if (code === "busy") {
      toasts.show(t("picker.ocr.busy"), { error: true, action: { label: t("common.retry"), icon: "refresh", run: () => void copyImageText(clip) } });
    } else if (code === "invalid") {
      toasts.show(t("picker.ocr.off"), {
        error: true,
        action: { label: t("picker.ocr.settings"), icon: "settings", run: () => void settingsOpen("ocr").catch(() => {}) },
      });
    } else if (code === "not_found") {
      toasts.error(t("picker.outcome.not_found"));
    } else {
      toasts.error(t("picker.ocr.failed"));
    }
    return;
  }
  if (!text.trim()) {
    toasts.show(t("picker.ocr.empty"), { icon: "info" });
    return;
  }
  await copyText(text, t("picker.ocr.copied"));
}

/** "More from this app": the feed shows everything copied from the record's app. */
export function moreFromApp(appId: number): void {
  const app = shelf.apps.get(appId);
  if (!app) return;
  const stem = (app.exe.split(/[\\/]/).pop() ?? app.exe).replace(/\.exe$/i, "");
  shelf.setPill("all");
  shelf.clearSearch();
  shelf.chips = [serializeToken("app", stem || app.name)];
  shelf.focusSearch(false);
}

/** Create a folder from the menu and move the records into it (the pill stays where it is).
 *  Returns an inline error key, or null on success. */
export async function createFolderAndMove(name: string, ids: number[]): Promise<TranslationKey | null> {
  const issue = validateFolderName(name, shelf.folders);
  if (issue) return issue === "tooLong" ? "picker.folder.error.invalid" : issue === "exists" ? "picker.folder.error.exists" : "picker.folder.error.invalid";
  try {
    const f = await folderCreate(name.trim());
    shelf.folders = [...shelf.folders, f];
    await runAction("moveToFolder", ids, { folderId: f.id });
    return null;
  } catch (e) {
    const raw = String(e);
    return errorCode(e) === "invalid" && raw.includes("exists") ? "picker.folder.error.exists" : "picker.folder.error.failed";
  }
}

// ── confirmations ────────────────────────────────────────────────────────────

const RECENT_PAGE = 200;
const RECENT_PAGES = 5;

/** How many records were copied in the last `minutes` (exact up to 1 000). */
async function countRecent(minutes: number): Promise<{ n: number; more: boolean }> {
  const since = Date.now() - minutes * 60_000;
  let cursor: string | null = null;
  let n = 0;
  for (let i = 0; i < RECENT_PAGES; i++) {
    const page = await clipsList({ text: "", exclude: [], filter: { since }, cursor, limit: RECENT_PAGE });
    n += page.items.length;
    cursor = page.nextCursor;
    if (!cursor) return { n, more: false };
  }
  return { n, more: true };
}

/** "Delete recent ▸ N minutes" — hard delete without undo (SPEC §2.4.6): always confirmed,
 *  with the number of records that will go. */
export function confirmDeleteRecent(minutes: number): void {
  const ctx: ConfirmCtx = {
    title: t(`picker.recent.title.${minutes}` as TranslationKey),
    icon: "clock",
    load: async () => {
      const { n, more } = await countRecent(minutes);
      if (n === 0) return { desc: t("picker.recent.descNone"), disabled: true };
      return { desc: t(more ? "picker.recent.descMany" : "picker.recent.desc", { count: itemsText(n) }) };
    },
    choices: [
      {
        label: t("picker.recent.confirm"),
        variant: "danger",
        icon: "trash",
        autofocus: true,
        run: () => void deleteRecent(minutes),
      },
    ],
  };
  overlays.open("confirm", ctx);
}

async function deleteRecent(minutes: number): Promise<void> {
  try {
    const n = await clipsDeleteRecent(minutes);
    toasts.show(t("picker.toast.deleted", { n }), { icon: "trash" });
    void shelf.refreshMeta();
  } catch (e) {
    toasts.error(errorMessage(e));
  }
}

/** Folder deletion (SPEC §3.9): keep the items in the history, or delete them too — both
 *  undoable from the toast. The safe choice is focused. */
export function confirmFolderDelete(folderId: number): void {
  const name = shelf.folderName(folderId);
  const count = shelf.counts?.folders[folderId] ?? shelf.folders.find((f) => f.id === folderId)?.count ?? 0;
  const run = (withClips: boolean) => () => void shelf.deleteFolder(folderId, withClips);
  const ctx: ConfirmCtx = {
    title: t("picker.folder.confirm.title", { name }),
    desc: count > 0 ? t("picker.folder.confirm.desc", { count: itemsText(count) }) : t("picker.folder.confirm.empty"),
    icon: "folderMinus",
    choices:
      count > 0
        ? [
            { label: t("picker.folder.confirm.keep"), variant: "secondary", icon: "folderMinus", autofocus: true, run: run(false) },
            { label: t("picker.folder.confirm.withClips"), variant: "danger", icon: "trash", run: run(true) },
          ]
        : [{ label: t("common.delete"), variant: "danger", icon: "trash", autofocus: true, run: run(false) }],
  };
  overlays.open("confirm", ctx);
}

/** Snippet deletion — permanent (the store keeps no undo for snippets). */
export function confirmSnippetDelete(s: Snippet): void {
  const ctx: ConfirmCtx = {
    title: t("settings.snippets.deleteTitle", { name: s.name }),
    desc: t("settings.snippets.deleteBody"),
    icon: "trash",
    choices: [
      {
        label: t("settings.snippets.delete"),
        variant: "danger",
        icon: "trash",
        autofocus: true,
        run: async () => {
          try {
            await snippetDelete(s.id);
            toasts.show(t("picker.toast.snippetDeleted"), { icon: "trash" });
          } catch (e) {
            toasts.error(errorMessage(e));
          }
        },
      },
    ],
  };
  overlays.open("confirm", ctx);
}

/** Leaving an edit with unsaved changes. "Keep editing" is focused (Enter keeps the work);
 *  `keep` runs when the user stays (to put the caret back into the editor). */
export function confirmDiscard(discard: () => void, keep?: () => void): void {
  const ctx: ConfirmCtx = {
    title: t("picker.discard.title"),
    desc: t("picker.discard.desc"),
    icon: "edit",
    cancelLabel: t("picker.discard.keep"),
    onCancel: keep,
    choices: [{ label: t("picker.discard.confirm"), variant: "danger", run: discard }],
  };
  overlays.open("confirm", ctx);
}

/** Records still in the feed among `ids` (a layer may outlive a deleted record). */
export function liveIds(ids: readonly number[]): number[] {
  return ids.filter((id) => feed.clip(id) !== undefined);
}
