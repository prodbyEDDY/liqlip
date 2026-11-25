<script lang="ts">
  // Placeholder reference (SPEC §3.10): every placeholder and syntax form with a live
  // example of what it expands to right now; clicking inserts it at the caret.
  import Ico from "$lib/components/Ico.svelte";
  import { t, type TranslationKey } from "$lib/i18n/index.svelte";
  import { expandSnippet, type ExpandContext } from "$lib/snippets/expand";

  interface Props {
    ctx: ExpandContext;
    onInsert: (text: string) => void;
  }
  let { ctx, onInsert }: Props = $props();

  let open = $state(true);
  const id = $props.id();

  // [inserted text, description key, show an example]
  const ROWS: [string, string, boolean][] = [
    ["{clipboard}", "settings.snippets.ph.clipboard", true],
    ["{date}", "settings.snippets.ph.date", true],
    ["{time}", "settings.snippets.ph.time", true],
    ["{datetime}", "settings.snippets.ph.datetime", true],
    ["{weekday}", "settings.snippets.ph.weekday", true],
    ["{uuid}", "settings.snippets.ph.uuid", true],
    ["{cursor}", "settings.snippets.ph.cursor", false],
    ['{date format="d MMMM yyyy"}', "settings.snippets.ph.format", true],
    ['{date offset="+1d"}', "settings.snippets.ph.offset", true],
    ["{clipboard | trim | uppercase}", "settings.snippets.ph.modifiers", true],
    ["{{", "settings.snippets.ph.escape", false],
  ];

  const example = (text: string) => {
    const out = expandSnippet(text, ctx).text;
    return out.length > 48 ? out.slice(0, 47) + "…" : out;
  };
</script>

<section class="ref">
  <button type="button" class="ref__head" aria-expanded={open} aria-controls={id} onclick={() => (open = !open)}>
    <span class="ref__title">{t("settings.snippets.placeholders")}</span>
    <span class="ref__chev" class:ref__chev--open={open}><Ico name="chevronDown" size={15} /></span>
  </button>
  {#if open}
    <ul class="ref__list" {id}>
      {#each ROWS as [text, desc, showExample] (text)}
        <li>
          <button
            type="button"
            class="ph"
            aria-label={t("settings.snippets.insert", { ph: text })}
            title={t("settings.snippets.insert", { ph: text })}
            onclick={() => onInsert(text)}
          >
            <code class="ph__code">{text}</code>
            <span class="ph__desc">{t(desc as TranslationKey)}</span>
            {#if showExample}<span class="ph__ex">→ {example(text)}</span>{/if}
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .ref {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .ref__head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: 4px 0;
    border: 0;
    background: transparent;
    color: var(--color-ink);
    font: inherit;
  }
  .ref__head:focus-visible {
    border-radius: var(--radius-sm);
  }
  .ref__title {
    font-size: var(--text-row);
    font-weight: 600;
  }
  .ref__chev {
    display: inline-flex;
    color: var(--color-ink-3);
    transition: transform var(--dur-base) var(--ease-out-soft);
  }
  .ref__chev--open {
    transform: rotate(180deg);
  }
  .ref__list {
    display: grid;
    grid-template-columns: 1fr;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .ph {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    grid-template-areas: "code ex" "desc desc";
    align-items: baseline;
    gap: 2px 12px;
    width: 100%;
    padding: 8px 12px;
    border: 0;
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--color-ink);
    font: inherit;
    text-align: left;
    transition: background-color var(--dur-fast) var(--ease-out-soft);
  }
  .ph:hover {
    background: var(--color-track);
  }
  .ph:active {
    background: var(--color-hover);
  }
  .ph__code {
    grid-area: code;
    font-family: var(--font-mono);
    font-size: var(--text-secondary);
    font-weight: 600;
  }
  .ph__ex {
    grid-area: ex;
    justify-self: end;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-mono);
    font-size: var(--text-meta);
    color: var(--color-ink-3);
  }
  .ph__desc {
    grid-area: desc;
    font-size: var(--text-secondary);
    line-height: 1.45;
    color: var(--color-ink-3);
  }
</style>
