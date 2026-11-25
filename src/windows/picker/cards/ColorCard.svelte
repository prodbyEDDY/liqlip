<script lang="ts">
  // The only place colour is allowed: it IS the user's content (Liquid Tools ColorCard). The
  // value chip is functional mono, pinned light because it sits over an arbitrary colour.
  interface Props {
    /** Re-validated CSS colour, or null → the neutral track. */
    css: string | null;
    value: string;
  }
  let { css, value }: Props = $props();
</script>

<!-- A translucent colour (`#8B5CF6CC`, `rgba(…, 0.8)`) shows its alpha over the checkerboard. -->
<div class="checker">
  <div class="swatch" style:background={css ?? "var(--color-track)"}>
    <span class="hex">{value}</span>
  </div>
</div>

<style>
  .checker {
    width: 100%;
    height: 100%;
    background-color: var(--color-paper);
    background-image: conic-gradient(
      var(--color-checker) 25%,
      transparent 0 50%,
      var(--color-checker) 0 75%,
      transparent 0
    );
    background-size: 16px 16px;
  }
  .swatch {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .hex {
    max-width: 100%;
    margin: 10px;
    padding: 5px 8px;
    overflow: hidden;
    border-radius: 6px;
    background: rgb(0 0 0 / 0.55);
    font-family: var(--font-mono);
    font-size: var(--text-meta);
    text-overflow: ellipsis;
    white-space: nowrap;
    /* over user colour — pinned light in both themes */
    color: #fff;
    text-shadow: 0 1px 2px rgb(0 0 0 / 0.35);
  }
</style>
