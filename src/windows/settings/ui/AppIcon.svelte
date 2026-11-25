<script lang="ts">
  // App icon (from capture's icon cache) or a monochrome initial tile when there is none
  // or the image fails to load.
  interface Props {
    name: string;
    url: string | null;
    size?: number;
  }
  let { name, url, size = 28 }: Props = $props();
  let broken = $state(false);
  $effect(() => {
    void url;
    broken = false;
  });
  const initial = $derived((name.trim()[0] ?? "?").toLocaleUpperCase());
</script>

{#if url && !broken}
  <img class="icon squircle" src={url} alt="" width={size} height={size} style:--s="{size}px" onerror={() => (broken = true)} />
{:else}
  <span class="icon icon--letter squircle" style:--s="{size}px" aria-hidden="true">{initial}</span>
{/if}

<style>
  .icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: 0 0 auto;
    width: var(--s);
    height: var(--s);
    border-radius: calc(var(--s) * 0.28);
    object-fit: contain;
  }
  .icon--letter {
    background: var(--color-track);
    color: var(--color-ink-2);
    font-size: calc(var(--s) * 0.46);
    font-weight: 700;
    line-height: 1;
  }
</style>
