<script lang="ts">
  // Full text of a record in Quick Look: search matches highlighted (the store's matching),
  // optional invisible characters. A mark is drawn by CSS (::before) over / next to the real
  // character, so selecting and copying from the view copies the original text.
  import { textRuns, INVISIBLES_LIMIT, VIEW_LIMIT } from "./textView";

  interface Props {
    text: string;
    terms: readonly string[];
    invisibles?: boolean;
    mono?: boolean;
    /** Wrap long lines (prose); code keeps its lines and scrolls sideways. */
    wrap?: boolean;
  }
  let { text, terms, invisibles = false, mono = false, wrap = true }: Props = $props();

  const shown = $derived(text.length > VIEW_LIMIT ? text.slice(0, VIEW_LIMIT) : text);
  const marks = $derived(invisibles && shown.length <= INVISIBLES_LIMIT);
  const runs = $derived(textRuns(shown, terms, marks));
</script>

<pre class="text selectable" class:text--mono={mono} class:text--nowrap={!wrap} class:text--marks={marks}>{#each runs as r, i (i)}{#if r.ws === "nl"}<span
        class="ws ws--nl"
        class:hl={r.hit}></span>{r.text}{:else if r.ws}<span class="ws ws--{r.ws}" class:hl={r.hit}>{r.text}</span>{:else if r.hit}<mark class="hl">{r.text}</mark
      >{:else}{r.text}{/if}{/each}</pre>

<style>
  .text {
    margin: 0;
    color: var(--color-ink);
    font-family: var(--font-sans);
    font-size: var(--text-row);
    line-height: 1.6;
    white-space: pre-wrap;
    overflow-wrap: break-word;
    tab-size: 4;
  }
  .text--mono {
    font-family: var(--font-mono);
    font-size: var(--text-secondary);
    line-height: 1.65;
  }
  .text--nowrap {
    white-space: pre;
    overflow-wrap: normal;
  }
  .hl {
    padding: 0 1px;
    border-radius: 4px;
    background: color-mix(in srgb, var(--color-ink) 16%, transparent);
    color: var(--color-ink);
    font-weight: 600;
  }
  /* Invisible characters: the character stays in the DOM; the mark is painted over it. */
  .ws {
    position: relative;
  }
  .ws::before {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--color-ink-3);
    font-family: var(--font-mono);
    font-size: 0.85em;
    font-weight: 400;
    pointer-events: none;
    user-select: none;
  }
  .ws--space::before {
    content: "·";
  }
  .ws--nbsp::before {
    content: "°";
  }
  .ws--tab::before {
    justify-content: flex-start;
    content: "→";
  }
  .ws--zw {
    padding: 0 0.3em;
  }
  .ws--zw::before {
    content: "¤";
  }
  .ws--nl {
    position: relative;
    display: inline-block;
    width: 0.9em;
  }
  .ws--nl::before {
    justify-content: flex-start;
    content: "¶";
  }
</style>
