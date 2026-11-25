// Selection of the shelf: the current card (keyboard cursor, what Enter acts on) plus the
// multi-selection (a set of ids + an anchor for ranges). The keys for multi-select
// (Shift+arrows, Ctrl+A, Ctrl+click, Shift+click) and the ✓ mark are W1; batch-action UI
// is W2-C, which acts on `targets()` through `runAction`.

class Selection {
  /** Id of the current card, or null. */
  current = $state<number | null>(null);
  /** Multi-selection (empty = just the current card). */
  ids = $state.raw<ReadonlySet<number>>(new Set());
  /** Range anchor for Shift+arrows / Shift+click. */
  anchor: number | null = null;
  /** The user moved the cursor since the last fresh load — a live reload then keeps the
   *  card under the cursor instead of snapping back to the newest record. */
  userMoved = false;

  get multi(): boolean {
    return this.ids.size > 0;
  }

  has(id: number): boolean {
    return this.ids.has(id);
  }

  /** Move the cursor (no multi change). */
  focus(id: number | null, byUser = true): void {
    this.current = id;
    if (byUser) this.userMoved = true;
  }

  /** Ctrl+click: toggle one card in the multi-selection. */
  toggle(id: number): void {
    const next = new Set(this.ids);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    this.ids = next;
    this.anchor = id;
    this.focus(id);
  }

  /** Replace the multi-selection with these ids (Shift ranges, Ctrl+A). */
  setMany(ids: Iterable<number>): void {
    this.ids = new Set(ids);
  }

  clearMulti(): void {
    if (this.ids.size) this.ids = new Set();
    this.anchor = null;
  }

  /** Forget ids that left the list (deleted, filtered out). */
  prune(alive: (id: number) => boolean): void {
    if (this.current !== null && !alive(this.current)) this.current = null;
    if ([...this.ids].some((id) => !alive(id))) this.ids = new Set([...this.ids].filter(alive));
  }

  /** The records an action applies to: the multi-selection in feed order, else the current card. */
  targets(order: (id: number) => number): number[] {
    if (this.ids.size) return [...this.ids].sort((a, b) => order(a) - order(b));
    return this.current === null ? [] : [this.current];
  }

  reset(): void {
    this.current = null;
    this.clearMulti();
    this.userMoved = false;
  }
}

export const selection = new Selection();
