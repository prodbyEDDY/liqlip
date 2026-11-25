<script lang="ts">
  // Shelf position: the big-type list of Liquid Tools (`BigTypeList`, ported with its
  // check/label choreography) with a mini screen diagram per option. A radio group:
  // ←/→/↑/↓ move and select, like native radios.
  import Ico from "$lib/components/Ico.svelte";
  import type { Position } from "$lib/ipc/types";
  import { stepIndex } from "../logic";

  interface Props {
    value: Position;
    options: { value: Position; label: string }[];
    onChange: (v: Position) => void;
    ariaLabelledby?: string;
  }
  let { value, options, onChange, ariaLabelledby }: Props = $props();

  function onKeydown(e: KeyboardEvent) {
    const i = options.findIndex((o) => o.value === value);
    let d = 0;
    if (e.key === "ArrowDown" || e.key === "ArrowRight") d = 1;
    else if (e.key === "ArrowUp" || e.key === "ArrowLeft") d = -1;
    if (!d) return;
    e.preventDefault();
    const n = stepIndex(i, d, options.length);
    onChange(options[n].value);
    (e.currentTarget as HTMLElement).querySelectorAll<HTMLButtonElement>("[role=radio]")[n]?.focus();
  }

  // Shelf rectangle inside a 32×22 screen, per position.
  const SHELF: Record<Position, { x: number; y: number; w: number; h: number }> = {
    center: { x: 9, y: 5, w: 14, h: 12 },
    left: { x: 3, y: 3, w: 8, h: 16 },
    right: { x: 21, y: 3, w: 8, h: 16 },
    top: { x: 5, y: 3, w: 22, h: 6 },
    bottom: { x: 5, y: 13, w: 22, h: 6 },
  };
</script>

<!-- svelte-ignore a11y_interactive_supports_focus -->
<div class="pos" role="radiogroup" aria-labelledby={ariaLabelledby} onkeydown={onKeydown}>
  {#each options as opt (opt.value)}
    {@const sel = opt.value === value}
    {@const r = SHELF[opt.value]}
    <button
      type="button"
      role="radio"
      aria-checked={sel}
      tabindex={sel ? 0 : -1}
      class="pos__item"
      class:pos__item--sel={sel}
      onclick={() => onChange(opt.value)}
    >
      <span class="pos__ck" aria-hidden="true"><Ico name="check" size={16} /></span>
      <svg class="pos__map" width="32" height="22" viewBox="0 0 32 22" aria-hidden="true">
        <rect class="pos__screen" width="32" height="22" rx="5" />
        <rect class="pos__shelf" x={r.x} y={r.y} width={r.w} height={r.h} rx="2.5" />
      </svg>
      <span class="pos__label">{opt.label}</span>
    </button>
  {/each}
</div>

<style>
  .pos {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .pos__item {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 4px 0;
    border: 0;
    background: transparent;
    font: inherit;
    font-size: 17px;
    font-weight: 700;
    text-align: left;
    color: var(--color-ink-3);
    cursor: default;
    transition: color var(--dur-base) var(--ease-out-soft);
  }
  .pos__item:hover:not(.pos__item--sel) {
    color: var(--color-ink-2);
  }
  .pos__item--sel {
    color: var(--color-ink);
  }
  /* Same choreography as BigTypeList: the check grows in while the rest slides right. */
  .pos__ck {
    display: inline-flex;
    flex: 0 0 auto;
    width: 0;
    margin-right: -12px;
    opacity: 0;
    transform: scale(0.6);
    transform-origin: left center;
    overflow: hidden;
    transition:
      width var(--dur-slow) var(--ease-out-soft),
      margin var(--dur-slow) var(--ease-out-soft),
      opacity var(--dur-base) var(--ease-out-soft),
      transform var(--dur-slow) var(--ease-pill);
  }
  .pos__item--sel .pos__ck {
    width: 16px;
    margin-right: 0;
    opacity: 1;
    transform: scale(1);
  }
  .pos__map {
    flex: 0 0 auto;
  }
  .pos__screen {
    fill: color-mix(in srgb, currentColor 16%, transparent);
  }
  .pos__shelf {
    fill: currentColor;
    transition:
      x var(--dur-slow) var(--ease-liquid),
      y var(--dur-slow) var(--ease-liquid);
  }
  .pos__label {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
