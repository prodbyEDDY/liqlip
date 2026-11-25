// The actions menu as data (SPEC §3.7): which items a record / a selection / a snippet gets,
// their groups, shortcuts and submenus — and the menu's search and shortcut matching. Pure
// (no Svelte, no IPC): `ActionsMenu.svelte` renders the nodes and runs their `cmd`.
// Node-tested (tests/pickerOverlays).
import type { Clip, Format, Kind, MergeSeparator } from "$lib/ipc/types";
import type { IconName } from "$lib/icons";
import type { TranslationKey } from "$lib/i18n/en";
import { TRANSFORM_GROUPS, type TransformId } from "../transforms";
import type { KeyInput } from "../keymap";

export type MenuCmd =
  | "paste"
  | "pastePlain"
  | "pasteAs"
  | "pasteKeepOpen"
  | "copy"
  | "copyAs"
  | "copyPath"
  | "copyColor"
  | "open"
  | "openWith"
  | "reveal"
  | "preview"
  | "ocr"
  | "saveSnippet"
  | "favorite"
  | "folder"
  | "folderNew"
  | "folderRemove"
  | "rename"
  | "edit"
  | "stack"
  | "showStack"
  | "moreFromApp"
  | "delete"
  | "deleteRecent"
  | "pasteAll"
  | "copyAll"
  | "merge"
  | "transform"
  | "transformPaste"
  | "transformCopy"
  | "transformSave"
  | "snippetEdit"
  | "snippetNew"
  | "snippetSettings"
  | "snippetDelete"
  | "submenu";

/** A key press that runs an item while the menu is open (physical keys, like the shelf). */
export interface Chord {
  code: readonly string[];
  ctrl?: boolean;
  shift?: boolean;
  alt?: boolean;
}

export interface MenuNode {
  /** Unique within the menu ("paste", "pasteAs.html", "folder.12", "t.upper.copy"). */
  id: string;
  cmd: MenuCmd;
  /** Argument of the command: a format, folder id, separator, minutes, transform id, colour. */
  arg?: string | number;
  label: TranslationKey;
  vars?: Record<string, string | number>;
  /** A literal label (a folder name) — wins over `label`. */
  text?: string;
  icon: IconName;
  /** A short mono glyph drawn instead of the icon ("Ab"). */
  glyph?: string;
  /** Shortcut shown on the right. */
  keys?: string;
  chord?: Chord;
  /** Second line. */
  hint?: TranslationKey;
  /** Mono value on the right (a colour in HEX / RGB / HSL). */
  value?: string;
  danger?: boolean;
  checked?: boolean;
  disabled?: boolean;
  gapBefore?: boolean;
  children?: MenuNode[];
  /** Left out of the flattened search (generic rows inside a transform). */
  noSearch?: boolean;
}

export type MenuClip = Pick<Clip, "id" | "kind" | "formats" | "favorite" | "folderId" | "meta" | "ocr" | "fileCount">;

export interface MenuInput {
  clips: readonly MenuClip[];
  /** The menu of a snippet (the Snippets pill). */
  snippet?: boolean;
  folders: readonly { id: number; name: string }[];
  /** Source app of a single record ("More from Chrome"). */
  appName?: string | null;
  ocrEnabled: boolean;
  /** The remembered separator of "Paste all". */
  separator: MergeSeparator;
}

/** Kinds whose content is text (transform, edit, save as snippet). */
export const TEXT_KINDS: readonly Kind[] = ["text", "code", "link", "contact", "color"];
export const SEPARATORS: readonly MergeSeparator[] = ["newline", "space", "tab", "none"];
export const RECENT_MINUTES: readonly number[] = [5, 15, 30, 60, 1440];
const OPENABLE: readonly Kind[] = ["link", "file", "image"];
const REVEALABLE: readonly Kind[] = ["file", "image"];

const FORMAT_ICON: Record<Format, IconName> = { text: "text", html: "code", rtf: "document", png: "image", files: "file" };
const FORMAT_ORDER: readonly Format[] = ["html", "rtf", "text", "png", "files"];

