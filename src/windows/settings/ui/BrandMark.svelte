<script lang="ts">
  // The Liqlip mark (W2-D `public/brand/`): the dark glyph on light surfaces, the light glyph
  // in the dark theme — both are in the DOM and CSS shows the one matching `data-theme`, so the
  // swap is instant and needs no state. Decorative next to the product name.
  interface Props {
    /** Rendered size in px (the artwork is drawn on a 24 px grid; ≥ 24). */
    size?: number;
  }
  let { size = 32 }: Props = $props();
</script>

<span class="mark" style:--s="{size}px" aria-hidden="true">
  <img class="mark__img mark__img--light" src="brand/logo-light.svg" alt="" width={size} height={size} draggable="false" />
  <img class="mark__img mark__img--dark" src="brand/logo-dark.svg" alt="" width={size} height={size} draggable="false" />
</span>

<style>
  .mark {
    display: inline-flex;
    flex: 0 0 auto;
    width: var(--s);
    height: var(--s);
  }
  .mark__img {
    display: block;
    width: 100%;
    height: 100%;
  }
  .mark__img--dark {
    display: none;
  }
  :global([data-theme="dark"]) .mark__img--light {
    display: none;
  }
  :global([data-theme="dark"]) .mark__img--dark {
    display: block;
  }
</style>
