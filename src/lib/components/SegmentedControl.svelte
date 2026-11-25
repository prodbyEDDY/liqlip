<script lang="ts" module>
  export interface SegmentOption {
    value: string;
    label: string;
  }
</script>

<script lang="ts">
  interface Props {
    options: SegmentOption[];
    value: string;
    onChange?: (value: string) => void;
    ariaLabel?: string;
    disabled?: boolean;
    /** Fill the parent width. Default true. */
    full?: boolean;
  }
  let {
    options,
    value,
    onChange,
    ariaLabel,
    disabled = false,
    full = true,
  }: Props = $props();

  // A single white pill slides between segments (measured, so it fits each
  // segment exactly even when labels differ in width).
  let segEl = $state<HTMLDivElement>();
  let btns: HTMLButtonElement[] = [];
  let pillX = $state(0);
  let pillW = $state(0); // 0 → hidden
  let armed = $state(false);
  let placed = false;

  function measure() {
    const i = options.findIndex((o) => o.value === value);
    const b = btns[i];
    if (!segEl || !b) {
      pillW = 0; // no match → hide (keep last x)
      return;
    }
    pillX = b.offsetLeft;
    pillW = b.offsetWidth;
    if (!placed) {
      placed = true;
      requestAnimationFrame(() => (armed = true));
    }
  }

  $effect(() => {
    void value;
    void options;
    measure();
  });

  $effect(() => {
    if (!segEl) return;
    const ro = new ResizeObserver(() => measure());
    ro.observe(segEl);
    return () => ro.disconnect();
  });
</script>

<div
  class="seg squircle"
  class:seg--full={full}
  bind:this={segEl}
  role="radiogroup"
  aria-label={ariaLabel}
  aria-disabled={disabled}
>
  <span
    class="seg__pill squircle"
    class:seg__pill--ready={pillW > 0}
    class:seg__pill--armed={armed}
    style={`transform: translateX(${pillX}px); width: ${pillW}px`}
    aria-hidden="true"
  ></span>

  {#each options as opt, i (opt.value)}
    {@const active = opt.value === value}
    <button
      type="button"
      role="radio"
      aria-checked={active}
      class="seg__opt"
      class:seg__opt--active={active}
      bind:this={btns[i]}
      {disabled}
      onclick={() => onChange?.(opt.value)}
    >
      {opt.label}
    </button>
  {/each}
</div>

<style>
  .seg {
    position: relative;
    display: inline-flex;
    align-items: center;
    gap: 3px;
    padding: 3px;
    border-radius: 13px;
    background: var(--color-track);
  }
  .seg--full {
    display: flex;
    width: 100%;
  }
  /* Sliding active pill, behind the labels. */
  .seg__pill {
    position: absolute;
    top: 3px;
    bottom: 3px;
    left: 0;
    z-index: 0;
    border-radius: 10px;
    background: var(--color-paper);
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.13); /* .pen seg pill: blur 2 */
    opacity: 0;
    pointer-events: none;
  }
  .seg__pill--ready {
    opacity: 1;
  }
  .seg__pill--armed {
    transition:
      transform var(--dur-pill) var(--ease-pill),
      width var(--dur-pill) var(--ease-pill);
  }
  .seg__opt {
    position: relative;
    z-index: 1;
    flex: 1 1 auto;
    height: 36px;
    padding: 0 14px;
    border: 0;
    border-radius: 10px;
    background: transparent;
    color: var(--color-ink-2);
    font: inherit;
    font-size: var(--text-secondary);
    font-weight: 600; /* .pen segment labels are 600 */
    white-space: nowrap;
    cursor: default;
    transition: color var(--dur-base) var(--ease-out-soft);
  }
  .seg__opt:hover:not(.seg__opt--active):not(:disabled) {
    color: var(--color-ink);
  }
  .seg__opt--active {
    color: var(--color-ink);
  }
  .seg[aria-disabled="true"] {
    opacity: 0.5;
  }
  @media (prefers-reduced-motion: reduce) {
    .seg__pill--armed {
      transition: none;
    }
  }
</style>
