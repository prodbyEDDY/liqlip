<script lang="ts">
  // ⚠ TEMPORARY STUB (W6-D) of W6-C's `Toolbar.svelte` (ARCHITECTURE §18.6) — see `../editorStub.ts`.
  // Two tools only; the labels are the tool keys (no strings of its own to translate).
  import Ico from "$lib/components/Ico.svelte";
  import { untrack } from "svelte";
  import type { EditorState, ToolId } from "../editorStub";

  let { state: es }: { state: EditorState } = $props();
  const tool = untrack(() => es.tool);
  const TOOLS: [ToolId, string][] = [
    ["select", "V"],
    ["rect", "R"],
  ];
</script>

<div class="stub-tools">
  {#each TOOLS as [id, key] (id)}
    <button
      type="button"
      class="stub-tool"
      class:stub-tool--on={$tool === id}
      title={key}
      aria-label={key}
      aria-pressed={$tool === id}
      onmousedown={(e) => e.preventDefault()}
      onclick={() => tool.set(id)}
    >
      {#if id === "select"}<Ico name="pointer" size={16} />{:else}<span class="stub-glyph" aria-hidden="true"></span>{/if}
    </button>
  {/each}
</div>

<style>
  .stub-tools {
    display: flex;
    gap: 2px;
  }
  .stub-tool {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--h-md);
    height: var(--h-md);
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: var(--color-ink-2);
  }
  .stub-tool:hover {
    background: var(--color-track);
    color: var(--color-ink);
  }
  .stub-tool--on {
    background: var(--color-ink);
    color: var(--color-paper);
  }
  .stub-tool--on:hover {
    background: var(--color-ink-hover);
    color: var(--color-paper);
  }
  .stub-glyph {
    width: 14px;
    height: 11px;
    border-radius: 2px;
    box-shadow: inset 0 0 0 1.5px currentColor;
  }
</style>
