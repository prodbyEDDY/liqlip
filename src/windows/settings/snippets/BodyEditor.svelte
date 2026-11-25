<script lang="ts">
  // Monospace snippet body with placeholder highlighting: a transparent textarea over a
  // mirror layer that paints only the marks (same font metrics, scroll kept in sync), so
  // the text itself, the caret, selection and IME stay native. Problems (unknown names,
  // bad attributes…) get a wavy underline; `parseSnippet` is the shared parser.
  import { parseSnippet } from "$lib/snippets/expand";

  interface Props {
    value: string;
    id?: string;
    describedby?: string;
    invalid?: boolean;
    placeholder?: string;
    oninput: (value: string) => void;
  }
  let { value, id, describedby, invalid = false, placeholder, oninput }: Props = $props();

  let area = $state<HTMLTextAreaElement>();
  let scrollTop = $state(0);
  let scrollLeft = $state(0);

  const tokens = $derived(parseSnippet(value));

  /** Insert text at the caret (placeholder reference). */
  export function insert(text: string) {
    if (!area) return;
    area.focus();
    area.setRangeText(text, area.selectionStart, area.selectionEnd, "end");
    oninput(area.value);
  }
</script>

<div class="body squircle" class:body--invalid={invalid}>
  <div class="mirror" aria-hidden="true">
    <div class="mirror__inner" style:transform="translate({-scrollLeft}px, {-scrollTop}px)">
      {#each tokens as tok (tok.start)}
        {#if tok.kind === "text"}{tok.text}{:else if tok.kind === "escape"}<span class="mk mk--esc">{tok.text === "{" ? "{{" : "}}"}</span>{:else}<span
            class="mk"
            class:mk--bad={tok.issue !== null}>{tok.raw}</span
          >{/if}
      {/each}{"​"}
    </div>
  </div>
  <textarea
    bind:this={area}
    class="area selectable"
    {id}
    {placeholder}
    aria-describedby={describedby}
    aria-invalid={invalid || undefined}
    spellcheck="false"
    autocomplete="off"
    rows="8"
    {value}
    oninput={(e) => oninput((e.currentTarget as HTMLTextAreaElement).value)}
    onscroll={(e) => {
      scrollTop = (e.currentTarget as HTMLTextAreaElement).scrollTop;
      scrollLeft = (e.currentTarget as HTMLTextAreaElement).scrollLeft;
    }}
  ></textarea>
</div>

<style>
  /* Multi-line field: r12 squircle (--r-field), the concentric ring while typing. */
  .body {
    position: relative;
    border-radius: var(--r-field);
    background: var(--color-track);
    transition: background-color var(--dur-fast) var(--ease-out-soft);
  }
  .body:hover:not(:focus-within) {
    background: var(--color-hover);
  }
  .body:focus-within {
    outline: var(--focus-ring);
    outline-offset: var(--focus-offset);
  }
  .body--invalid {
    background: var(--color-raised);
    box-shadow: var(--shadow-on-lift);
  }
  /* Mirror and textarea share every metric that affects line breaking. */
  .mirror__inner,
  .area {
    margin: 0;
    padding: 10px 14px;
    font-family: var(--font-mono);
    font-size: var(--text-secondary);
    line-height: 1.6;
    letter-spacing: 0;
    white-space: pre-wrap;
    overflow-wrap: break-word;
    word-break: normal;
    tab-size: 4;
  }
  /* overflow:hidden + a stable gutter reserve the same scrollbar space as the textarea,
     so both wrap lines at the same width. */
  .mirror {
    position: absolute;
    inset: 0;
    overflow: hidden;
    scrollbar-gutter: stable;
    border-radius: inherit;
    pointer-events: none;
  }
  .mirror__inner {
    color: transparent;
  }
  /* Placeholder marks: a wash behind the token (no stroke). */
  .mk {
    border-radius: var(--radius-xs);
    background: color-mix(in srgb, var(--color-ink) 14%, transparent);
  }
  .mk--esc {
    background: color-mix(in srgb, var(--color-ink) 7%, transparent);
  }
  .mk--bad {
    background: transparent;
    text-decoration: underline wavy;
    text-decoration-color: var(--color-ink-2);
    text-underline-offset: 3px;
  }
  .area {
    position: relative;
    display: block;
    width: 100%;
    min-height: 168px;
    max-height: 420px;
    border: 0;
    border-radius: inherit;
    background: transparent;
    color: var(--color-ink);
    caret-color: var(--color-ink);
    outline: none;
    /* Grows with the text (no resize grip in the corner). */
    field-sizing: content;
    resize: none;
    overflow: auto;
    scrollbar-gutter: stable;
  }
  .area::placeholder {
    color: var(--color-ink-3);
  }
</style>