const TRANSFORM_LOOK: Record<TransformId, { icon: IconName; glyph?: string }> = {
  upper: { icon: "text", glyph: "AB" },
  lower: { icon: "text", glyph: "ab" },
  title: { icon: "text", glyph: "Ab" },
  sentence: { icon: "text", glyph: "Ab." },
  trim: { icon: "minus" },
  joinLines: { icon: "text" },
  dropEmpty: { icon: "x" },
  sortLines: { icon: "arrowsUpDown" },
  dedupeLines: { icon: "layers" },
  jsonFormat: { icon: "code" },
  jsonMinify: { icon: "code" },
  urlEncode: { icon: "link" },
  urlDecode: { icon: "link" },
  base64Encode: { icon: "hash" },
  base64Decode: { icon: "hash" },
  htmlToText: { icon: "document" },
  slug: { icon: "text", glyph: "a-b" },
};

const ENTER = ["Enter", "NumpadEnter"] as const;
const C = {
  shiftEnter: { code: ENTER, shift: true },
  ctrlEnter: { code: ENTER, ctrl: true },
  altEnter: { code: ENTER, alt: true },
  open: { code: ["KeyO"], ctrl: true },
  reveal: { code: ["KeyO"], ctrl: true, shift: true },
  preview: { code: ["KeyY"], ctrl: true },
  favorite: { code: ["KeyD"], ctrl: true },
  rename: { code: ["F2"] },
  edit: { code: ["KeyE"], ctrl: true },
  stack: { code: ["KeyS"], ctrl: true, shift: true },
  del: { code: ["Delete"] },
  save: { code: ["KeyS"], ctrl: true },
} satisfies Record<string, Chord>;

/** Group items; `null`/`false` entries are dropped, the first item of every non-first group
 *  gets a gap before it. */
function groups(...gs: (MenuNode | null | false | undefined)[][]): MenuNode[] {
  const out: MenuNode[] = [];
  for (const g of gs) {
    const items = g.filter((n): n is MenuNode => !!n);
    items.forEach((n, i) => out.push(i === 0 && out.length > 0 ? { ...n, gapBefore: true } : n));
  }
  return out;
}

function folderMenu(clips: readonly MenuClip[], folders: MenuInput["folders"]): MenuNode {
  const inFolder = clips.some((c) => c.folderId !== null);
  const children: MenuNode[] = folders.map((f) => ({
    id: `folder.${f.id}`,
    cmd: "folder",
    arg: f.id,
    label: "picker.menu.folder",
    text: f.name,
    icon: "folder",
    checked: clips.length > 0 && clips.every((c) => c.folderId === f.id),
  }));
  children.push({ id: "folderNew", cmd: "folderNew", label: "picker.menu.folderNew", icon: "folderPlus", gapBefore: children.length > 0 });
  if (inFolder) children.push({ id: "folderRemove", cmd: "folderRemove", label: "picker.menu.folderRemove", icon: "folderMinus" });
  return { id: "folder", cmd: "submenu", label: "picker.menu.folder", icon: "folder", children };
}

function stackItems(): MenuNode[] {
  return [
    { id: "stack", cmd: "stack", label: "picker.menu.stack", icon: "stack", keys: "Ctrl ⇧ S", chord: C.stack },
    { id: "showStack", cmd: "showStack", label: "picker.menu.showStack", icon: "stack" },
  ];
}

/** The rows of a transform: what to do with its result. */
function transformActions(id: TransformId): MenuNode[] {
  return [
    { id: `t.${id}.paste`, cmd: "transformPaste", arg: id, label: "picker.transform.paste", icon: "paste", keys: "↵", noSearch: true },
    { id: `t.${id}.copy`, cmd: "transformCopy", arg: id, label: "picker.transform.copy", icon: "copy", keys: "Ctrl ↵", chord: C.ctrlEnter, noSearch: true },
    { id: `t.${id}.save`, cmd: "transformSave", arg: id, label: "picker.transform.save", icon: "plus", keys: "Ctrl S", chord: C.save, noSearch: true },
  ];
}

