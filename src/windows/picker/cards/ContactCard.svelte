<script lang="ts">
  // Email or phone (Liquid Tools ContactCard): a round symbol on paper + the value.
  import Ico from "$lib/components/Ico.svelte";
  import Hl from "../Hl.svelte";

  interface Props {
    value: string;
    email: boolean;
    terms: readonly string[];
  }
  let { value, email, terms }: Props = $props();
</script>

<div class="contact">
  <span class="symbol"><Ico name={email ? "mail" : "phone"} size={30} /></span>
  <span class="value"><Hl text={value.replace(/^(mailto:|tel:)/i, "")} {terms} /></span>
</div>

<style>
  .contact {
    height: 100%;
    display: flex;
    flex-direction: column;
    justify-content: center;
    align-items: center;
    gap: 14px;
    padding: 14px;
    text-align: center;
    font-size: 13px;
    font-weight: 550;
    color: var(--color-ink);
    overflow-wrap: anywhere;
  }
  .symbol {
    display: inline-flex;
    padding: 12px;
    border-radius: 999px;
    background: var(--color-paper);
    color: var(--color-ink-2);
  }
  .value {
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    overflow: hidden;
  }
</style>
