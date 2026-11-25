<script lang="ts">
  // Text reads as text: as many lines as the preview holds (12 px / 1.5), wrapped only
  // between words (a long token breaks only when it can't fit a line), the last line fading
  // out instead of a hard cut. Leading blank lines are skipped. Also renders snippet bodies.
  import Hl from "../Hl.svelte";

  interface Props {
    text: string;
    terms: readonly string[];
  }
  let { text, terms }: Props = $props();

  const shown = $derived(text.replace(/^\s*\n/, "").replace(/[ \t]+$/gm, ""));
</script>

<div class="body"><p class="text"><Hl text={shown} {terms} /></p></div>

<style>
  .body {
    width: 100%;
    height: 100%;
    padding: 11px 12px 0;
    overflow: hidden;
    mask-image: linear-gradient(to bottom, #000 calc(100% - 22px), transparent calc(100% - 2px));
  }
  .text {
    margin: 0;
    color: var(--color-ink);
    font-size: var(--text-secondary);
    line-height: 1.5;
    white-space: pre-wrap;
    overflow-wrap: break-word;
  }
</style>
