<script lang="ts">
  // Progress of `import_lt_run` (`import:progress { phase, done, total }`).
  import { getLocale, t, type TranslationKey } from "$lib/i18n/index.svelte";
  import type { ImportProgress } from "$lib/ipc/types";
  import { formatCount } from "../logic";

  let { progress }: { progress: ImportProgress | null } = $props();

  const KNOWN = ["clips", "media", "folders"];
  const label = $derived(
    progress && KNOWN.includes(progress.phase)
      ? t(`settings.import.phase.${progress.phase}` as TranslationKey)
      : t("settings.import.lt.running"),
  );
  const pct = $derived(progress && progress.total > 0 ? Math.min(100, Math.round((progress.done / progress.total) * 100)) : 0);
</script>

<div
  class="prog"
  role="progressbar"
  aria-label={label}
  aria-valuemin={0}
  aria-valuemax={progress?.total ?? 100}
  aria-valuenow={progress?.done}
>
  <div class="prog__row">
    <span class="prog__label">{label}</span>
    {#if progress && progress.total > 0}
      <span class="prog__num">
        {t("settings.import.progress", { done: formatCount(progress.done, getLocale()), total: formatCount(progress.total, getLocale()) })}
      </span>
    {/if}
  </div>
  <div class="prog__bar" class:prog__bar--indeterminate={!progress || progress.total === 0}>
    <span style:width="{pct}%"></span>
  </div>
</div>

<style>
  .prog {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .prog__row {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    font-size: var(--text-secondary);
  }
  .prog__label {
    font-weight: 600;
    color: var(--color-ink);
  }
  .prog__num {
    font-family: var(--font-mono);
    font-size: var(--text-meta);
    color: var(--color-ink-2);
  }
  .prog__bar {
    height: 6px;
    border-radius: 999px;
    background: var(--color-track);
    overflow: hidden;
  }
  .prog__bar span {
    display: block;
    height: 100%;
    border-radius: 999px;
    background: var(--color-ink);
    transition: width var(--dur-slow) var(--ease-out-soft);
  }
  .prog__bar--indeterminate span {
    width: 30% !important;
    animation: prog-slide 1.2s var(--ease-in-out) infinite;
  }
  @keyframes prog-slide {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(340%);
    }
  }
</style>
