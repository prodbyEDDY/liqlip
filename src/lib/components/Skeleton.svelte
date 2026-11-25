<script lang="ts">
  interface Props {
    width?: string;
    height?: string;
    radius?: number;
    circle?: boolean;
  }
  let { width = "100%", height = "12px", radius = 8, circle = false }: Props = $props();
</script>

<span
  class="skeleton"
  class:skeleton--circle={circle}
  style={`width:${width};height:${height};border-radius:${circle ? "999px" : radius + "px"};`}
  aria-hidden="true"
></span>

<style>
  .skeleton {
    display: block;
    background: var(--color-track);
    animation: skeleton-pulse 1.4s var(--ease-out-soft) infinite;
  }
  .skeleton--circle {
    flex: 0 0 auto;
  }
  @keyframes skeleton-pulse {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.55;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .skeleton {
      animation: none;
      opacity: 0.8;
    }
  }
</style>
