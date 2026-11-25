// Viewport of the card grid: the scroller's size and offset (written by `Grid.svelte`), the
// geometry derived from the feed, and the scroll helpers keyboard navigation uses.
import { feed } from "./feed.svelte";
import { geometry, onScreen, scrollFor, type Geometry } from "./layout";

/** Horizontal padding of the scroller (`.list-wrap`: 4 px each side, as in Liquid Tools). */
const PAD_X = 8;
/** Vertical padding of the scroller (2 px top + 8 px bottom). */
const PAD_Y = 10;

class GridView {
  el = $state<HTMLElement | null>(null);
  width = $state(0);
  height = $state(0);
  offset = $state(0);
  horizontal = $state(false);
  /** Leading inset of the strip (= the panel's side padding): the first card lines up with
   *  the pills, then scrolls away so cards meet the panel edge flush (Liquid Tools
   *  `.list--row`). */
  stripLead = $state(14);

  /** Geometry of the records on screen. */
  geometry: Geometry = $derived(geometry(feed.count, this.width, this.height, this.horizontal));
  /** Viewport length along the scroll axis. */
  extent = $derived(this.horizontal ? this.width : this.height);
  /** Content inset before the first tile along the scroll axis. */
  lead = $derived(this.horizontal ? this.stripLead : 0);

  measure(): void {
    const el = this.el;
    if (!el) return;
    this.width = Math.max(80, el.clientWidth - (this.horizontal ? 0 : PAD_X));
    this.height = Math.max(0, el.clientHeight - PAD_Y);
    this.offset = this.horizontal ? el.scrollLeft : el.scrollTop;
  }

  /** Bring card `index` fully into view (instant — keyboard navigation must feel immediate). */
  reveal(index: number): void {
    const el = this.el;
    if (!el || index < 0) return;
    const next = scrollFor(this.geometry, index, this.offset - this.lead, this.extent) + (this.horizontal ? this.lead : 0);
    if (Math.abs(next - this.offset) < 1) return;
    if (this.horizontal) el.scrollLeft = next;
    else el.scrollTop = next;
    this.offset = next;
  }

  scrollToStart(): void {
    const el = this.el;
    if (!el) return;
    el.scrollTo({ top: 0, left: 0 });
    this.offset = 0;
  }

  /** The first nine cards on screen, in order (Ctrl+1…9). */
  onScreen(): number[] {
    return onScreen(this.geometry, this.offset - this.lead, this.extent, 9);
  }

  /** Scrolled away from the newest records (live updates then wait behind a plaque). */
  get scrolled(): boolean {
    return this.offset > 80;
  }
}

export const view = new GridView();
