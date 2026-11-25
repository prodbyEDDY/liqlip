<script lang="ts">
  // Loading placeholder block. Callers pass the geometry of what will appear (radius from
  // the scale, or `circle` for capsules) — skeletons mirror the real layout, never a spinner.
  interface Props {
    width?: string;
    height?: string;
    radius?: number | string;
    circle?: boolean;
  }
  let { width = "100%", height = "12px", radius = 6, circle = false }: Props = $props();
  const r = $derived(circle ? "999px" : typeof radius === "number" ? `${radius}px` : radius);
</script>

<span class="skeleton" class:skeleton--circle={circle} style:width style:height style:border-radius={r} aria-hidden="true"></span>

<style>
  .skeleton {
    display: block;
    flex: none;
    background: var(--color-track);
    animation: skeleton-pulse 1.4s var(--ease-in-out) infinite;
  }
  @keyframes skeleton-pulse {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.5;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .skeleton {
      animation: none;
    }
  }
</style>
