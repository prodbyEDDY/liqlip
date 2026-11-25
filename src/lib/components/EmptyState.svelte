<script lang="ts">
  import type { Snippet } from "svelte";
  import Ico from "$lib/components/Ico.svelte";
  import type { IconName } from "$lib/icons";

  interface Props {
    icon?: IconName;
    title: string;
    desc?: string;
    children?: Snippet;
  }
  let { icon, title, desc, children }: Props = $props();
</script>

<div class="empty">
  {#if icon}
    <div class="empty__icon squircle"><Ico name={icon} size={22} /></div>
  {/if}
  <div class="empty__title">{title}</div>
  {#if desc}<div class="empty__desc">{desc}</div>{/if}
  {#if children}<div class="empty__action">{@render children()}</div>{/if}
</div>

<style>
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 6px;
    padding: 28px 24px;
    max-width: 280px;
    margin: 0 auto;
  }
  .empty__icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 48px;
    height: 48px;
    margin-bottom: 6px;
    border-radius: 16px;
    background: var(--color-track);
    color: var(--color-ink-3);
  }
  .empty__title {
    font-size: var(--text-panel);
    font-weight: 600;
    color: var(--color-ink);
  }
  .empty__desc {
    font-size: var(--text-secondary);
    line-height: 1.45;
    color: var(--color-ink-3);
  }
  .empty__action {
    margin-top: 10px;
  }
</style>
