<script lang="ts">
  // W1 stand-in for Quick Look (Space / Ctrl+Y): the whole record over the grid — full text
  // (mono for code) with search matches, the image fitted, the colour, the file list — with
  // ←/→ to the neighbouring record and Enter to paste. W2-C registers the real Quick Look
  // (info table, zoom, editing) over this kind: `overlays.register("preview", QuickLook)`.
  import Ico from "$lib/components/Ico.svelte";
  import { LayerFooter, LayerHeader, LayerSurface } from "./kit";
  import Skeleton from "$lib/components/Skeleton.svelte";
  import { clipDetail } from "$lib/ipc/commands";
  import { errorMessage } from "$lib/ipc/errors";
  import { t, type TranslationKey } from "$lib/i18n/index.svelte";
  import type { ClipDetail, Snippet } from "$lib/ipc/types";
  import Hl from "../Hl.svelte";
  import type { LayerProps } from "../overlays.svelte";
  import { feed } from "../feed.svelte";
  import { selection } from "../selection.svelte";
  import { shelf } from "../shelf.svelte";
  import { pasteSnippet, runAction } from "../actions.svelte";
  import { colorValue } from "../format";
  import { autoTitle, baseName } from "../preview";

  let { layer, close }: LayerProps = $props();

  let id = $state<number | null>(null);
  let detail = $state<ClipDetail | null>(null);
  let error = $state<string | null>(null);
  let seq = 0;

  $effect(() => {
    id = layer.ctx.ids?.[0] ?? null;
  });

  const snippet: Snippet | undefined = $derived(feed.mode === "snippets" && id !== null ? feed.snippet(id) : undefined);

  $effect(() => {
    const target = id;
    if (target === null || feed.mode !== "clips") return;
    const mine = ++seq;
    detail = null;
    error = null;
    clipDetail(target)
      .then((d) => {
        if (mine === seq) detail = d;
      })
      .catch((e) => {
        if (mine === seq) error = errorMessage(e);
      });
  });

  function step(delta: number) {
    const i = feed.indexOf(id) + delta;
    const next = feed.idAt(i);
    if (next === null) return;
    id = next;
    selection.focus(next);
    selection.clearMulti();
  }

  $effect(() => {
    layer.onKey = (e: KeyboardEvent) => {
      if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
        step(-1);
        return true;
      }
      if (e.key === "ArrowRight" || e.key === "ArrowDown") {
        step(1);
        return true;
      }
      if (e.key === "Enter" && id !== null) {
        close();
        if (snippet) void pasteSnippet(snippet, e.shiftKey);
        else void runAction(e.shiftKey ? "pastePlain" : "paste", [id]);
        return true;
      }
      if (e.code === "Space") {
        close();
        return true;
      }
      return false;
    };
  });

  const title = $derived.by(() => {
    if (snippet) return snippet.name;
    if (!detail) return "";
    const app = detail.appId !== null ? shelf.apps.get(detail.appId) : undefined;
    return detail.title ?? autoTitle(detail, app?.name ?? null) ?? t(`common.kind.${detail.kind}` as TranslationKey);
  });
</script>

<LayerSurface variant="sheet" label={t("picker.preview.label")} onclose={close}>
  {#snippet header()}
    <LayerHeader {title} onclose={close} />
  {/snippet}
  <div class="body">
    {#if snippet}
      <pre class="text well selectable"><Hl text={snippet.body} terms={shelf.terms} /></pre>
    {:else if error}
      <p class="error"><Ico name="warning" size={16} />{error}</p>
    {:else if !detail}
      <div class="loading well" aria-busy="true">
        <Skeleton height="12px" width="70%" />
        <Skeleton height="12px" width="90%" />
        <Skeleton height="12px" width="55%" />
      </div>
    {:else if detail.kind === "image"}
      <div class="stage well checker"><img class="image" src={detail.imageUrl ?? detail.thumbUrl ?? ""} alt={title} draggable="false" /></div>
    {:else if detail.kind === "color"}
      <div class="stage well checker"><div class="swatch" style:background={colorValue(detail) ?? "var(--color-track)"}></div></div>
      <div class="values selectable">
        {#each [detail.meta.color?.hex, detail.meta.color?.rgb, detail.meta.color?.hsl].filter(Boolean) as v (v)}
          <code>{v}</code>
        {/each}
      </div>
    {:else if detail.kind === "file"}
      <ul class="files selectable">
        {#each detail.files ?? [] as f (f.path)}
          <li class:missing={!f.exists}>
            <Ico name={f.exists ? "file" : "warning"} size={16} />
            <span class="files__name">{baseName(f.path)}</span>
            <span class="files__path" title={f.path}><bdi>{f.path}</bdi></span>
          </li>
        {/each}
      </ul>
    {:else}
      <pre class="text well selectable" class:text--mono={detail.kind === "code"}><Hl text={detail.text ?? detail.preview} terms={shelf.terms} /></pre>
    {/if}
  </div>
  {#snippet footer()}
    <LayerFooter hints={[["↵", t("picker.preview.paste")], ["← →", t("picker.preview.browse")], ["Esc", t("common.close")]]} />
  {/snippet}
</LayerSurface>

<style>
  .body {
    display: flex;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 8px;
    min-height: 0;
    margin-top: 6px;
  }
  /* Content wells: concentric with the sheet (--layer-r-in). */
  .well {
    flex: 1 1 auto;
    min-height: 0;
    border-radius: var(--layer-r-in);
  }
  .text {
    margin: 0;
    padding: 12px 14px;
    overflow: auto;
    background: var(--color-track);
    color: var(--color-ink);
    font-family: var(--font-sans);
    font-size: var(--text-row);
    line-height: 1.6;
    white-space: pre-wrap;
    overflow-wrap: break-word;
  }
  .text--mono {
    font-family: var(--font-mono);
    font-size: var(--text-secondary);
    white-space: pre;
  }
  .stage {
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
  }
  .checker {
    background-color: var(--color-paper);
    background-image: conic-gradient(var(--color-checker) 25%, transparent 0 50%, var(--color-checker) 0 75%, transparent 0);
    background-size: 16px 16px;
  }
  .image {
    display: block;
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
  }
  .swatch {
    align-self: stretch;
    flex: 1 1 auto;
  }
  .values {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .values code {
    display: inline-flex;
    align-items: center;
    height: var(--h-md);
    padding: 0 12px;
    border-radius: 999px;
    background: var(--color-track);
    color: var(--color-ink);
    font-family: var(--font-mono);
    font-size: var(--text-secondary);
  }
  .files {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    overflow: auto;
    list-style: none;
  }
  .files li {
    display: grid;
    grid-template-columns: 20px minmax(0, 1fr);
    column-gap: 10px;
    align-items: center;
    padding: 8px 10px;
    border-radius: var(--layer-r-in);
    color: var(--color-ink);
    font-size: var(--text-row);
  }
  .files li:hover {
    background: var(--color-track);
  }
  .files li.missing {
    color: var(--color-ink-3);
  }
  .files__name {
    overflow: hidden;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .files__path {
    grid-column: 2;
    overflow: hidden;
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;
    text-align: left;
  }
  .loading {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px;
    background: var(--color-track);
  }
  .error {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    padding: 12px;
    color: var(--color-ink-2);
    font-size: var(--text-secondary);
  }
</style>
