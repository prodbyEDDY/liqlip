<script lang="ts">
  // The only place colour is allowed: it IS the user's content. The swatch fills the whole
  // preview (alpha shows over a checkerboard); the value sits on the colour in mono, black or
  // white — whichever contrasts more with that exact colour (auto-contrast, no plate).
  interface Props {
    /** Re-validated CSS colour, or null → the neutral track. */
    css: string | null;
    value: string;
  }
  let { css, value }: Props = $props();

  let probe: CanvasRenderingContext2D | null | undefined;
  /** Normalise any CSS colour through the canvas parser → [r, g, b, a]. */
  function rgba(color: string): [number, number, number, number] | null {
    probe ??= typeof document !== "undefined" ? document.createElement("canvas").getContext("2d") : null;
    if (!probe) return null;
    probe.fillStyle = "#000";
    probe.fillStyle = color;
    const v = String(probe.fillStyle);
    if (v.startsWith("#")) {
      const n = parseInt(v.slice(1, 7), 16);
      return [(n >> 16) & 255, (n >> 8) & 255, n & 255, 1];
    }
    const m = v.match(/rgba?\(([^)]+)\)/);
    if (!m) return null;
    const [r, g, b, a = "1"] = m[1].split(",").map((x) => x.trim());
    return [Number(r), Number(g), Number(b), Number(a)];
  }
  const lin = (c: number) => {
    const s = c / 255;
    return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  /** Black text when the colour (composited over the white checker for alpha) is light. */
  const darkText = $derived.by(() => {
    const c = css ? rgba(css) : null;
    if (!c) return false;
    const [r, g, b, a] = c;
    const mix = (v: number) => v * a + 255 * (1 - a);
    const L = 0.2126 * lin(mix(r)) + 0.7152 * lin(mix(g)) + 0.0722 * lin(mix(b));
    // WCAG contrast against black vs against white: pick the larger.
    return (L + 0.05) / 0.05 > 1.05 / (L + 0.05);
  });
</script>

<!-- A translucent colour (`#8B5CF6CC`, `rgba(…, 0.8)`) shows its alpha over the checkerboard. -->
<div class="checker">
  <div class="swatch" style:background={css ?? "var(--color-track)"}>
    <span class="value" class:value--dark={darkText || !css}>{value}</span>
  </div>
</div>

<style>
  /* Pinned light checker in both themes: alpha is judged against white, like in any editor. */
  .checker {
    width: 100%;
    height: 100%;
    background-color: #ffffff;
    background-image: conic-gradient(#e4e4e3 25%, transparent 0 50%, #e4e4e3 0 75%, transparent 0);
    background-size: 14px 14px;
  }
  .swatch {
    display: flex;
    align-items: flex-end;
    width: 100%;
    height: 100%;
    padding: 10px 12px;
  }
  /* Over user colour: pinned black / white in both themes (functional, not decoration). */
  .value {
    max-width: 100%;
    overflow: hidden;
    color: #ffffff;
    font-family: var(--font-mono);
    font-size: var(--text-row);
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .value--dark {
    color: #000000;
  }
</style>
