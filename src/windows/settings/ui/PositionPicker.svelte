<script lang="ts">
  // Shelf position: five tiles, each a mini screen with the shelf drawn where it opens. A
  // radio group: ←/→/↑/↓ move and select, like native radios. Chosen = a raised tile with
  // the shelf in ink; the others are quiet until hovered.
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

  // Shelf rectangle inside a 64×40 screen, per position (screen inset 4, radius-concentric).
  const SHELF: Record<Position, { x: number; y: number; w: number; h: number }> = {
    center: { x: 19, y: 9, w: 26, h: 22 },
    left: { x: 4, y: 4, w: 16, h: 32 },
    right: { x: 44, y: 4, w: 16, h: 32 },
    top: { x: 10, y: 4, w: 44, h: 12 },
    bottom: { x: 10, y: 24, w: 44, h: 12 },
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
      class="pos__item squircle"
      title={opt.label}
      class:pos__item--sel={sel}
      onclick={() => onChange(opt.value)}
    >
      <svg class="pos__map" width="64" height="40" viewBox="0 0 64 40" aria-hidden="true">
        <rect class="pos__screen" width="64" height="40" rx="8" />
        <rect class="pos__shelf" x={r.x} y={r.y} width={r.w} height={r.h} rx="4" />
      </svg>
      <span class="pos__label">{opt.label}</span>
    </button>
  {/each}
</div>

<style>
  .pos {
    display: grid;
    grid-template-columns: repeat(5, minmax(0, 1fr));
    gap: 6px;
  }
  .pos__item {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    min-width: 0;
    padding: 10px 6px 8px;
    border: 0;
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--color-ink-2);
    font: inherit;
    transition:
      background-color var(--dur-fast) var(--ease-out-soft),
      box-shadow var(--dur-fast) var(--ease-out-soft),
      color var(--dur-fast) var(--ease-out-soft);
  }
  .pos__item:hover:not(.pos__item--sel) {
    background: var(--color-track);
    color: var(--color-ink);
  }
  .pos__item--sel {
    background: var(--color-raised);
    box-shadow: var(--shadow-card);
    color: var(--color-ink);
  }
  .pos__map {
    flex: 0 0 auto;
    max-width: 100%;
    height: auto;
  }
  .pos__screen {
    fill: var(--color-track);
  }
  .pos__shelf {
    fill: color-mix(in srgb, var(--color-ink) 28%, transparent);
    transition: fill var(--dur-fast) var(--ease-out-soft);
  }
  .pos__item:hover .pos__shelf {
    fill: color-mix(in srgb, var(--color-ink) 45%, transparent);
  }
  .pos__item--sel .pos__shelf,
  .pos__item--sel:hover .pos__shelf {
    fill: var(--color-ink);
  }
  .pos__label {
    max-width: 100%;
    overflow: hidden;
    font-size: var(--text-secondary);
    font-weight: 600;
    line-height: 1.3;
    text-align: center;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  @media (max-width: 760px) {
    .pos {
      grid-template-columns: repeat(auto-fill, minmax(96px, 1fr));
    }
  }
</style>
