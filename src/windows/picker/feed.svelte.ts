// The shelf feed: pages of 40 from `clips_list` (opaque keyset cursor), or the snippet list.
// - A fresh load supersedes everything in flight (monotonic `seq`): a slower, older response
//   can never overwrite a newer one.
// - At most one "load more" request is in flight; appended pages are de-duplicated by id
//   (a record raised to the top by a re-copy can reappear in a later page).
// - `release()` drops the rows when the shelf is closed (called 260 ms after `picker:close`,
//   after the exit animation), so the hidden window holds nothing.
import { clipsList, snippetsList } from "$lib/ipc/commands";
import type { Clip, FeedQuery, Snippet } from "$lib/ipc/types";

export const PAGE = 40;

export type FeedStatus = "idle" | "loading" | "ready" | "error";

/** What the grid shows: clips for a query, snippets for a text, or a known-empty result. */
export type FeedSource =
  | { mode: "clips"; query: FeedQuery }
  | { mode: "snippets"; text: string }
  | { mode: "none" };

class Feed {
  items = $state.raw<Clip[]>([]);
  snippets = $state.raw<Snippet[]>([]);
  mode = $state<"clips" | "snippets">("clips");
  status = $state<FeedStatus>("idle");
  /** Rejection of the last fresh load (for the error state). */
  error = $state<unknown>(null);
  hasMore = $state(false);
  loadingMore = $state(false);
  /** The last "load more" failed — the grid offers a retry instead of the skeleton row. */
  moreFailed = $state(false);

  #cursor: string | null = null;
  #query: FeedQuery | null = null;
  #seq = 0;

  /** id → index of the shown records (clips or snippets). */
  index = $derived.by(() => {
    const m = new Map<number, number>();
    const list = this.mode === "snippets" ? this.snippets : this.items;
    for (let i = 0; i < list.length; i++) m.set(list[i].id, i);
    return m;
  });

  get count(): number {
    return this.mode === "snippets" ? this.snippets.length : this.items.length;
  }

  idAt(i: number): number | null {
    const list = this.mode === "snippets" ? this.snippets : this.items;
    return list[i]?.id ?? null;
  }

  indexOf(id: number | null): number {
    return id === null ? -1 : (this.index.get(id) ?? -1);
  }

  clip(id: number): Clip | undefined {
    const i = this.index.get(id);
    return i === undefined || this.mode !== "clips" ? undefined : this.items[i];
  }

  snippet(id: number): Snippet | undefined {
    const i = this.index.get(id);
    return i === undefined || this.mode !== "snippets" ? undefined : this.snippets[i];
  }

  /**
   * Load the first page of `source`. `keep` = a refresh behind the user's back (live change):
   * the rows on screen stay until the answer arrives and no skeleton flashes. Resolves true
   * when the result was applied (false: superseded or failed).
   */
  async load(source: FeedSource, keep = false): Promise<boolean> {
    const seq = ++this.#seq;
    this.loadingMore = false;
    this.moreFailed = false;
    if (!keep || this.status !== "ready") {
      this.status = "loading";
      this.error = null;
    }
    try {
      if (source.mode === "none") {
        this.#apply(seq, "clips", [], [], null, null);
        return seq === this.#seq;
      }
      if (source.mode === "snippets") {
        const list = await snippetsList(source.text);
        return this.#apply(seq, "snippets", [], list, null, null);
      }
      const query = { ...source.query, cursor: null, limit: PAGE };
      const page = await clipsList(query);
      return this.#apply(seq, "clips", page.items, [], page.nextCursor, query);
    } catch (e) {
      if (seq !== this.#seq) return false;
      if (keep && this.status === "ready") throw e; // the caller toasts; the rows stay
      this.status = "error";
      this.error = e;
      return false;
    }
  }

  #apply(seq: number, mode: "clips" | "snippets", items: Clip[], snippets: Snippet[], cursor: string | null, query: FeedQuery | null): boolean {
    if (seq !== this.#seq) return false;
    this.mode = mode;
    this.items = items;
    this.snippets = snippets;
    this.#cursor = cursor;
    this.#query = query;
    this.hasMore = cursor !== null;
    this.status = "ready";
    this.error = null;
    return true;
  }

  /** Append the next page. One request in flight; resolves true if rows were added. */
  async loadMore(): Promise<boolean> {
    if (this.mode !== "clips" || !this.hasMore || this.loadingMore || !this.#query || !this.#cursor) return false;
    const seq = this.#seq;
    this.loadingMore = true;
    this.moreFailed = false;
    try {
      const page = await clipsList({ ...this.#query, cursor: this.#cursor });
      if (seq !== this.#seq) return false;
      const seen = new Set(this.items.map((c) => c.id));
      const fresh = page.items.filter((c) => !seen.has(c.id));
      this.items = fresh.length ? [...this.items, ...fresh] : this.items;
      this.#cursor = page.nextCursor;
      this.hasMore = page.nextCursor !== null;
      return fresh.length > 0;
    } catch {
      if (seq === this.#seq) this.moreFailed = true;
      return false;
    } finally {
      if (seq === this.#seq) this.loadingMore = false;
    }
  }

  /** Optimistic update of one record. */
  patch(id: number, patch: Partial<Clip>): void {
    const i = this.index.get(id);
    if (i === undefined || this.mode !== "clips") return;
    const next = this.items.slice();
    next[i] = { ...next[i], ...patch };
    this.items = next;
  }

  /** Replace one record with a fresh copy from the store (keeps its place). */
  replace(clip: Clip): void {
    const i = this.index.get(clip.id);
    if (i === undefined || this.mode !== "clips") return;
    const next = this.items.slice();
    next[i] = clip;
    this.items = next;
  }

  /** Optimistic removal; returns what was removed with positions (for a rollback). */
  remove(ids: readonly number[]): { clip: Clip; index: number }[] {
    if (this.mode !== "clips") return [];
    const drop = new Set(ids);
    const removed: { clip: Clip; index: number }[] = [];
    this.items.forEach((clip, index) => {
      if (drop.has(clip.id)) removed.push({ clip, index });
    });
    if (removed.length) this.items = this.items.filter((c) => !drop.has(c.id));
    return removed;
  }

  /** Put removed records back where they were (failed optimistic removal). */
  restore(removed: readonly { clip: Clip; index: number }[]): void {
    if (this.mode !== "clips" || removed.length === 0) return;
    const next = this.items.slice();
    for (const r of [...removed].sort((a, b) => a.index - b.index)) {
      if (!next.some((c) => c.id === r.clip.id)) next.splice(Math.min(r.index, next.length), 0, r.clip);
    }
    this.items = next;
  }

  /** Reorder: move a record before another (manual Favorites order). */
  move(id: number, beforeId: number | null): void {
    const from = this.index.get(id);
    if (from === undefined || this.mode !== "clips") return;
    const next = this.items.slice();
    const [c] = next.splice(from, 1);
    const to = beforeId === null ? next.length : next.findIndex((x) => x.id === beforeId);
    next.splice(to < 0 ? next.length : to, 0, c);
    this.items = next;
  }

  /** Drop the rows (closed shelf) and cancel anything in flight. */
  release(): void {
    this.#seq++;
    this.items = [];
    this.snippets = [];
    this.#cursor = null;
    this.#query = null;
    this.hasMore = false;
    this.loadingMore = false;
    this.moreFailed = false;
    this.status = "idle";
    this.error = null;
  }
}

export const feed = new Feed();
