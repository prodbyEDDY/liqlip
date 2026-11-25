<script lang="ts">
  // A titled group of setting rows. Rows sit on `panel-2` tiles separated by 2 px of the
  // `content` ground — no borders or dividers (monochrome lock).
  import type { Snippet } from "svelte";

  interface Props {
    title?: string;
    desc?: string;
    /** Trailing header content (e.g. a button). */
    aside?: Snippet;
    children: Snippet;
  }
  let { title, desc, aside, children }: Props = $props();
  const id = $props.id();
</script>

<section class="group" aria-labelledby={title ? id : undefined}>
  {#if title || aside}
    <header class="group__head">
      <div class="group__text">
        {#if title}<h2 class="group__title" {id}>{title}</h2>{/if}
        {#if desc}<p class="group__desc">{desc}</p>{/if}
      </div>
      {#if aside}<div class="group__aside">{@render aside()}</div>{/if}
    </header>
  {/if}
  <div class="group__body squircle">
    {@render children()}
  </div>
</section>

<style>
  .group {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .group + :global(.group) {
    margin-top: 26px;
  }
  .group__head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 12px;
    padding: 0 4px;
  }
  .group__text {
    min-width: 0;
  }
  .group__title {
    margin: 0;
    font-size: var(--text-panel);
    font-weight: 600;
    color: var(--color-ink);
  }
  .group__desc {
    margin: 3px 0 0;
    font-size: var(--text-secondary);
    line-height: 1.45;
    color: var(--color-ink-3);
  }
  .group__aside {
    flex: 0 0 auto;
  }
  .group__body {
    display: flex;
    flex-direction: column;
    gap: 2px;
    border-radius: 16px;
    overflow: hidden;
  }
</style>
