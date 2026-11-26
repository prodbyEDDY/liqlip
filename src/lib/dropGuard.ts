// "Nothing can be dropped here" for a window's page (W7-R4, from reviewer R2's finding).
//
// A drag that ends over one of our own windows — a QAO card released back over its stack, a shelf
// card over the shelf, a file from Explorer — must be REFUSED, visibly (the "no" cursor) and to
// the drag source: our native drag-out (`clip_drag_out`, OLE `DoDragDrop`) treats any accepted
// drop as "dropped" and the QAO then closes the card. WebView2 accepts a file drop by default
// (and navigates to it); a page refuses by cancelling `dragenter` / `dragover` with
// `dataTransfer.dropEffect = "none"` (MDN, HTML Drag and Drop API → Drag operations: "You can use
// the value none to indicate that no drop is allowed at this location"), and cancels `drop` so the
// browser's default never runs. Works only where the native drop handler is disabled
// (`disable_drag_drop_handler`, ARCHITECTURE §18.2) — otherwise wry's own drop target answers.
//
//   <svelte:window ondragenter={refuseDrop} ondragover={refuseDrop} ondrop={refuseDrop} />

/** The part of a `DragEvent` the guard touches (a plain object in node tests). */
export interface DropLike {
  type: string;
  preventDefault(): void;
  dataTransfer: { dropEffect: string } | null;
}

export function refuseDrop(e: DropLike): void {
  e.preventDefault();
  if (e.type !== "drop" && e.dataTransfer) e.dataTransfer.dropEffect = "none";
}
