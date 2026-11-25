<script lang="ts">
  // A link: the host carries the card (Liquid Tools LinkCard); the full URL is in the meta
  // title and in Quick Look.
  import Ico from "$lib/components/Ico.svelte";
  import Hl from "../Hl.svelte";

  interface Props {
    host: string;
    terms: readonly string[];
  }
  let { host, terms }: Props = $props();
</script>

<div class="body">
  <Ico name="link" size={22} />
  <span class="host"><Hl text={host} {terms} /></span>
</div>

<style>
  .body {
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 12px;
    background: var(--color-content);
    color: var(--color-ink-3);
  }
  .host {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--text-secondary);
    color: var(--color-ink-2);
  }
</style>
