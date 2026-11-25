<script lang="ts">
  // A titled group of setting rows: a quiet heading above one tile (--color-tile on the
  // window ground, radius --r-tile, padding --pad-tile). Rows inside are separated by space,
  // never by lines (monochrome lock: no borders, no dividers).
  import type { Snippet } from "svelte";

  interface Props {
    title?: string;
    desc?: string;
    /** Trailing header content (e.g. a button). */
    aside?: Snippet;
    /** Content that is not made of rows (lists, stats): padded like a row. */
    plain?: boolean;
    children: Snippet;
  }
  let { title, desc, aside, plain = false, children }: Props = $props();
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
  <div class="group__body squircle" class:group__body--plain={plain}>
    {@render children()}
  </div>
</section>

<style>
  .group {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .group + :global(.group) {
    margin-top: 24px;
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
    color: var(--color-ink);
    font-size: var(--text-row);
    font-weight: 600;
    line-height: var(--text-row--line-height);
  }
  .group__desc {
    margin: 2px 0 0;
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
    line-height: var(--text-secondary--line-height);
  }
  .group__aside {
    flex: 0 0 auto;
  }
  .group__body {
    display: flex;
    flex-direction: column;
    padding: var(--pad-tile);
    border-radius: var(--r-tile);
    background: var(--color-tile);
  }
  .group__body--plain {
    padding: 16px;
  }
</style>
