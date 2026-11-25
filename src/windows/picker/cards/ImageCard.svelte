<script lang="ts">
  // The card IS the image: the thumbnail fitted whole, never cropped (Liquid Tools
  // 2026-09-22). A missing / broken thumbnail falls back to a quiet glyph.
  import Ico from "$lib/components/Ico.svelte";
  import { t } from "$lib/i18n/index.svelte";

  interface Props {
    src: string | null;
  }
  let { src }: Props = $props();

  let failed = $state(false);
  $effect(() => {
    void src;
    failed = false;
  });
</script>

{#if src && !failed}
  <!-- `http://liq.localhost/thumb/…` (not a data URL), so lazy loading and async decoding apply. -->
  <img class="shot" {src} alt="" draggable="false" loading="lazy" decoding="async" onerror={() => (failed = true)} />
{:else}
  <div class="fallback"><Ico name="image" size={22} /><span>{t("picker.card.noPreview")}</span></div>
{/if}

<style>
  .shot {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: contain;
  }
  .fallback {
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    font-size: 11px;
    color: var(--color-ink-3);
  }
</style>
