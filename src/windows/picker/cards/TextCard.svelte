<script lang="ts">
  // Text reads as text: the first lines on the content tone, clamped to 7 lines (Liquid Tools
  // TextCard). Code is functional mono at 11 px. Also renders snippet bodies.
  import Hl from "../Hl.svelte";

  interface Props {
    text: string;
    terms: readonly string[];
    mono?: boolean;
  }
  let { text, terms, mono = false }: Props = $props();
</script>

<div class="body"><p class="text" class:code={mono}><Hl {text} {terms} /></p></div>

<style>
  .body {
    width: 100%;
    height: 100%;
    padding: 12px;
    background: var(--color-content);
    overflow: hidden;
  }
  .text {
    margin: 0;
    font-size: var(--text-secondary);
    line-height: var(--text-secondary--line-height);
    color: var(--color-ink-2);
    white-space: pre-wrap;
    word-break: break-word;
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 7;
    line-clamp: 7;
    overflow: hidden;
  }
  .code {
    font-family: var(--font-mono);
    font-size: 11px;
  }
</style>