export function transformMenu(): MenuNode {
  const children = TRANSFORM_GROUPS.flatMap((g, gi) =>
    g.map(
      (id, i): MenuNode => ({
        id: `t.${id}`,
        cmd: "transform",
        arg: id,
        label: `picker.transform.${id}` as TranslationKey,
        icon: TRANSFORM_LOOK[id].icon,
        glyph: TRANSFORM_LOOK[id].glyph,
        gapBefore: gi > 0 && i === 0,
        children: transformActions(id),
      }),
    ),
  );
  return { id: "transform", cmd: "submenu", label: "picker.menu.transform", icon: "sparkles", children };
}

function formatMenu(id: "pasteAs" | "copyAs", formats: readonly Format[]): MenuNode {
  const list = FORMAT_ORDER.filter((f) => formats.includes(f));
  return {
    id,
    cmd: "submenu",
    label: id === "pasteAs" ? "picker.menu.pasteAs" : "picker.menu.copyAs",
    icon: id === "pasteAs" ? "paste" : "copy",
    children: list.map((f) => ({
      id: `${id}.${f}`,
      cmd: id,
      arg: f,
      label: `picker.menu.format.${f}` as TranslationKey,
      icon: FORMAT_ICON[f],
    })),
  };
}

function recentMenu(): MenuNode {
  return {
    id: "deleteRecent",
    cmd: "submenu",
    label: "picker.menu.deleteRecent",
    icon: "clock",
    danger: true,
    children: RECENT_MINUTES.map((m) => ({
      id: `recent.${m}`,
      cmd: "deleteRecent",
      arg: m,
      label: `picker.menu.recent.${m}` as TranslationKey,
      icon: "clock",
      danger: true,
    })),
  };
}

function singleMenu(c: MenuClip, input: MenuInput): MenuNode[] {
  const text = TEXT_KINDS.includes(c.kind);
  const rich = c.formats.length > 1;
  const color = c.kind === "color" ? c.meta.color : undefined;
  const colorRows: MenuNode[] = color
    ? (["hex", "rgb", "hsl"] as const).map((f) => ({
        id: `copyColor.${f}`,
        cmd: "copyColor",
        arg: color[f],
        label: `picker.menu.copyColor.${f}` as TranslationKey,
        icon: "palette",
        value: color[f],
      }))
    : [];
  const ocrOff = !input.ocrEnabled && c.ocr !== "done";
  return groups(
    [
      { id: "paste", cmd: "paste", label: "picker.menu.paste", icon: "paste", keys: "↵" },
      c.kind !== "image" && { id: "pastePlain", cmd: "pastePlain", label: "picker.menu.pastePlain", icon: "text", keys: "⇧ ↵", chord: C.shiftEnter },
      rich && formatMenu("pasteAs", c.formats),
      { id: "pasteKeepOpen", cmd: "pasteKeepOpen", label: "picker.menu.pasteKeepOpen", icon: "paste", keys: "Alt ↵", chord: C.altEnter },
    ],
    [
      { id: "copy", cmd: "copy", label: "picker.menu.copy", icon: "copy", keys: "Ctrl ↵", chord: C.ctrlEnter },
      rich && formatMenu("copyAs", c.formats),
      c.kind === "file" && {
        id: "copyPath",
        cmd: "copyPath",
        label: (c.fileCount ?? 1) > 1 ? "picker.menu.copyPaths" : "picker.menu.copyPath",
        icon: "file",
      },
      ...colorRows,
    ],
    [
      { id: "preview", cmd: "preview", label: "picker.menu.preview", icon: "eye", keys: "Space", chord: C.preview },
      OPENABLE.includes(c.kind) && { id: "open", cmd: "open", label: "picker.menu.open", icon: "external", keys: "Ctrl O", chord: C.open },
      c.kind === "file" && { id: "openWith", cmd: "openWith", label: "picker.menu.openWith", icon: "appWindow" },
      REVEALABLE.includes(c.kind) && { id: "reveal", cmd: "reveal", label: "picker.menu.reveal", icon: "folderOpen", keys: "Ctrl ⇧ O", chord: C.reveal },
    ],
    [
      text && transformMenu(),
      c.kind === "image" && {
        id: "ocr",
        cmd: "ocr",
        label: "picker.menu.ocr",
        icon: "ocr",
        disabled: ocrOff,
        hint: ocrOff ? "picker.menu.ocrOff" : undefined,
      },
      text && { id: "saveSnippet", cmd: "saveSnippet", label: "picker.menu.saveSnippet", icon: "document" },
    ],
    [
      {
        id: "favorite",
        cmd: "favorite",
        label: c.favorite ? "picker.menu.unfavorite" : "picker.menu.favorite",
        icon: "heart",
        keys: "Ctrl D",
        chord: C.favorite,
      },
      folderMenu([c], input.folders),
      { id: "rename", cmd: "rename", label: "picker.menu.rename", icon: "pen", keys: "F2", chord: C.rename },
      text && { id: "edit", cmd: "edit", label: "picker.menu.edit", icon: "edit", keys: "Ctrl E", chord: C.edit },
    ],
    stackItems(),
    [!!input.appName && { id: "moreFromApp", cmd: "moreFromApp", label: "picker.menu.moreFromApp", vars: { app: input.appName }, icon: "appWindow" }],
    [{ id: "delete", cmd: "delete", label: "picker.menu.delete", icon: "trash", keys: "Del", chord: C.del, danger: true }, recentMenu()],
  );
}

