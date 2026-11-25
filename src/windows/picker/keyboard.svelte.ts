// Window-level keyboard of the shelf: routes every key press — first to the open overlay
// layer, then through the key map (`keymap.ts`, physical keys) to the shelf / actions.
// Typing anywhere goes to the search field. Also tracks a held Ctrl (digits 1…9 on the
// first nine cards on screen).
import { settingsOpen } from "$lib/ipc/commands";
import { errorMessage } from "$lib/ipc/errors";
import { feed } from "./feed.svelte";
import { overlays } from "./overlays.svelte";
import { selection } from "./selection.svelte";
import { shelf } from "./shelf.svelte";
import { toasts } from "./toasts.svelte";
import { view } from "./view.svelte";
import { drag } from "./drag.svelte";
import { copySnippet, pasteSnippet, runAction } from "./actions.svelte";
import { isSwallowedRepeat, isTypingKey, resolveKey, type Command } from "./keymap";

/** Ctrl must be held this long before the digits appear (quick chords don't flash them). */
const CTRL_HINT_DELAY_MS = 120;

class Keyboard {
  /** Ctrl is held: show 1…9 on the first nine cards on screen. */
  ctrlHeld = $state(false);
  #ctrlTimer: ReturnType<typeof setTimeout> | undefined;

  onKeydown = (e: KeyboardEvent): void => {
    if (!shelf.open) return;
    if ((e.key === "Control" || e.code.startsWith("Control")) && !e.repeat) {
      clearTimeout(this.#ctrlTimer);
      this.#ctrlTimer = setTimeout(() => (this.ctrlHeld = true), CTRL_HINT_DELAY_MS);
    }
    if (overlays.handleKey(e)) return;
    if (drag.active && e.code === "Escape") {
      e.preventDefault();
      drag.cancel();
      return;
    }

    const el = e.target instanceof HTMLElement ? e.target : null;
    const inSearch = el !== null && el === shelf.searchEl;
    const inOtherInput = !inSearch && el !== null && (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement || el.isContentEditable);
    const input = shelf.searchEl;
    const caretAtEnd = !input || (input.selectionStart === input.value.length && input.selectionEnd === input.value.length);
    const ctx = {
      inSearch,
      inOtherInput,
      onButton: el instanceof HTMLButtonElement,
      searchEmpty: shelf.draft.length === 0,
      caretAtEnd,
      undoAvailable: toasts.undoToken !== null,
    };
    if (isSwallowedRepeat(e, ctx)) {
      e.preventDefault();
      return;
    }
    const cmd = resolveKey(e, ctx);
    if (!cmd) {
      // Type-to-search: focusing the field during keydown makes the character land in it.
      if (!inSearch && !inOtherInput && (isTypingKey(e) || e.code === "Backspace")) shelf.focusSearch(false);
      return;
    }
    e.preventDefault();
    void this.#run(cmd);
  };

  onKeyup = (e: KeyboardEvent): void => {
    if (e.key === "Control" || e.code.startsWith("Control")) this.releaseCtrl();
  };

  releaseCtrl = (): void => {
    clearTimeout(this.#ctrlTimer);
    this.ctrlHeld = false;
  };

  async #run(cmd: Command): Promise<void> {
    const ids = shelf.targets();
    const snippet = feed.mode === "snippets" && selection.current !== null ? feed.snippet(selection.current) : undefined;
    switch (cmd.type) {
      case "nav":
        return shelf.navigate(cmd.dir, cmd.extend);
      case "paste":
        if (snippet) return pasteSnippet(snippet, false);
        return runAction("paste", ids);
      case "pastePlain":
        if (snippet) return pasteSnippet(snippet, true);
        return runAction("pastePlain", ids);
      case "pasteKeepOpen":
        if (snippet) return pasteSnippet(snippet, false, true);
        return runAction("pasteKeepOpen", ids);
      case "copy":
        if (snippet) return copySnippet(snippet);
        return runAction("copy", ids);
      case "pasteNth": {
        const index = view.onScreen()[cmd.n - 1];
        const id = index === undefined ? null : feed.idAt(index);
        if (id === null) return;
        selection.focus(id);
        const s = feed.mode === "snippets" ? feed.snippet(id) : undefined;
        if (s) return pasteSnippet(s, cmd.plain);
        return runAction(cmd.plain ? "pastePlain" : "paste", [id]);
      }
      case "pill":
        return shelf.pillAt(cmd.n);
      case "pillCycle":
        return shelf.cyclePill(cmd.delta);
      case "favorite":
        if (feed.mode === "clips") return runAction("toggleFavorite", ids);
        return;
      case "rename":
        return shelf.startRename(selection.current);
      case "open":
        return runAction("open", ids.slice(0, 1));
      case "reveal":
        return runAction("reveal", ids.slice(0, 1));
      case "delete":
        if (feed.mode === "clips") return runAction("delete", ids);
        return;
      case "undo":
        return shelf.undo();
      case "selectAll":
        return shelf.selectAll();
      case "focusSearch":
        return shelf.focusSearch(true);
      case "settings":
        try {
          await settingsOpen();
        } catch (e) {
          toasts.error(errorMessage(e));
        }
        return;
      case "escape":
        return shelf.escape();
      case "preview":
        if (ids.length) overlays.open("preview", { ids });
        return;
      case "actions":
        if (ids.length) overlays.open("actions", { ids, anchor: cardAnchor(selection.current) });
        return;
      case "stack":
        if (feed.mode === "clips") return runAction("stack", ids);
        return;
    }
  }
}

/** Viewport point under the bottom-left of a card (where the actions menu opens). */
export function cardAnchor(id: number | null): { x: number; y: number } | undefined {
  if (id === null) return undefined;
  const el = document.querySelector<HTMLElement>(`[data-card="${id}"]`);
  const r = el?.getBoundingClientRect();
  return r ? { x: r.left + 8, y: r.bottom - 40 } : undefined;
}

export const keyboard = new Keyboard();
