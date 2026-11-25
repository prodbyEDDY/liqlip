// Pills of the shelf (SPEC §2.1, §3.9): All · Text (text + contact) · Links · Images · Files ·
// Code · Colors · Favorites · Snippets, then the user's folders in manual order — each with its
// counter from `clips_counts`. Only Favorites and folders accept a dropped card. Pure —
// node-tested (tests/pickerPills).
import type { Counts, Filter, Folder, Kind } from "$lib/ipc/types";

export type BuiltinPill = "all" | "text" | "link" | "image" | "file" | "code" | "color" | "favorites" | "snippets";
/** `folder:<id>` for user folders. */
export type PillId = BuiltinPill | `folder:${number}`;

export interface Pill {
  id: PillId;
  /** Built-in pills are localized by id (`picker.pill.<id>`); folders show `name`. */
  builtin: boolean;
  name: string;
  count: number;
  /** Feed filter of this pill (snippets list their own store). */
  filter: Filter;
  mode: "clips" | "snippets";
  droppable: boolean;
  folderId: number | null;
}

export const BUILTIN_PILLS: readonly BuiltinPill[] = ["all", "text", "link", "image", "file", "code", "color", "favorites", "snippets"];

const KIND_PILLS: Partial<Record<BuiltinPill, Kind[]>> = {
  text: ["text", "contact"],
  link: ["link"],
  image: ["image"],
  file: ["file"],
  code: ["code"],
  color: ["color"],
};

export const folderPillId = (id: number): PillId => `folder:${id}`;

export function folderIdOf(id: PillId): number | null {
  const m = /^folder:(\d+)$/.exec(id);
  return m ? Number(m[1]) : null;
}

/** Filter of a pill id (also for a pill that is not in the list any more). */
export function pillFilter(id: PillId): Filter {
  if (id === "favorites") return { favorites: true };
  const kinds = KIND_PILLS[id as BuiltinPill];
  if (kinds) return { kinds: [...kinds] };
  const folder = folderIdOf(id);
  return folder !== null ? { folderId: folder } : {};
}

export function buildPills(counts: Counts | null, folders: readonly Folder[]): Pill[] {
  const kind = (k: Kind) => counts?.kinds[k] ?? 0;
  const count: Record<BuiltinPill, number> = {
    all: counts?.all ?? 0,
    text: kind("text") + kind("contact"),
    link: kind("link"),
    image: kind("image"),
    file: kind("file"),
    code: kind("code"),
    color: kind("color"),
    favorites: counts?.favorites ?? 0,
    snippets: counts?.snippets ?? 0,
  };
  const builtins: Pill[] = BUILTIN_PILLS.map((id) => ({
    id,
    builtin: true,
    name: id,
    count: count[id],
    filter: pillFilter(id),
    mode: id === "snippets" ? "snippets" : "clips",
    droppable: id === "favorites",
    folderId: null,
  }));
  const own: Pill[] = [...folders]
    .sort((a, b) => a.position - b.position)
    .map((f) => ({
      id: folderPillId(f.id),
      builtin: false,
      name: f.name,
      count: counts?.folders[f.id] ?? f.count,
      filter: { folderId: f.id },
      mode: "clips",
      droppable: true,
      folderId: f.id,
    }));
  return [...builtins, ...own];
}

/** Next/previous pill with wrap-around (Ctrl+Tab / Ctrl+Shift+Tab). */
export function cyclePill(pills: readonly Pill[], current: PillId, delta: number): PillId {
  if (pills.length === 0) return current;
  const i = Math.max(0, pills.findIndex((p) => p.id === current));
  return pills[(i + delta + pills.length) % pills.length].id;
}

/** Where a dragged folder pill lands: the folder id to insert before (`null` = the end). */
export function folderDropBefore(order: readonly number[], dragged: number, targetIndex: number): number | null {
  const rest = order.filter((id) => id !== dragged);
  return targetIndex >= rest.length ? null : rest[Math.max(0, targetIndex)];
}
