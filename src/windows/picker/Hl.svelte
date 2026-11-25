<script lang="ts">
  // Text with the search matches highlighted (monochrome wash — no colour).
  import { segments } from "./highlight";

  interface Props {
    text: string;
    terms: readonly string[];
  }
  let { text, terms }: Props = $props();

  const parts = $derived(terms.length ? segments(text, terms) : null);
</script>

{#if parts}{#each parts as part, i (i)}{#if part.hit}<mark class="hl">{part.text}</mark>{:else}{part.text}{/if}{/each}{:else}{text}{/if}

<style>
  .hl {
    padding: 0 1px;
    border-radius: 3px;
    background: color-mix(in srgb, var(--color-ink) 16%, transparent);
    color: var(--color-ink);
    font-weight: 600;
  }
</style>
