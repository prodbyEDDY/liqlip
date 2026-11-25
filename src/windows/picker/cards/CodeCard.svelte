<script lang="ts">
  // Code keeps its shape: functional mono, indentation preserved (the common indent removed so
  // a nested fragment starts at the left edge), starting at the first meaningful line. Long
  // lines wrap (a card is ~24 columns wide — a one-liner would otherwise show a fragment) and
  // the rest fades out at the bottom.
  import Hl from "../Hl.svelte";

  interface Props {
    text: string;
    terms: readonly string[];
  }
  let { text, terms }: Props = $props();

  const shown = $derived.by(() => {
    const lines = text.replace(/\t/g, "  ").split(/\r?\n/);
    while (lines.length > 1 && !lines[0].trim()) lines.shift();
    const indents = lines.filter((l) => l.trim()).map((l) => (l.match(/^ */)?.[0].length ?? 0));
    const cut = indents.length ? Math.min(...indents) : 0;
    return lines
      .slice(0, 14)
      .map((l) => l.slice(cut).trimEnd())
      .join("\n");
  });
</script>

<div class="body"><pre class="code"><Hl text={shown} {terms} /></pre></div>

<style>
  .body {
    width: 100%;
    height: 100%;
    padding: 11px 12px 0;
    overflow: hidden;
    mask-image: linear-gradient(to bottom, #000 calc(100% - 22px), transparent calc(100% - 2px));
  }
  .code {
    margin: 0;
    color: var(--color-ink);
    font-family: var(--font-mono);
    font-size: 11.5px;
    line-height: 1.55;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    tab-size: 2;
  }
</style>
