<script lang="ts" module>
  export interface RowIds {
    labelledby: string;
    describedby: string | undefined;
  }
</script>

<script lang="ts">
  // One setting inside a Group tile: label + short explanation on the left, the control on the
  // right (or below with `stack`), an optional error / note line. The row is an ARIA group
  // labelled by the title and described by the explanation and the error; the `control`
  // snippet also receives `{ labelledby, describedby }` so a control can point at them.
  // Geometry: the row is inset --pad-tile in the tile, so anything that fills it (hover wash,
  // error box) uses --r-tile-in; an icon well sits ≥ 16 px from the corner → scale radius.
  import type { Snippet } from "svelte";
  import Ico from "$lib/components/Ico.svelte";
  import type { IconName } from "$lib/icons";

  interface Props {
    label: string;
    desc?: string | null;
    /** Error message shown under the row (announced). */
    error?: string | null;
    /** Neutral note under the row (e.g. a warning that is not an error). */
    note?: string | null;
    icon?: IconName;
    /** Put the control under the text, full width. */
    stack?: boolean;
    disabled?: boolean;
    control?: Snippet<[RowIds]>;
    /** Extra full-width content under the text/control. */
    children?: Snippet;
    /** Full-width content after the error / note (e.g. the way out of that error). */
    footer?: Snippet;
  }
  let { label, desc, error, note, icon, stack = false, disabled = false, control, children, footer }: Props = $props();

  const uid = $props.id();
  const describedBy = $derived(
    [desc ? `${uid}-d` : null, note ? `${uid}-n` : null, error ? `${uid}-e` : null].filter(Boolean).join(" ") || undefined,
  );
  const ids = $derived<RowIds>({ labelledby: `${uid}-l`, describedby: describedBy });
</script>

<div
  class="row"
  class:row--stack={stack}
  class:row--disabled={disabled}
  class:row--error={!!error}
  role="group"
  aria-labelledby="{uid}-l"
  aria-describedby={describedBy}
>
  <div class="row__main">
    {#if icon}<span class="row__icon squircle"><Ico name={icon} size={16} /></span>{/if}
    <div class="row__text">
      <div class="row__label" id="{uid}-l">{label}</div>
      {#if desc}<div class="row__desc" id="{uid}-d">{desc}</div>{/if}
    </div>
    {#if control && !stack}<div class="row__control">{@render control(ids)}</div>{/if}
  </div>
  {#if control && stack}<div class="row__below" class:row__below--indent={!!icon}>{@render control(ids)}</div>{/if}
  {#if children}<div class="row__below" class:row__below--indent={!!icon}>{@render children()}</div>{/if}
  {#if error}
    <div class="row__error squircle" id="{uid}-e" role="alert"><Ico name="warning" size={14} /><span>{error}</span></div>
  {/if}
  {#if note}
    <div class="row__note" id="{uid}-n"><Ico name="info" size={14} /><span>{note}</span></div>
  {/if}
  {#if footer}{@render footer()}{/if}
</div>

<style>
  .row {
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-height: 60px;
    justify-content: center;
    padding: 12px;
    border-radius: var(--r-tile-in);
  }
  .row__main {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 10px 16px;
  }
  .row__icon {
    display: inline-flex;
    flex: 0 0 auto;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    margin-right: -4px;
    border-radius: var(--radius-sm);
    background: var(--color-track);
    color: var(--color-ink);
  }
  .row__text {
    flex: 1 1 240px;
    min-width: 0;
  }
  .row__label {
    color: var(--color-ink);
    font-size: var(--text-row);
    font-weight: 600;
    line-height: var(--text-row--line-height);
  }
  .row__desc {
    margin-top: 2px;
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
    line-height: var(--text-secondary--line-height);
    text-wrap: pretty;
  }
  .row__control {
    display: flex;
    flex: 0 1 auto;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    min-width: 0;
    margin-left: auto;
  }
  .row__below {
    min-width: 0;
  }
  .row__below--indent {
    padding-left: 44px;
  }
  .row--disabled .row__label,
  .row--disabled .row__desc,
  .row--disabled .row__icon {
    opacity: 0.55;
  }
  .row__note,
  .row__error {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    font-size: var(--text-secondary);
    line-height: var(--text-secondary--line-height);
  }
  .row__note {
    color: var(--color-ink-2);
  }
  .row__note :global(.ico),
  .row__error :global(.ico) {
    margin-top: 2px;
  }
  .row__error {
    padding: 8px 12px;
    border-radius: var(--radius-sm);
    background: var(--color-track);
    color: var(--color-ink);
    font-weight: 600;
    animation: err-in var(--dur-base) var(--ease-out-soft);
  }
  @keyframes err-in {
    from {
      opacity: 0;
      transform: translateY(-3px);
    }
  }
</style>
