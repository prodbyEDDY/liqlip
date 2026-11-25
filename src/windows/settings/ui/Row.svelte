<script lang="ts">
  // One setting: label + short explanation on the left, the control on the right (or below
  // with `stack`), an optional error line. The row is an ARIA group labelled by the title
  // and described by the explanation and the error, so a screen reader announces both when
  // focus enters the control.
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
    control?: Snippet;
    /** Extra full-width content under the text/control. */
    children?: Snippet;
  }
  let { label, desc, error, note, icon, stack = false, disabled = false, control, children }: Props = $props();

  const uid = $props.id();
  const describedBy = $derived(
    [desc ? `${uid}-d` : null, note ? `${uid}-n` : null, error ? `${uid}-e` : null].filter(Boolean).join(" ") || undefined,
  );
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
    {#if icon}<span class="row__icon"><Ico name={icon} size={18} /></span>{/if}
    <div class="row__text">
      <div class="row__label" id="{uid}-l">{label}</div>
      {#if desc}<div class="row__desc" id="{uid}-d">{desc}</div>{/if}
    </div>
    {#if control && !stack}<div class="row__control">{@render control()}</div>{/if}
  </div>
  {#if control && stack}<div class="row__below">{@render control()}</div>{/if}
  {#if children}<div class="row__below">{@render children()}</div>{/if}
  {#if note}
    <div class="row__note" id="{uid}-n"><Ico name="info" size={14} />{note}</div>
  {/if}
  {#if error}
    <div class="row__error" id="{uid}-e" role="alert"><Ico name="warning" size={14} /><span>{error}</span></div>
  {/if}
</div>

<style>
  .row {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 14px 16px;
    background: var(--s-tile);
    transition: background-color var(--dur-fast) var(--ease-out-soft);
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
    align-self: flex-start;
    margin-top: 1px;
    color: var(--color-ink-2);
  }
  .row__text {
    flex: 1 1 260px;
    min-width: 0;
  }
  .row__label {
    font-size: var(--text-row);
    font-weight: 600;
    line-height: 1.4;
    color: var(--color-ink);
  }
  .row__desc {
    margin-top: 2px;
    font-size: var(--text-secondary);
    line-height: 1.45;
    color: var(--color-ink-3);
  }
  .row__control {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    flex: 0 1 auto;
    min-width: 0;
    margin-left: auto;
  }
  .row__below {
    min-width: 0;
  }
  .row--disabled .row__label,
  .row--disabled .row__desc {
    opacity: 0.55;
  }
  .row__note,
  .row__error {
    display: flex;
    align-items: flex-start;
    gap: 7px;
    font-size: var(--text-secondary);
    line-height: 1.45;
  }
  .row__note {
    color: var(--color-ink-2);
  }
  .row__note :global(.ico),
  .row__error :global(.ico) {
    margin-top: 2px;
  }
  .row__error {
    padding: 8px 10px;
    border-radius: 10px;
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
