// The shelf's layers (W2-C, on the W2-E kit), registered by `Picker.svelte` at start:
//   actions   — the actions menu (Ctrl+K, right click, ⋯)          ActionsMenu
//   preview   — Quick Look (Space, Ctrl+Y; Ctrl+E edits)             QuickLook
//   stack     — the paste-stack popover ("Stack N", menu)            StackPopover
//   snippet   — new / edit / save-as snippet sheet                   SnippetSheet
//   confirm   — confirmation of an irreversible step                 ConfirmDialog
//   pillMenu  — context menu of a folder pill / the Snippets pill    PillMenuLayer
//   menu      — a small menu of caller-built entries                 ContextMenu
import { overlays } from "../overlays.svelte";
import ActionsMenu from "./ActionsMenu.svelte";
import QuickLook from "./QuickLook.svelte";
import StackPopover from "./StackPopover.svelte";
import SnippetSheet from "./SnippetSheet.svelte";
import ConfirmDialog from "./ConfirmDialog.svelte";
import PillMenuLayer from "./PillMenuLayer.svelte";
import ContextMenu from "./ContextMenu.svelte";

export function registerDefaultLayers(): void {
  overlays.register("actions", ActionsMenu);
  overlays.register("preview", QuickLook);
  overlays.register("stack", StackPopover);
  overlays.register("snippet", SnippetSheet);
  overlays.register("confirm", ConfirmDialog);
  overlays.register("pillMenu", PillMenuLayer);
  overlays.register("menu", ContextMenu);
}
