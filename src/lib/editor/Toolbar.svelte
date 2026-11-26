<script lang="ts">
  // The tools (SPEC §9.3 table) + undo/redo as one capsule of 32 px circle buttons, inset 4 px
  // (40 px capsule, concentric). The host places it (overlay: at the frame; window: on top) and
  // passes the editor's store. Each button: its own glyph, a tooltip with its key, the full set
  // of states; the current tool is inverted ink. Buttons never keep focus after a mouse click
  // (mousedown → preventDefault), so Space/Enter never "press it again" (LT defect П-25).
  import { t } from "$lib/i18n/index.svelte";
  import Glyph from "./glyphs/Glyph.svelte";
  import Tip from "./Tip.svelte";
  import { TOOL_META } from "./ui";
  import type { EditorState } from "./state.svelte";

  interface Props {
    state: EditorState;
    /** where tooltips open (overlay panel above the frame → "above") */
    tips?: "below" | "above";
    /** show undo / redo at the end (default true) */
    history?: boolean;
  }
  let { state: st, tips = "below", history = true }: Props = $props();

  const tools = $derived(TOOL_META.filter((m) => m.id !== "crop" || st.mode === "window"));
  const keep = (e: MouseEvent) => e.preventDefault();
</script>

<div class="tb" role="toolbar" aria-label={t("editor.toolbar")}>
  {#each tools as m (m.id)}
    <button
      type="button"
      class="tb__btn"
      class:is-on={st.tool === m.id}
      aria-pressed={st.tool === m.id}
      aria-label={`${t(m.label)} (${m.key})`}
      onmousedown={keep}
      onclick={() => st.setTool(m.id)}
    >
      <Glyph name={m.glyph} />
      <Tip label={t(m.label)} keys={[m.key]} side={tips} />
    </button>
    {#if m.gap}<span class="tb__gap" aria-hidden="true"></span>{/if}
  {/each}
  {#if history}
    <span class="tb__gap tb__gap--wide" aria-hidden="true"></span>
    <button type="button" class="tb__btn" disabled={!st.canUndo} aria-label={`${t("editor.undo")} (Ctrl+Z)`} onmousedown={keep} onclick={() => st.undo()}>
      <Glyph name="undo" />
      <Tip label={t("editor.undo")} keys={["Ctrl", "Z"]} side={tips} />
    </button>
    <button type="button" class="tb__btn" disabled={!st.canRedo} aria-label={`${t("editor.redo")} (Ctrl+Shift+Z)`} onmousedown={keep} onclick={() => st.redo()}>
      <Glyph name="redo" />
      <Tip label={t("editor.redo")} keys={["Ctrl", "Shift", "Z"]} side={tips} />
    </button>
  {/if}
</div>

<style>
  .tb {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    height: 40px;
    padding: 4px;
    border-radius: 999px;
    background: var(--color-raised);
    box-shadow: var(--shadow-card);
  }
  .tb__gap {
    width: 4px;
    flex: none;
  }
  .tb__gap--wide {
    width: 8px;
  }
  .tb__btn {
    position: relative;
    display: inline-flex;
    flex: none;
    align-items: center;
    justify-content: center;
    width: var(--h-md);
    height: var(--h-md);
    padding: 0;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: var(--color-ink-2);
    transition:
      background-color var(--dur-fast) var(--ease-out-soft),
      color var(--dur-fast) var(--ease-out-soft),
      transform var(--dur-fast) var(--ease-out-soft);
  }
  .tb__btn:hover:not(:disabled) {
    background: var(--color-track);
    color: var(--color-ink);
  }
  .tb__btn:active:not(:disabled) {
    background: var(--color-hover);
    transform: scale(0.94);
  }
  .tb__btn.is-on {
    background: var(--color-ink);
    color: var(--color-paper);
  }
  .tb__btn.is-on:hover {
    background: var(--color-ink-hover);
    color: var(--color-paper);
  }
  .tb__btn.is-on:focus-visible {
    outline-color: var(--color-ink);
  }
  .tb__btn:disabled {
    opacity: 0.45;
  }
</style>
