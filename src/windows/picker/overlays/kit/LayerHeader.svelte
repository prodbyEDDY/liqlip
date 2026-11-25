<script lang="ts">
  // Header row of a layer: optional icon, title (+ quiet count), trailing tools, close. 32 px
  // tall, so the close circle in the layer's corner is concentric (2 × --layer-r-in = 32).
  import type { Snippet } from "svelte";
  import Ico from "$lib/components/Ico.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import type { IconName } from "$lib/icons";

  interface Props {
    title: string;
    id?: string;
    icon?: IconName;
    count?: number | string | null;
    onclose?: () => void;
    tools?: Snippet;
  }
  let { title, id, icon, count = null, onclose, tools }: Props = $props();
</script>

<header class="head">
  {#if icon}<span class="head__icon"><Ico name={icon} size={16} /></span>{/if}
  <h2 class="head__title" {id} {title}>{title}</h2>
  {#if count !== null && count !== undefined}<span class="head__count">{count}</span>{/if}
  {#if tools}<div class="head__tools">{@render tools()}</div>{/if}
  {#if onclose}
    <button type="button" class="head__close" aria-label={t("common.close")} title={t("common.close")} onmousedown={(e) => e.preventDefault()} onclick={onclose}>
      <Ico name="x" size={15} />
    </button>
  {/if}
</header>

<style>
  .head {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    gap: 8px;
    min-height: var(--h-md);
    padding-left: 10px;
  }
  .head:not(:has(.head__close)) {
    padding-right: 10px;
  }
  .head__icon {
    display: inline-flex;
    flex: 0 0 auto;
    color: var(--color-ink-2);
  }
  .head__title {
    flex: 0 1 auto;
    min-width: 0;
    margin: 0;
    overflow: hidden;
    color: var(--color-ink);
    font-size: var(--text-row);
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .head__count {
    flex: 0 0 auto;
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
    font-variant-numeric: tabular-nums;
  }
  .head__tools {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    gap: 4px;
    margin-left: auto;
  }
  .head__close {
    display: inline-flex;
    flex: 0 0 auto;
    align-items: center;
    justify-content: center;
    width: calc(2 * var(--layer-r-in, 16px));
    height: calc(2 * var(--layer-r-in, 16px));
    margin-left: auto;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: var(--color-ink-3);
    transition:
      background-color var(--dur-fast) var(--ease-out-soft),
      color var(--dur-fast) var(--ease-out-soft);
  }
  .head__tools + .head__close {
    margin-left: 0;
  }
  .head__close:hover {
    background: var(--color-track);
    color: var(--color-ink);
  }
  .head__close:active {
    background: var(--color-hover);
  }
</style>
