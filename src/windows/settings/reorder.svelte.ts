// Manual ordering for the settings lists (folders, snippets): HTML5 drag with a live
// preview (rows move while you drag; dropping outside the list puts everything back) and
// Alt+↑/↓ on a focused row. The list commits one move as `(id, beforeId | null)`, the
// shape of `folder_move`; a failed commit restores the previous order.
import { moveBefore } from "./logic";

interface Item {
  id: number;
}

export interface ReorderOptions<T extends Item> {
  get: () => T[];
  set: (items: T[]) => void;
  commit: (id: number, beforeId: number | null) => Promise<void>;
  /** Reordering allowed right now (e.g. not while a search filter is active). */
  enabled: () => boolean;
  onError?: (e: unknown) => void;
}

export function createReorder<T extends Item>(opts: ReorderOptions<T>) {
  let dragId = $state<number | null>(null);
  let snapshot: T[] | null = null;
  let dropped = false;

  const byOrder = (ids: number[], items: T[]) => ids.map((id) => items.find((x) => x.id === id)!);

  async function commit(id: number, prev: T[]) {
    const items = opts.get();
    if (items.map((i) => i.id).join() === prev.map((i) => i.id).join()) return;
    const at = items.findIndex((i) => i.id === id);
    const beforeId = items[at + 1]?.id ?? null;
    try {
      await opts.commit(id, beforeId);
    } catch (e) {
      opts.set(prev);
      opts.onError?.(e);
    }
  }

  return {
    get dragId() {
      return dragId;
    },
    start(id: number, e: DragEvent) {
      if (!opts.enabled()) {
        e.preventDefault();
        return;
      }
      dragId = id;
      dropped = false;
      snapshot = opts.get();
      if (e.dataTransfer) {
        e.dataTransfer.effectAllowed = "move";
        e.dataTransfer.setData("text/plain", String(id));
      }
    },
    over(id: number, e: DragEvent) {
      if (dragId === null) return;
      e.preventDefault();
      if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
      if (id === dragId) return;
      const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
      const after = e.clientY > rect.top + rect.height / 2;
      const items = opts.get();
      const ids = items.map((i) => i.id);
      const rest = ids.filter((x) => x !== dragId);
      const beforeId = rest[rest.indexOf(id) + (after ? 1 : 0)] ?? null;
      const order = moveBefore(ids, dragId, beforeId);
      if (order.join() !== ids.join()) opts.set(byOrder(order, items));
    },
    drop(e: DragEvent) {
      if (dragId === null) return;
      e.preventDefault();
      dropped = true;
    },
    end() {
      if (dragId === null) return;
      const id = dragId;
      const prev = snapshot ?? opts.get();
      dragId = null;
      snapshot = null;
      if (!dropped) {
        opts.set(prev);
        return;
      }
      void commit(id, prev);
    },
    /** Alt+↑/↓: move one step. Returns true when handled. */
    key(id: number, e: KeyboardEvent): boolean {
      if (!e.altKey || (e.key !== "ArrowUp" && e.key !== "ArrowDown") || !opts.enabled()) return false;
      e.preventDefault();
      void this.step(id, e.key === "ArrowUp" ? -1 : 1);
      return true;
    },
    async step(id: number, delta: -1 | 1) {
      if (!opts.enabled()) return;
      const prev = opts.get();
      const ids = prev.map((i) => i.id);
      const at = ids.indexOf(id);
      const to = at + delta;
      if (at < 0 || to < 0 || to >= ids.length) return;
      const rest = ids.filter((x) => x !== id);
      const beforeId = delta < 0 ? ids[to] : (rest[to] ?? null);
      opts.set(byOrder(moveBefore(ids, id, beforeId), prev));
      await commit(id, prev);
    },
  };
}
