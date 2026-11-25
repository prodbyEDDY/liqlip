<script lang="ts" module>
  export interface SegmentOption {
    value: string;
    label: string;
  }
</script>

<script lang="ts">
  // Segmented choice: a capsule track with a capsule thumb inset 3 px (concentric by
  // construction) that slides to the chosen segment. A radio group: ←/→ move the choice,
  // only the chosen segment is a tab stop. `id` / `describedby` let a settings row label it.
  interface Props {
    options: SegmentOption[];
    value: string;
    onChange?: (value: string) => void;
    ariaLabel?: string;
    id?: string;
    describedby?: string;
    disabled?: boolean;
    /** Fill the parent width. Default true. */
    full?: boolean;
  }
  let { options, value, onChange, ariaLabel, id, describedby, disabled = false, full = true }: Props = $props();

  let segEl = $state<HTMLDivElement>();
  let btns = $state<HTMLButtonElement[]>([]);
  let thumbX = $state(0);
  let thumbW = $state(0); // 0 → hidden
  let armed = $state(false);
  let placed = false;

  function measure() {
    const i = options.findIndex((o) => o.value === value);
    const b = btns[i];
    if (!segEl || !b) {
      thumbW = 0;
      return;
    }
    thumbX = b.offsetLeft;
    thumbW = b.offsetWidth;
    if (!placed) {
      placed = true;
      requestAnimationFrame(() => (armed = true));
    }
  }

  $effect(() => {
    void value;
    void options;
    void btns.length;
    measure();
  });

  $effect(() => {
    if (!segEl) return;
    const ro = new ResizeObserver(() => measure());
    ro.observe(segEl);
    return () => ro.disconnect();
  });

  function onKeydown(e: KeyboardEvent) {
    if (disabled) return;
    const d = e.key === "ArrowRight" || e.key === "ArrowDown" ? 1 : e.key === "ArrowLeft" || e.key === "ArrowUp" ? -1 : 0;
    if (!d) return;
    e.preventDefault();
    const i = options.findIndex((o) => o.value === value);
    const n = (Math.max(0, i) + d + options.length) % options.length;
    onChange?.(options[n].value);
    btns[n]?.focus();
  }
</script>

<!-- svelte-ignore a11y_interactive_supports_focus -->
<div
  class="seg"
  class:seg--full={full}
  bind:this={segEl}
  role="radiogroup"
  {id}
  aria-label={ariaLabel}
  aria-describedby={describedby}
  aria-disabled={disabled}
  onkeydown={onKeydown}
>
  <span
    class="seg__thumb"
    class:seg__thumb--ready={thumbW > 0}
    class:seg__thumb--armed={armed}
    style={`transform: translateX(${thumbX}px); width: ${thumbW}px`}
    aria-hidden="true"
  ></span>

  {#each options as opt, i (opt.value)}
    {@const active = opt.value === value}
    <button
      type="button"
      role="radio"
      aria-checked={active}
      tabindex={active || (i === 0 && !options.some((o) => o.value === value)) ? 0 : -1}
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
    max-width: 100%;
    height: var(--h-lg);
    padding: 3px;
    border-radius: 999px;
    background: var(--color-track);
  }
  .seg--full {
    display: flex;
    width: 100%;
  }
  .seg__thumb {
    position: absolute;
    top: 3px;
    bottom: 3px;
    left: 0;
    z-index: 0;
    border-radius: 999px;
    background: var(--color-thumb);
    box-shadow: var(--shadow-on-lift);
    opacity: 0;
    pointer-events: none;
  }
  .seg__thumb--ready {
    opacity: 1;
  }
  .seg__thumb--armed {
    transition:
      transform var(--dur-pill) var(--ease-pill),
      width var(--dur-pill) var(--ease-pill);
  }
  .seg__opt {
    position: relative;
    z-index: 1;
    flex: 1 1 auto;
    min-width: 0;
    height: 100%;
    padding: 0 14px;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: var(--color-ink-2);
    font: inherit;
    font-size: var(--text-secondary);
    font-weight: 600;
    white-space: nowrap;
    transition: color var(--dur-base) var(--ease-out-soft);
  }
  .seg__opt:hover:not(.seg__opt--active):not(:disabled) {
    color: var(--color-ink);
  }
  .seg__opt--active {
    color: var(--color-ink);
  }
  .seg__opt:focus-visible {
    outline-offset: 0;
  }
  .seg[aria-disabled="true"] {
    opacity: 0.5;
  }
</style>
