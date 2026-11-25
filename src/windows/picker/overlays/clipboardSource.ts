// What {clipboard} expands to (SPEC §3.10: "the current clipboard text") — pure, with the IPC
// injected, so the decision is node-tested (tests/pickerClipboard). Semantics of
// `clipboard_text` (W3-A, FIX-09; coordinator's decision 2026-11-26):
//
//   · a string — the live CF_UNICODETEXT (also a password copied from KeePass: pasting the
//     snippet is an explicit action of the user; a record pasted by the stack hotkey). Any
//     string, even "", is the answer.
//   · null — the clipboard holds no text right now (an image, files, nothing): {clipboard} is
//     "" — never an older history record the user didn't copy last.
//   · an error (`busy: …` — the clipboard is locked or its owner hangs) — the fallback: the
//     newest text record of the history, unless the shelf itself wrote the clipboard after it
//     (a paste / copy from the shelf: then what it wrote). An unreadable history rejects too:
//     the caller offers "Paste anyway" instead of silently pasting an empty {clipboard}.
//
// The command is called at paste time only, and only when the body has {clipboard}. Previews
// (Quick Look, the snippet sheet) never read the clipboard: they show the history's latest
// copied text (`historyClipboardText`) — nothing on screen comes from a silent clipboard read.
import type { ClipDetail, FeedQuery, Kind, Page } from "$lib/ipc/types";

const TEXT_KINDS: readonly Kind[] = ["text", "code", "link", "contact", "color"];

export interface ClipboardIo {
  /** `clipboard_text`: the live clipboard text, `null` = no text there. */
  live: () => Promise<string | null>;
  list: (q: FeedQuery) => Promise<Page>;
  detail: (id: number) => Promise<ClipDetail>;
}

/** A clipboard write made by the shelf (records joined by `separator`, or a text). */
export interface ShelfWrite {
  at: number;
  ids?: number[];
  separator?: string;
  text?: string;
}

let lastWrite: ShelfWrite | null = null;

/** Remember a clipboard write by the shelf (records joined by `separator`, or a text). */
export function noteWrite(w: Omit<ShelfWrite, "at">, at = Date.now()): void {
  lastWrite = { at, ...w };
}

/** Forget the remembered write (tests). */
export function resetWrites(): void {
  lastWrite = null;
}

const textOf = (d: ClipDetail) => d.text ?? d.preview;

/** The history's idea of the clipboard text: the fallback when `clipboard_text` fails, and
 *  the {clipboard} sample of previews. */
export async function historyClipboardText(io: Pick<ClipboardIo, "list" | "detail">): Promise<string> {
  // One page is enough: with "favorites first" the newest record may not be the first one.
  const page = await io.list({ text: "", exclude: [], filter: {}, cursor: null, limit: 40 });
  let newest = page.items[0] ?? null;
  for (const c of page.items) if (c.copiedAt > (newest?.copiedAt ?? -Infinity)) newest = c;
  const own = lastWrite;
  if (own && (!newest || own.at >= newest.copiedAt)) {
    if (own.text !== undefined) return own.text;
    if (own.ids?.length) {
      const details = await Promise.all(own.ids.map((id) => io.detail(id).catch(() => null)));
      return details
        .filter((d): d is ClipDetail => d !== null && TEXT_KINDS.includes(d.kind))
        .map(textOf)
        .join(own.separator ?? "\n");
    }
  }
  if (!newest || !TEXT_KINDS.includes(newest.kind)) return "";
  return textOf(await io.detail(newest.id));
}

/** The text {clipboard} expands to when a snippet is pasted / copied right now. */
export async function resolveClipboardText(io: ClipboardIo): Promise<string> {
  let live: string | null;
  try {
    live = await io.live();
  } catch {
    // Unreadable right now (busy, a hung owner): the history decides.
    return historyClipboardText(io);
  }
  return typeof live === "string" ? live : "";
}
