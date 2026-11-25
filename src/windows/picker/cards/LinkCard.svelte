<script lang="ts">
  // A link at a glance: the site's monogram (no favicon fetch — nothing leaves the machine),
  // the domain large, the path quieter under it (two lines max).
  import Hl from "../Hl.svelte";

  interface Props {
    host: string;
    /** Full URL (or what was copied). */
    url: string;
    terms: readonly string[];
  }
  let { host, url, terms }: Props = $props();

  const domain = $derived(host.replace(/^www\./i, ""));
  const path = $derived.by(() => {
    const rest = url
      .trim()
      .replace(/^[a-z][a-z0-9+.-]*:\/\//i, "")
      .replace(/^[^/?#]*/, "")
      .replace(/\/$/, "");
    try {
      return decodeURI(rest);
    } catch {
      return rest;
    }
  });
  const letter = $derived((domain.split(".").at(-2) ?? domain).charAt(0).toUpperCase() || "#");
</script>

<div class="body">
  <span class="mono squircle" aria-hidden="true">{letter}</span>
  <span class="text">
    <span class="domain"><Hl text={domain} {terms} /></span>
    {#if path}<span class="path"><Hl text={path} {terms} /></span>{/if}
  </span>
</div>

<style>
  .body {
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    width: 100%;
    height: 100%;
    padding: 12px;
  }
  .mono {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border-radius: var(--radius-sm);
    background: var(--color-track);
    color: var(--color-ink);
    font-size: 15px;
    font-weight: 700;
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .domain {
    overflow: hidden;
    color: var(--color-ink);
    font-size: var(--text-panel);
    font-weight: 600;
    line-height: 1.3;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .path {
    display: -webkit-box;
    overflow: hidden;
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
    line-height: 1.4;
    overflow-wrap: anywhere;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
  }
</style>
