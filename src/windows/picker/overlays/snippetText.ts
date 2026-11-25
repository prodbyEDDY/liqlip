// Snippet expansion on the shelf (SPEC §3.10): the shared pure expander (`$lib/snippets`,
// W1-F) fed with the current time, locale, a fresh UUID and the text for {clipboard}.
//
// {clipboard} = what is on the clipboard now: `clipboard_text` (FIX-09, W3) at paste time —
// null (no text) → "", an error → the history; previews use the history only. The decision is
// `clipboardSource.ts` (pure, node-tested).
import { clipboardText as readClipboard, clipDetail, clipsList } from "$lib/ipc/commands";
import type { Kind } from "$lib/ipc/types";
import { getLocale } from "$lib/i18n/index.svelte";
import { expandSnippet, parseSnippet, type ExpandResult, type PlaceholderToken } from "$lib/snippets/expand";
import { historyClipboardText, resolveClipboardText } from "./clipboardSource";

export { noteWrite } from "./clipboardSource";

const TEXT_KINDS: readonly Kind[] = ["text", "code", "link", "contact", "color"];

/** Full text of records joined by `separator` (text kinds only; images and files have none). */
export async function recordsText(ids: readonly number[], separator: string): Promise<{ text: string; skipped: number }> {
  const details = await Promise.all(ids.map((id) => clipDetail(id)));
  const parts: string[] = [];
  let skipped = 0;
  for (const d of details) {
    if (TEXT_KINDS.includes(d.kind)) parts.push(d.text ?? d.preview);
    else if (d.kind === "file" && d.files?.length) parts.push(d.files.map((f) => f.path).join("\r\n"));
    else skipped++;
  }
  return { text: parts.join(separator), skipped };
}

/** The text {clipboard} expands to when pasting right now (see the header); rejects only when
 *  the clipboard is unreadable and so is the history. */
export function clipboardText(): Promise<string> {
  return resolveClipboardText({ live: readClipboard, list: clipsList, detail: clipDetail });
}

/** The {clipboard} sample of a preview: the latest copied text from the history — a preview
 *  never reads the clipboard itself (it may hold a password the history never saw). */
export function previewClipboardText(): Promise<string> {
  return historyClipboardText({ list: clipsList, detail: clipDetail });
}

function uuid(): string {
  const c: Crypto = globalThis.crypto;
  // randomUUID needs a secure context; the shelf's origin is one, the fallback is for safety.
  if (typeof (c as { randomUUID?: unknown }).randomUUID === "function") return c.randomUUID();
  const b = new Uint8Array(16);
  c.getRandomValues(b);
  b[6] = (b[6] & 0x0f) | 0x40;
  b[8] = (b[8] & 0x3f) | 0x80;
  const h = [...b].map((x) => x.toString(16).padStart(2, "0")).join("");
  return `${h.slice(0, 8)}-${h.slice(8, 12)}-${h.slice(12, 16)}-${h.slice(16, 20)}-${h.slice(20)}`;
}

export interface SnippetExpansion extends ExpandResult {
  /** Placeholders that can't be filled (pasted as written). A second {cursor} isn't one. */
  issues: PlaceholderToken[];
  /** {clipboard} is used but neither the clipboard nor the history could be read (it
   *  expanded to ""). */
  clipboardFailed: boolean;
}

/** Placeholders of a body that can't be expanded. */
export function snippetIssues(body: string): PlaceholderToken[] {
  return parseSnippet(body).filter((t): t is PlaceholderToken => t.kind === "placeholder" && t.issue !== null && t.issue !== "cursor");
}

/** Does the body use {clipboard}? */
export function usesClipboard(body: string): boolean {
  return parseSnippet(body).some((t) => t.kind === "placeholder" && t.name === "clipboard" && t.issue === null);
}

/** Expand a snippet body for pasting / copying now. */
export async function expandNow(body: string): Promise<SnippetExpansion> {
  let clipboard = "";
  let clipboardFailed = false;
  if (usesClipboard(body)) {
    try {
      clipboard = await clipboardText();
    } catch {
      clipboardFailed = true;
    }
  }
  const r = expandSnippet(body, { now: Date.now(), clipboard, uuid, locale: getLocale() });
  return { ...r, issues: snippetIssues(body), clipboardFailed };
}

/** Expansion for a live preview (a fixed sample UUID, so the preview doesn't flicker). */
export function previewExpansion(body: string, clipboard: string, now: number, sampleUuid: string): ExpandResult {
  return expandSnippet(body, { now, clipboard, uuid: () => sampleUuid, locale: getLocale() });
}
