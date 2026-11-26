<script lang="ts">
  // An editor glyph in the Heroicons format (1em box, stroke 1.5, currentColor) — rendered the
  // same way `<Ico>` renders Heroicons, so both sit side by side in the toolbars.
  import { GLYPHS, type GlyphName } from "./glyphs";

  interface Props {
    name: GlyphName;
    size?: number;
  }
  let { name, size = 18 }: Props = $props();
</script>

<span class="ico ico--static" style="font-size:{size}px" aria-hidden="true">
  <svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
    <!-- eslint-disable-next-line svelte/no-at-html-tags -- static, bundled glyph markup -->
    {@html GLYPHS[name]}
  </svg>
</span>