function multiMenu(clips: readonly MenuClip[], input: MenuInput): MenuNode[] {
  const allFav = clips.every((c) => c.favorite);
  const sepKey = (s: MergeSeparator) => `picker.menu.sep.${s}` as TranslationKey;
  const mergeable = clips.some((c) => c.kind !== "image");
  return groups(
    [
      { id: "pasteAll", cmd: "pasteAll", arg: input.separator, label: "picker.menu.pasteAll", icon: "paste", keys: "↵", hint: sepKey(input.separator) },
      {
        id: "pasteAllWith",
        cmd: "submenu",
        label: "picker.menu.pasteAllWith",
        icon: "paste",
        children: SEPARATORS.map((s) => ({
          id: `sep.${s}`,
          cmd: "pasteAll",
          arg: s,
          label: sepKey(s),
          icon: "paste",
          checked: s === input.separator,
        })),
      },
    ],
    [
      { id: "copyAll", cmd: "copyAll", arg: input.separator, label: "picker.menu.copyAll", icon: "copy", keys: "Ctrl ↵", chord: C.ctrlEnter, hint: sepKey(input.separator) },
      mergeable && { id: "merge", cmd: "merge", label: "picker.menu.merge", icon: "plus" },
    ],
    [
      {
        id: "favorite",
        cmd: "favorite",
        label: allFav ? "picker.menu.unfavorite" : "picker.menu.favorite",
        icon: "heart",
        keys: "Ctrl D",
        chord: C.favorite,
      },
      folderMenu(clips, input.folders),
    ],
    stackItems(),
    [{ id: "delete", cmd: "delete", label: "picker.menu.deleteMany", vars: { n: clips.length }, icon: "trash", keys: "Del", chord: C.del, danger: true }],
  );
}

