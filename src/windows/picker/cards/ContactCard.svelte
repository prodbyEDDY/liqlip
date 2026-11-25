<script lang="ts">
  // Email or phone: the kind's symbol and the value large. An address splits at "@" (name on
  // the first line, the domain quieter below) so it never breaks mid-word; a phone number
  // stays on one line.
  import Ico from "$lib/components/Ico.svelte";
  import Hl from "../Hl.svelte";

  interface Props {
    value: string;
    email: boolean;
    terms: readonly string[];
  }
  let { value, email, terms }: Props = $props();

  const clean = $derived(value.replace(/^(mailto:|tel:)/i, "").trim());
  const at = $derived(email ? clean.lastIndexOf("@") : -1);
</script>

<div class="body">
  <span class="symbol" aria-hidden="true"><Ico name={email ? "mail" : "phone"} size={16} /></span>
  {#if at > 0}
    <span class="value">
      <span class="value__main"><Hl text={clean.slice(0, at)} {terms} /></span>
      <span class="value__sub"><Hl text={clean.slice(at)} {terms} /></span>
    </span>
  {:else}
    <span class="value"><span class="value__main value__main--one"><Hl text={clean} {terms} /></span></span>
  {/if}
</div>

<style>
  .body {
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    width: 100%;
    height: 100%;
    padding: 12px;
  }
  .symbol {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border-radius: 999px;
    background: var(--color-track);
    color: var(--color-ink);
  }
  .value {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .value__main,
  .value__sub {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .value__main {
    color: var(--color-ink);
    font-size: var(--text-panel);
    font-weight: 600;
    line-height: 1.3;
  }
  .value__main--one {
    font-variant-numeric: tabular-nums;
  }
  .value__sub {
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
    line-height: 1.4;
  }
</style>
