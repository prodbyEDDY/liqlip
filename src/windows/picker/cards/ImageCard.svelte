<script lang="ts">
  // The card IS the image: it fills the preview (cover), over a checkerboard so transparency
  // reads. Extreme shapes (a long strip, a tall phone screenshot) and images smaller than the
  // card are shown whole instead (contain / never upscaled), so nothing important is cropped
  // away or blown up. A missing / broken thumbnail falls back to a quiet glyph.
  import Ico from "$lib/components/Ico.svelte";
  import { t } from "$lib/i18n/index.svelte";

  interface Props {
    src: string | null;
    width?: number | null;
    height?: number | null;
  }
  let { src, width = null, height = null }: Props = $props();

  let failed = $state(false);
  $effect(() => {
    void src;
    failed = false;
  });

  /** Card preview aspect (layout.ts PREVIEW_RATIO 0.75 → 4:3). */
  const BOX = 4 / 3;
  /** Smaller than the card: shown whole, at most 2× its pixel size (icons stay crisp-ish). */
  const small = $derived(!!width && !!height && width < 120 && height < 90);
  const fit = $derived.by(() => {
    if (!width || !height) return "cover";
    if (small) return "contain";
    const r = width / height;
    return r > BOX * 2.2 || r < BOX / 2.6 ? "contain" : "cover";
  });
</script>

{#if src && !failed}
  <!-- `http://liq.localhost/thumb/…` (not a data URL), so lazy loading and async decoding apply. -->
  <div class="stage" class:stage--small={small}>
    <img
      class="shot"
      style:object-fit={fit}
      style:max-width={small ? `${width! * 2}px` : null}
      style:max-height={small ? `${height! * 2}px` : null}
      {src}
      alt=""
      draggable="false"
      loading="lazy"
      decoding="async"
      onerror={() => (failed = true)}
    />
  </div>
{:else}
  <div class="fallback"><Ico name="image" size={22} /><span>{t("picker.card.noPreview")}</span></div>
{/if}

<style>
  .stage {
    width: 100%;
    height: 100%;
    background-color: var(--color-card);
    background-image: conic-gradient(var(--color-checker) 25%, transparent 0 50%, var(--color-checker) 0 75%, transparent 0);
    background-size: 14px 14px;
  }
  .stage--small {
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .shot {
    display: block;
    width: 100%;
    height: 100%;
  }
  .fallback {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    width: 100%;
    height: 100%;
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
  }
</style>
