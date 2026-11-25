<script lang="ts">
  // Empty / error state that teaches: an icon well (a small composition of design-system
  // icons, not a lone glyph), a title, one line of explanation and the way out (actions).
  // `compact` for tight spots (a strip, a list inside a tile).
  import type { Snippet } from "svelte";
  import Ico from "$lib/components/Ico.svelte";
  import type { IconName } from "$lib/icons";

  interface Props {
    icon?: IconName;
    /** Two quieter icons behind the main one (the "illustration"). */
    companions?: [IconName, IconName];
    title: string;
    desc?: string;
    compact?: boolean;
    children?: Snippet;
  }
  let { icon, companions, title, desc, compact = false, children }: Props = $props();
</script>

<div class="empty" class:empty--compact={compact}>
  {#if icon}
    <div class="empty__art" aria-hidden="true">
      {#if companions && !compact}
        <span class="empty__ghost empty__ghost--l squircle"><Ico name={companions[0]} size={18} /></span>
        <span class="empty__ghost empty__ghost--r squircle"><Ico name={companions[1]} size={18} /></span>
      {/if}
      <span class="empty__icon squircle"><Ico name={icon} size={compact ? 18 : 22} /></span>
    </div>
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
    gap: 6px;
    max-width: 340px;
    margin: 0 auto;
    padding: 24px;
    text-align: center;
  }
  .empty--compact {
    padding: 16px;
    gap: 4px;
  }
  .empty__art {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 120px;
    height: 56px;
    margin-bottom: 8px;
  }
  .empty--compact .empty__art {
    width: 40px;
    height: 40px;
    margin-bottom: 6px;
  }
  .empty__icon,
  .empty__ghost {
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--radius-lg);
  }
  .empty__icon {
    position: relative;
    z-index: 1;
    width: 56px;
    height: 56px;
    background: var(--color-raised);
    box-shadow: var(--shadow-card);
    color: var(--color-ink);
  }
  .empty--compact .empty__icon {
    width: 40px;
    height: 40px;
    border-radius: var(--radius-md);
    background: var(--color-track);
    box-shadow: none;
    color: var(--color-ink-2);
  }
  .empty__ghost {
    position: absolute;
    top: 10px;
    width: 40px;
    height: 40px;
    border-radius: var(--radius-md);
    background: var(--color-track);
    color: var(--color-ink-3);
  }
  .empty__ghost--l {
    left: 8px;
    transform: rotate(-8deg);
  }
  .empty__ghost--r {
    right: 8px;
    transform: rotate(8deg);
  }
  .empty__title {
    color: var(--color-ink);
    font-size: var(--text-panel);
    font-weight: 600;
    line-height: var(--text-panel--line-height);
    text-wrap: balance;
  }
  .empty--compact .empty__title {
    font-size: var(--text-row);
  }
  .empty__desc {
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
    line-height: var(--text-secondary--line-height);
    text-wrap: pretty;
  }
  .empty__action {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 8px;
    margin-top: 10px;
  }
</style>
