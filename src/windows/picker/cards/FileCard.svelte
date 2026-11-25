<script lang="ts">
  // Files (Liquid Tools FileCard): the format family symbol + the format + the first file's
  // name; "+N" when several files were copied together.
  import Ico from "$lib/components/Ico.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import Hl from "../Hl.svelte";
  import { fileIcon } from "../preview";

  interface Props {
    name: string;
    format: string;
    more: number;
    terms: readonly string[];
  }
  let { name, format, more, terms }: Props = $props();
</script>

<div class="body">
  <div class="file-symbol"><Ico name={fileIcon(format)} size={36} /><b>{format}</b></div>
  <span class="name-row">
    <span class="name"><Hl text={name} {terms} /></span>
    {#if more > 0}<span class="more">{t("picker.card.moreFiles", { n: more })}</span>{/if}
  </span>
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
  .file-symbol {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    color: var(--color-ink-2);
  }
  .file-symbol b {
    font-size: 10px;
    letter-spacing: 0.08em;
  }
  .name-row {
    display: flex;
    align-items: center;
    gap: 5px;
    max-width: 100%;
    min-width: 0;
  }
  .name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--text-secondary);
    color: var(--color-ink-2);
  }
  .more {
    flex: 0 0 auto;
    padding: 2px 6px;
    border-radius: 999px;
    background: var(--color-track);
    color: var(--color-ink-2);
    font-size: 10px;
    font-weight: 650;
    font-variant-numeric: tabular-nums;
  }
</style>
