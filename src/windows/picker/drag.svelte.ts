// Card drag (SPEC §3.6, port of Liquid Tools' press → drag gesture): a press that moves
// more than 6 px becomes an in-app drag with a ghost pinned to the cursor (rotated −3°).
// Drop targets are elements with `data-drop` — the Favorites pill ("favorites"), folder
// pills ("folder:<id>") and the paste-stack indicator ("stack"). Inside the Favorites pill a
// card dropped on another card reorders them (`data-card` + left/right half). Leaving the
// panel escalates to the native OLE drag-out (`clip_drag_out`).
import { feed } from "./feed.svelte";
import { selection } from "./selection.svelte";
import { shelf } from "./shelf.svelte";
import { activate, clickPastes, moveFavorite, runAction } from "./actions.svelte";
import { folderIdOf, type PillId } from "./pills";
import { clickIntent } from "./keymap";

const THRESHOLD_SQ = 36;

class CardDrag {
  active = $state(false);
  ids = $state.raw<number[]>([]);
  x = $state(0);
  y = $state(0);
  /** Drop target under the cursor (`data-drop` value). */
  over = $state<string | null>(null);
  /** Favorites reorder marker: insert before this card (`null` = at the end). */
  before = $state<{ id: number | null; edgeOf: number; side: "start" | "end" } | null>(null);
  panel: HTMLElement | null = null;

  #press: { id: number; x: number; y: number } | null = null;
  /** The press became a drag — swallow the click that follows the pointerup. */
  #dragged = false;

  /** pointerdown on a card (primary button, no modifier). */
  press(id: number, e: PointerEvent): void {
    if (e.button !== 0) return;
    this.#dragged = false;
    this.#press = { id, x: e.clientX, y: e.clientY };
    window.addEventListener("pointermove", this.#onPressMove);
    window.addEventListener("pointerup", this.#endPress);
    window.addEventListener("pointercancel", this.#endPress);
  }

  /** click on a card: activate unless the press turned into a drag. `detail` 2 = double click. */
  click(id: number, e: MouseEvent): void {
    if (this.#dragged) {
      this.#dragged = false;
      return;
    }
    if (e.ctrlKey || e.metaKey) {
      if (feed.mode === "clips") selection.toggle(id);
      return;
    }
    if (e.shiftKey) {
      if (feed.mode === "clips") shelf.selectRange(id);
      return;
    }
    selection.focus(id);
    // Double click = Enter (useful when a click only copies and keeps the shelf open); when a
    // click already pastes, the pair's first click did it — never twice (`clickIntent`).
    const intent = clickIntent(e.detail, clickPastes());
    if (intent === null || (e.detail >= 2 && !shelf.open)) return;
    if (e.detail >= 2) void runAction("paste", [id]);
    else void activate(id, true);
  }

  #onPressMove = (e: PointerEvent): void => {
    const p = this.#press;
    if (!p) return;
    const dx = e.clientX - p.x;
    const dy = e.clientY - p.y;
    if (dx * dx + dy * dy < THRESHOLD_SQ) return;
    this.#endPress();
    this.#begin(p.id, e.clientX, e.clientY);
  };

  #endPress = (): void => {
    this.#press = null;
    window.removeEventListener("pointermove", this.#onPressMove);
    window.removeEventListener("pointerup", this.#endPress);
    window.removeEventListener("pointercancel", this.#endPress);
  };

  #begin(id: number, x: number, y: number): void {
    if (feed.mode !== "clips") return; // snippets are not files/records to file away
    this.#dragged = true;
    this.ids = selection.multi && selection.has(id) ? shelf.targets() : [id];
    this.active = true;
    this.x = x;
    this.y = y;
    this.#hitTest(x, y);
    window.addEventListener("pointermove", this.#onDragMove);
    window.addEventListener("pointerup", this.#onDragUp);
    window.addEventListener("pointercancel", this.cancel);
  }

  #hitTest(x: number, y: number): void {
    const el = document.elementFromPoint(x, y) as HTMLElement | null;
    this.over = el?.closest<HTMLElement>("[data-drop]")?.dataset.drop ?? null;
    this.before = null;
    if (this.over || shelf.pill !== "favorites" || this.ids.length !== 1) return;
    const card = el?.closest<HTMLElement>("[data-card]");
    const id = card ? Number(card.dataset.card) : NaN;
    if (!card || !Number.isFinite(id) || id === this.ids[0]) return;
    // Cards flow left → right in the grid and in the strip: the half decides the side.
    const r = card.getBoundingClientRect();
    const after = x > r.left + r.width / 2;
    const i = feed.indexOf(id);
    const beforeId = after ? feed.idAt(i + 1) : id;
    this.before = { id: beforeId, edgeOf: id, side: after ? "end" : "start" };
  }

  #onDragMove = (e: PointerEvent): void => {
    if (!this.active) return;
    this.x = e.clientX;
    this.y = e.clientY;
    const r = this.panel?.getBoundingClientRect();
    if (r && (e.clientX < r.left || e.clientX > r.right || e.clientY < r.top || e.clientY > r.bottom)) {
      const ids = this.ids;
      this.cancel();
      void runAction("dragOut", ids);
      return;
    }
    this.#hitTest(e.clientX, e.clientY);
  };

  #onDragUp = (): void => {
    if (!this.active) return;
    const ids = this.ids;
    const over = this.over;
    const before = this.before;
    this.cancel();
    if (over) void drop(ids, over as PillId | "stack");
    else if (before) void moveFavorite(ids[0], before.id);
  };

  /** End the drag without dropping (Esc, close, pointercancel). */
  cancel = (): void => {
    this.#endPress();
    this.active = false;
    this.ids = [];
    this.over = null;
    this.before = null;
    window.removeEventListener("pointermove", this.#onDragMove);
    window.removeEventListener("pointerup", this.#onDragUp);
    window.removeEventListener("pointercancel", this.cancel);
  };
}

async function drop(ids: number[], target: PillId | "stack"): Promise<void> {
  if (target === "favorites") return runAction("favorite", ids);
  if (target === "stack") return runAction("stack", ids);
  const folderId = folderIdOf(target);
  if (folderId !== null) return runAction("moveToFolder", ids, { folderId });
}

export const drag = new CardDrag();
