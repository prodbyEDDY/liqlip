<script lang="ts">
  // W0 stub of the shelf (W1-E replaces it): the monochrome panel sized by
  // `picker:layout` and a plain list of the first records from `clips_list` — proof that
  // the IPC layer (Tauri or the browser mock) works end to end. Not the final design.
  import { onMount } from "svelte";
  import Ico from "$lib/components/Ico.svelte";
  import Skeleton from "$lib/components/Skeleton.svelte";
  import EmptyState from "$lib/components/EmptyState.svelte";
  import Button from "$lib/components/Button.svelte";
  import Badge from "$lib/components/Badge.svelte";
  import { t, type TranslationKey } from "$lib/i18n/index.svelte";
  import { clipsList } from "$lib/ipc/commands";
  import { onClipsChanged, onPickerLayout } from "$lib/ipc/events";
  import { errorMessage } from "$lib/ipc/errors";
  import type { Clip, Kind, PickerLayout } from "$lib/ipc/types";
  import type { IconName } from "$lib/icons";

  // Default = the centre sheet (SPEC §3.1) until Rust sends the real layout.
  let layout = $state<PickerLayout>({ position: "center", centerRow: false, width: 760, height: 640, bleed: 72 });
  let items = $state<Clip[]>([]);
  let status = $state<"loading" | "ready" | "error">("loading");
  let error = $state("");

  const KIND_ICON: Record<Kind, IconName> = {
    text: "text",
    link: "link",
    code: "code",
    color: "palette",
    contact: "at",
    image: "image",
    file: "file",
  };

  async function load() {
    if (items.length === 0) status = "loading";
    try {
      const page = await clipsList({ text: "", exclude: [], filter: {}, cursor: null, limit: 40 });
      items = page.items;
      status = "ready";
    } catch (e) {
      error = errorMessage(e);
      status = "error";
    }
  }

  // onMount, not $effect: `load` reads `items`/`status`, which an effect would track and
  // re-run on every result (a fetch loop).
  onMount(() => {
    void load();
    const subs = [onPickerLayout((l) => (layout = l)), onClipsChanged(() => void load())];
    return () => {
      for (const s of subs) void s.then((off) => off());
    };
  });
</script>

<div class="stage" style="--bleed:{layout.bleed}px">
  <section
    class="panel squircle"
    style="width:{layout.width}px;height:{layout.height}px"
    aria-label="Liqlip"
  >
    <div class="search squircle">
      <Ico name="search" size={16} />
      <span>{t("picker.search")}</span>
    </div>

    {#if status === "loading"}
      <ul class="list" aria-busy="true">
        {#each Array.from({ length: 8 }, (_, i) => i) as i (i)}
          <li class="row"><Skeleton width="16px" height="16px" radius={5} /><Skeleton height="12px" /></li>
        {/each}
      </ul>
    {:else if status === "error"}
      <EmptyState icon="warning" title={t("picker.error.title")} desc={error}>
        <Button variant="secondary" size="sm" icon="refresh" onclick={() => void load()}>{t("common.retry")}</Button>
      </EmptyState>
    {:else if items.length === 0}
      <EmptyState icon="clipboard" title={t("picker.empty.title")} desc={t("picker.empty.desc")} />
    {:else}
      <ul class="list">
        {#each items as clip (clip.id)}
          <li class="row">
            <span class="row__icon"><Ico name={KIND_ICON[clip.kind]} size={16} /></span>
            <span class="row__text" class:row__text--mono={clip.kind === "code"}>
              {clip.title ?? (clip.preview || (clip.width ? `${clip.width} × ${clip.height}` : ""))}
            </span>
            <Badge>{t(`common.kind.${clip.kind}` as TranslationKey)}</Badge>
          </li>
        {/each}
      </ul>
    {/if}
  </section>
</div>

<style>
  :global(html),
  :global(body) {
    background: transparent;
  }
  :global(#app) {
    height: 100%;
  }
  .stage {
    display: grid;
    place-items: center;
    width: 100%;
    height: 100%;
    padding: var(--bleed);
  }
  .panel {
    display: flex;
    flex-direction: column;
    gap: 10px;
    max-width: 100%;
    max-height: 100%;
    padding: 14px 14px 6px;
    border-radius: var(--radius-panel);
    background: var(--color-paper);
    box-shadow: var(--shadow-summoned);
    overflow: hidden;
  }
  .search {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    gap: 8px;
    height: 38px;
    padding: 0 12px;
    border-radius: var(--radius-control);
    background: var(--color-track);
    color: var(--color-ink-3);
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0 0 8px;
    list-style: none;
    overflow-y: auto;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 34px;
    padding: 0 10px;
    border-radius: var(--radius-control-sm);
    color: var(--color-ink);
  }
  .row:hover {
    background: var(--color-hover);
  }
  .row__icon {
    display: inline-flex;
    color: var(--color-ink-3);
  }
  .row__text {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    font-size: var(--text-row);
  }
  .row__text--mono {
    font-family: var(--font-mono);
    font-size: var(--text-secondary);
  }
</style>
