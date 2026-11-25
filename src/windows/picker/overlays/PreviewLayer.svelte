<script lang="ts">
  // W1 stand-in for Quick Look (Space / Ctrl+Y): the whole record over the grid — full text
  // (mono for code) with search matches, the image fitted, the colour, the file list — with
  // ←/→ to the neighbouring record and Enter to paste. W2-C registers the real Quick Look
  // (info table, zoom, editing) over this kind: `overlays.register("preview", QuickLook)`.
  import Ico from "$lib/components/Ico.svelte";
  import Kbd from "$lib/components/Kbd.svelte";
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
  import { baseName } from "../preview";

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

  const title = $derived(snippet?.name ?? detail?.title ?? (detail ? t(`common.kind.${detail.kind}` as TranslationKey) : ""));
</script>

<div class="scrim" role="presentation" onpointerdown={close}></div>
<div class="sheet squircle" role="dialog" aria-modal="true" aria-label={t("picker.preview.label")}>
  <header class="sheet__head">
    <span class="sheet__title">{title}</span>
    <button type="button" class="sheet__close" aria-label={t("common.close")} title={t("common.close")} onclick={close}>
      <Ico name="x" size={15} />
    </button>
  </header>
  <div class="sheet__body">
    {#if snippet}
      <pre class="text selectable"><Hl text={snippet.body} terms={shelf.terms} /></pre>
    {:else if error}
      <p class="sheet__error"><Ico name="warning" size={16} />{error}</p>
    {:else if !detail}
      <div class="sheet__loading" aria-busy="true">
        <Skeleton height="12px" width="70%" />
        <Skeleton height="12px" width="90%" />
        <Skeleton height="12px" width="55%" />
      </div>
    {:else if detail.kind === "image"}
      <img class="image" src={detail.imageUrl ?? detail.thumbUrl ?? ""} alt={title} draggable="false" />
    {:else if detail.kind === "color"}
      <div class="checker"><div class="swatch" style:background={colorValue(detail) ?? "var(--color-track)"}></div></div>
      <div class="values selectable">
        {#each [detail.meta.color?.hex, detail.meta.color?.rgb, detail.meta.color?.hsl].filter(Boolean) as v (v)}
          <code>{v}</code>
        {/each}
      </div>
    {:else if detail.kind === "file"}
      <ul class="files selectable">
        {#each detail.files ?? [] as f (f.path)}
          <li class:missing={!f.exists}>
            <Ico name={f.exists ? "file" : "warning"} size={14} />
            <span class="files__name">{baseName(f.path)}</span>
            <span class="files__path">{f.path}</span>
          </li>
        {/each}
      </ul>
    {:else}
      <pre class="text selectable" class:text--mono={detail.kind === "code"}><Hl text={detail.text ?? detail.preview} terms={shelf.terms} /></pre>
    {/if}
  </div>
  <footer class="sheet__foot">
    <span><Kbd>↵</Kbd>{t("picker.preview.paste")}</span>
    <span><Kbd>← →</Kbd>{t("picker.preview.browse")}</span>
    <span><Kbd>Esc</Kbd>{t("common.close")}</span>
  </footer>
</div>

<style>
  .scrim {
    position: absolute;
    inset: 0;
    z-index: 40;
    background: color-mix(in srgb, var(--color-paper) 55%, transparent);
    animation: fade-in var(--dur-base) var(--ease-out-soft);
  }
  @keyframes fade-in {
    from {
      opacity: 0;
    }
  }
  .sheet {
    position: absolute;
    inset: 54px 18px 46px;
    z-index: 41;
    display: flex;
    flex-direction: column;
    border-radius: 22px;
    background: var(--color-paper);
    box-shadow: var(--shadow-card-lg);
    overflow: hidden;
    animation: sheet-in var(--dur-slow) var(--ease-liquid);
  }
  @keyframes sheet-in {
    from {
      opacity: 0;
      transform: scale(0.96);
    }
  }
  .sheet__head {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 44px;
    padding: 0 8px 0 16px;
  }
  .sheet__title {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    color: var(--color-ink);
    font-size: var(--text-row);
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sheet__close {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: var(--color-ink-3);
    cursor: pointer;
    transition: background-color var(--dur-fast) var(--ease-out-soft);
  }
  .sheet__close:hover {
    background: var(--color-hover);
    color: var(--color-ink);
  }
  .sheet__close:focus-visible {
    outline: 2px solid var(--color-ink);
    outline-offset: 2px;
  }
  .sheet__body {
    display: flex;
    flex: 1 1 auto;
    flex-direction: column;
    min-height: 0;
    padding: 0 16px 12px;
    overflow: auto;
  }
  .text {
    margin: 0;
    color: var(--color-ink);
    font-family: var(--font-sans);
    font-size: var(--text-row);
    line-height: 1.55;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .text--mono {
    font-family: var(--font-mono);
    font-size: var(--text-secondary);
  }
  .image {
    display: block;
    width: 100%;
    height: 100%;
    min-height: 0;
    object-fit: contain;
  }
  .checker {
    display: flex;
    flex: 1 1 auto;
    min-height: 120px;
    border-radius: 16px;
    overflow: hidden;
    background-color: var(--color-paper);
    background-image: conic-gradient(var(--color-checker) 25%, transparent 0 50%, var(--color-checker) 0 75%, transparent 0);
    background-size: 16px 16px;
  }
  .swatch {
    flex: 1 1 auto;
  }
  .values {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding-top: 10px;
  }
  .values code {
    padding: 4px 8px;
    border-radius: 8px;
    background: var(--color-track);
    color: var(--color-ink);
    font-family: var(--font-mono);
    font-size: var(--text-secondary);
  }
  .files {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .files li {
    display: grid;
    grid-template-columns: 18px 1fr;
    column-gap: 8px;
    align-items: center;
    color: var(--color-ink);
    font-size: var(--text-secondary);
  }
  .files li.missing {
    color: var(--color-ink-3);
  }
  .files__name {
    font-weight: 600;
  }
  .files__path {
    grid-column: 2;
    overflow: hidden;
    color: var(--color-ink-3);
    font-size: var(--text-meta);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sheet__loading {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding-top: 6px;
  }
  .sheet__error {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    color: var(--color-ink-2);
    font-size: var(--text-secondary);
  }
  .sheet__foot {
    display: flex;
    align-items: center;
    gap: 14px;
    height: 36px;
    padding: 0 16px;
    color: var(--color-ink-3);
    font-size: var(--text-meta);
  }
  .sheet__foot span {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .sheet__foot :global(.kbd) {
    height: 20px;
    min-width: 20px;
    padding: 0 5px;
    border-radius: 6px;
  }
</style>
