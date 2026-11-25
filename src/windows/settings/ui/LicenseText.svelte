<script lang="ts">
  // A bundled license file shown inside the window (Tauri doesn't open target="_blank"
  // links): fetched from the app's own origin the first time it is opened (CSP
  // `connect-src 'self'`), then kept. Long files scroll in a mono well; loading = skeleton
  // lines, error = message + retry. The owner renders the Show/Hide button (`aria-controls`).
  // The scroller needs no tabindex: Chromium (WebView2 130+) makes scrollers keyboard-focusable.
  import Button from "$lib/components/Button.svelte";
  import Ico from "$lib/components/Ico.svelte";
  import Skeleton from "$lib/components/Skeleton.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { scrollFade } from "$lib/scrollFade";

  interface Props {
    /** Path under `public/` (e.g. "licenses/LICENSE.txt"). */
    src: string;
    /** Accessible name of the text region. */
    label: string;
    id: string;
    open: boolean;
  }
  let { src, label, id, open }: Props = $props();

  let status = $state<"idle" | "loading" | "ready" | "error">("idle");
  let text = $state("");

  async function load() {
    status = "loading";
    try {
      const res = await fetch(src);
      if (!res.ok) throw new Error(String(res.status));
      text = await res.text();
      status = "ready";
    } catch {
      status = "error";
    }
  }

  $effect(() => {
    if (open && status === "idle") void load();
  });
</script>

{#if open}
  <div class="lic" {id}>
    {#if status === "ready"}
      <pre class="lic__text selectable" role="region" aria-label={label} use:scrollFade>{text}</pre>
    {:else if status === "error"}
      <div class="lic__err" role="alert">
        <Ico name="warning" size={14} /><span>{t("settings.about.license.error")}</span>
        <Button variant="ghost" size="sm" icon="refresh" onclick={load}>{t("common.retry")}</Button>
      </div>
    {:else}
      <div class="lic__sk" aria-busy="true">
        <Skeleton width="62%" height="10px" />
        <Skeleton width="88%" height="10px" />
        <Skeleton width="74%" height="10px" />
      </div>
    {/if}
  </div>
{/if}

<style>
  /* Mono well: r12 from the scale (it sits ≥ 16 px from the tile corner); scrolls inside. */
  .lic__text,
  .lic__sk {
    border-radius: var(--radius-md);
    corner-shape: squircle;
    background: var(--color-track);
  }
  .lic__text {
    max-height: 320px;
    margin: 0;
    padding: 12px 14px;
    overflow: auto;
    color: var(--color-ink-2);
    font-family: var(--font-mono);
    font-size: var(--text-meta);
    line-height: 1.55;
    /* Keep the file's own layout (rulers, aligned columns); long lines scroll sideways. */
    white-space: pre;
  }
  .lic__sk {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 14px;
  }
  .lic__err {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--color-ink);
    font-size: var(--text-secondary);
    font-weight: 600;
  }
</style>
