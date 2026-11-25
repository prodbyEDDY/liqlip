<script lang="ts">
  // Shared Heroicons outline wrapper; selected upstream SVGs are bundled locally.
  import { ICONS, type IconName } from "$lib/icons";

  interface Props {
    name: IconName;
    /** Square side in px (maps to font-size; the svg renders at 1em). */
    size?: number;
    /** Extra class(es) forwarded onto the wrapper span. */
    class?: string;
    /** false for self-animating icons → opts out of the hover-tilt. */
    tilt?: boolean;
  }
  let { name, size = 16, class: cls = "", tilt = false }: Props = $props();

  const Glyph = $derived(ICONS[name]);
</script>

<span class="ico {cls}" class:ico--static={!tilt} style="font-size:{size}px" aria-hidden="true">
  <Glyph />
</span>
