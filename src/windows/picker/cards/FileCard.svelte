<script lang="ts">
  // Files: the format family icon (a folder when there's no extension) in a squircle well, the
  // first file's name (two lines, broken anywhere — file names rarely have spaces), and
  // "+N more" when several files were copied together.
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
  <div class="top">
    <span class="well squircle" aria-hidden="true"><Ico name={fileIcon(format)} size={20} /></span>
    {#if more > 0}<span class="more">{t("picker.card.moreFiles", { n: more })}</span>{/if}
  </div>
  <span class="name"><Hl text={name} {terms} /></span>
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
  .top {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 8px;
  }
  .well {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 36px;
    border-radius: var(--radius-sm);
    background: var(--color-track);
    color: var(--color-ink);
  }
  .more {
    display: inline-flex;
    align-items: center;
    height: 20px;
    padding: 0 8px;
    border-radius: 999px;
    background: var(--color-track);
    color: var(--color-ink-2);
    font-size: var(--text-meta);
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .name {
    display: -webkit-box;
    overflow: hidden;
    color: var(--color-ink);
    font-size: var(--text-row);
    font-weight: 600;
    line-height: 1.35;
    overflow-wrap: anywhere;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
  }
</style>
