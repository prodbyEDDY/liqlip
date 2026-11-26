<script lang="ts">
  // Typing a text object (SPEC §9.3 rule 8): a textarea laid exactly over the object, in the
  // same font, size and colour as the canvas draws it. Enter — new line; Ctrl+Enter, Esc or a
  // click outside — done (the text is KEPT; LT discarded it on Esc, П-9); empty text is removed.
  // Every key typed here stops at the field (rule 12): no tool shortcut fires while typing, and
  // Esc travels on to the host, which calls `editor.escape()` (that commits the text) — except
  // while an IME composes (its Esc cancels the composition). Ctrl+S / Ctrl+P / Ctrl+W finish the
  // text and go on to the host; WebView2 accelerators (F5, Ctrl+R…) are swallowed (`textKey`).
  import { onMount, tick } from "svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { TEXT_FONT, TEXT_WEIGHT, type TextObj } from "./doc";
  import { TEXT_LINE_HEIGHT, contrastInk, type MeasureText } from "./textLayout";
  import { textFieldLayout } from "./render";
  import { toScreen, type View } from "./view";
  import { textKey } from "./interaction";
  import type { EditorState } from "./state.svelte";

  interface Props {
    state: EditorState;
    obj: TextObj;
    view: View;
    unit: number;
    measure: MeasureText;
  }
  let { state: st, obj, view, unit, measure }: Props = $props();

  let field: HTMLTextAreaElement;
  const geo = $derived(textFieldLayout(obj, unit, measure));
  const pos = $derived(toScreen(view, obj.at));
  const z = $derived(view.z);
  const px = $derived(geo.shape.fontSize * z);
  const badge = $derived(obj.style === "badge");
  const ink = $derived(badge ? contrastInk(obj.color) : obj.color);
  // auto-width text grows with what is typed (+ room for the caret); a block keeps its width
  const width = $derived((obj.width === null ? geo.layout.w + geo.shape.fontSize * 0.6 : geo.layout.w) * z);
  const height = $derived(Math.max(geo.layout.h, geo.shape.fontSize * TEXT_LINE_HEIGHT + geo.layout.pad.y * 2) * z);

  onMount(() => {
    void tick().then(() => {
      field?.focus({ preventScroll: true });
      const n = field?.value.length ?? 0;
      field?.setSelectionRange(n, n);
    });
  });

  function onKeydown(e: KeyboardEvent): void {
    switch (textKey(e)) {
      case "escape":
        return; // the host's Esc → editor.escape() → commit
      case "action":
        // Ctrl+S / Ctrl+Shift+S / Ctrl+P / Ctrl+W: the text is done, then the same key as at rest
        // (→ the host's save / save as / pin / close intent)
        e.preventDefault();
        e.stopPropagation();
        st.dispatch({ type: "textCommit" });
        st.dispatch({ type: "key", code: e.code, ctrl: true, shift: e.shiftKey, alt: false, repeat: e.repeat, composing: false, time: e.timeStamp });
        return;
      case "commit":
        e.preventDefault();
        e.stopPropagation();
        st.dispatch({ type: "textCommit" });
        return;
      case "browser":
        e.preventDefault(); // F5 / Ctrl+R would reload the window and lose the drawing
        e.stopPropagation();
        return;
      default:
        // "ime" (the IME owns every key, Esc too) and ordinary keys stay in the field
        e.stopPropagation();
        if (e.code === "Tab") e.preventDefault(); // focus stays in the text while typing
    }
  }
</script>

<textarea
  bind:this={field}
  class="ti"
  class:ti--block={obj.width !== null}
  class:ti--badge={badge}
  value={obj.text}
  rows="1"
  spellcheck="false"
  autocomplete="off"
  aria-label={t("editor.text.field")}
  style:left={`${pos[0]}px`}
  style:top={`${pos[1]}px`}
  style:width={`${width}px`}
  style:height={`${height}px`}
  style:padding={`${geo.layout.pad.y * z}px ${geo.layout.pad.x * z}px`}
  style:font={`${TEXT_WEIGHT} ${px}px / ${TEXT_LINE_HEIGHT} ${TEXT_FONT}`}
  style:color={ink}
  style:caret-color={ink}
  style:--plate={obj.color}
  style:border-radius={badge ? `${geo.shape.fontSize * 0.35 * z}px` : "2px"}
  oninput={(e) => st.dispatch({ type: "text", value: (e.currentTarget as HTMLTextAreaElement).value })}
  onkeydown={onKeydown}
  onpointerdown={(e) => e.stopPropagation()}
></textarea>

<style>
  .ti {
    position: absolute;
    z-index: 5;
    margin: 0;
    border: 0;
    background: transparent;
    overflow: hidden;
    resize: none;
    white-space: pre;
    word-break: normal;
    outline: 1px dashed color-mix(in srgb, currentColor 70%, transparent);
    outline-offset: 2px;
    box-shadow: 0 0 0 1px rgb(255 255 255 / 0.6);
    user-select: text;
    -webkit-user-select: text;
  }
  .ti--block {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .ti--badge {
    background: var(--plate);
  }
  .ti:focus-visible {
    outline: 1px dashed color-mix(in srgb, currentColor 70%, transparent);
    outline-offset: 2px;
  }
</style>
