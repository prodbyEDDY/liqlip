// Focus helpers for modal shelf layers (Quick Look, dialogs, the snippet sheet, the stack
// popover): Tab cycles inside the layer — focus never leaves it for the grid behind.

const FOCUSABLE =
  'button:not(:disabled), [href], input:not(:disabled), textarea:not(:disabled), select:not(:disabled), [tabindex]:not([tabindex="-1"])';

/** Focusable elements of `root` in DOM order (visible ones only). */
export function focusables(root: HTMLElement): HTMLElement[] {
  return [...root.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((el) => el.offsetParent !== null || el === document.activeElement);
}

/** Handle Tab / Shift+Tab inside `root`: move to the next / previous focusable, wrapping.
 *  Returns true when the key was a Tab (the caller reports it handled). */
export function trapTab(e: KeyboardEvent, root: HTMLElement | null | undefined): boolean {
  if (e.key !== "Tab" || !root) return false;
  const list = focusables(root);
  if (list.length === 0) {
    root.focus({ preventScroll: true });
    return true;
  }
  const i = list.indexOf(document.activeElement as HTMLElement);
  const next = i < 0 ? (e.shiftKey ? list[list.length - 1] : list[0]) : list[(i + (e.shiftKey ? -1 : 1) + list.length) % list.length];
  next.focus({ preventScroll: false });
  return true;
}

/** Focus inside a layer: the element marked `data-autofocus`, else the layer itself. */
export function focusLayer(root: HTMLElement | null | undefined): void {
  if (!root) return;
  const target = root.querySelector<HTMLElement>("[data-autofocus]") ?? root;
  target.focus({ preventScroll: true });
}