function snippetMenu(): MenuNode[] {
  return groups(
    [
      { id: "paste", cmd: "paste", label: "picker.menu.paste", icon: "paste", keys: "↵" },
      { id: "pastePlain", cmd: "pastePlain", label: "picker.menu.pastePlain", icon: "text", keys: "⇧ ↵", chord: C.shiftEnter },
      { id: "pasteKeepOpen", cmd: "pasteKeepOpen", label: "picker.menu.pasteKeepOpen", icon: "paste", keys: "Alt ↵", chord: C.altEnter },
    ],
    [
      { id: "copy", cmd: "copy", label: "picker.menu.copy", icon: "copy", keys: "Ctrl ↵", chord: C.ctrlEnter },
      { id: "preview", cmd: "preview", label: "picker.menu.preview", icon: "eye", keys: "Space", chord: C.preview },
    ],
    [
      { id: "snippetEdit", cmd: "snippetEdit", label: "picker.menu.snippetEdit", icon: "edit", keys: "Ctrl E", chord: C.edit },
      { id: "snippetNew", cmd: "snippetNew", label: "picker.menu.snippetNew", icon: "plus" },
      { id: "snippetSettings", cmd: "snippetSettings", label: "picker.menu.snippetSettings", icon: "settings" },
    ],
    // Delete asks first (no undo for snippets) — the same as Delete on a snippet card.
    [{ id: "snippetDelete", cmd: "snippetDelete", label: "picker.menu.snippetDelete", icon: "trash", keys: "Del", chord: C.del, danger: true }],
  );
}

/** The root items of the actions menu for its context. */
export function buildMenu(input: MenuInput): MenuNode[] {
  if (input.snippet) return snippetMenu();
  if (input.clips.length === 0) return [];
  if (input.clips.length === 1) return singleMenu(input.clips[0], input);
  return multiMenu(input.clips, input);
}

// ── search ───────────────────────────────────────────────────────────────────

export interface SearchHit {
  node: MenuNode;
  /** Submenus from the searched level down to the node's parent. */
  path: MenuNode[];
}

/** Lower-case, ё → е, single spaces. */
export function normalize(s: string): string {
  return s.toLocaleLowerCase().replace(/ё/g, "е").replace(/\s+/g, " ").trim();
}

function score(label: string, words: readonly string[]): number {
  if (!words.every((w) => label.includes(w))) return -1;
  const first = words[0];
  if (label.startsWith(first)) return 0;
  if (new RegExp(`(^|[\\s\\-:(«"'/])${first.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}`).test(label)) return 1;
  return 2;
}

/**
 * Items matching `query` at this level and inside its submenus (up to `depth` levels):
 * every query word must occur in one of the item's labels (the localized one, plus any
 * extra the caller gives — e.g. the English label in the Russian UI). Order: label prefix,
 * then word prefix, then substring; shallower first; menu order last.
 */
export function searchMenu(nodes: readonly MenuNode[], query: string, labels: (n: MenuNode) => readonly string[], depth = 2): SearchHit[] {
  const words = normalize(query).split(" ").filter(Boolean);
  if (words.length === 0) return nodes.map((node) => ({ node, path: [] }));
  const hits: { hit: SearchHit; s: number; d: number; o: number }[] = [];
  let order = 0;
  const visit = (list: readonly MenuNode[], path: MenuNode[]) => {
    for (const node of list) {
      order++;
      if (node.noSearch) continue;
      let best = -1;
      for (const l of labels(node)) {
        const s = score(normalize(l), words);
        if (s >= 0 && (best < 0 || s < best)) best = s;
      }
      if (best >= 0) hits.push({ hit: { node, path }, s: best, d: path.length, o: order });
      if (node.children && path.length + 1 < depth) visit(node.children, [...path, node]);
    }
  };
  visit(nodes, []);
  return hits.sort((a, b) => a.s - b.s || a.d - b.d || a.o - b.o).map((h) => h.hit);
}

/** The node whose chord this key press is (enabled nodes of `nodes` only). */
export function chordNode(nodes: readonly MenuNode[], e: KeyInput): MenuNode | null {
  if (e.metaKey) return null;
  for (const n of nodes) {
    const c = n.chord;
    if (!c || n.disabled) continue;
    if (c.code.includes(e.code) && !!c.ctrl === e.ctrlKey && !!c.shift === e.shiftKey && !!c.alt === e.altKey) return n;
  }
  return null;
}
