<script lang="ts">
  // ⚠ TEMPORARY STUB (W6-D) of W6-C's `PropsBar.svelte` (ARCHITECTURE §18.6) — see `../editorStub.ts`.
  // Three colour swatches (annotation colours are user content — the only colour in the chrome).
  import { untrack } from "svelte";
  import type { EditorState } from "../editorStub";

  let { state: es }: { state: EditorState } = $props();
  const color = untrack(() => es.color);
  const SWATCHES = ["#e5484d", "#f5d90a", "#ffffff"];
</script>

<div class="stub-props">
  {#each SWATCHES as c (c)}
    <button
      type="button"
      class="stub-swatch"
      class:stub-swatch--on={$color === c}
      style="--swatch: {c}"
      title={c}
      aria-label={c}
      aria-pressed={$color === c}
      onmousedown={(e) => e.preventDefault()}
      onclick={() => color.set(c)}
    ></button>
  {/each}
</div>

<style>
  .stub-props {
    display: flex;
    gap: 4px;
    padding: 0 4px;
  }
  .stub-swatch {
    width: var(--h-sm);
    height: var(--h-sm);
    border: 0;
    border-radius: 999px;
    background: var(--swatch);
    box-shadow: var(--shadow-on-lift);
    transition: transform var(--dur-fast) var(--ease-out-soft);
  }
  .stub-swatch:hover {
    transform: scale(1.08);
  }
  .stub-swatch--on {
    transform: scale(1.18);
  }
</style>
