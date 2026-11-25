<script lang="ts">
  // Files: the format family icon (a folder when there's no extension) in a squircle well, the
  // first file's name (two lines, broken anywhere — file names rarely have spaces), "+N more"
  // when several files were copied together, and the tail of the folder it lives in
  // (`ClipMeta.dir`, FIX-08 — "…\me\Documents"; the full path in the tooltip). Records without
  // `dir` (older ones before the store fills it) show no folder line.
  import Ico from "$lib/components/Ico.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import Hl from "../Hl.svelte";
  import { fileIcon, pathTail } from "../preview";

  interface Props {
    name: string;
    format: string;
    more: number;
    /** Folder of the first file (`ClipMeta.dir`), if known. */
    dir?: string | null;
    terms: readonly string[];
  }
  let { name, format, more, dir = null, terms }: Props = $props();

  const tail = $derived(pathTail(dir));
</script>

<div class="body">
  <div class="top">
    <span class="well squircle" aria-hidden="true"><Ico name={fileIcon(format)} size={20} /></span>
    {#if more > 0}<span class="more">{t("picker.card.moreFiles", { n: more })}</span>{/if}
  </div>
  <div class="foot">
    <span class="name"><Hl text={name} {terms} /></span>
    {#if tail}<span class="dir" title={dir}>{tail}</span>{/if}
  </div>
</div>

<style>
  .body {
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    gap: 8px;
    width: 100%;
    height: 100%;
    padding: 12px;
  }
  .top {
    display: flex;
    flex: 0 0 auto;
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
  .foot {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    min-height: 0;
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
  /* The folder: one quiet line (like a link's path under its host), ellipsis + tooltip. */
  .dir {
    overflow: hidden;
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
    line-height: 1.4;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
