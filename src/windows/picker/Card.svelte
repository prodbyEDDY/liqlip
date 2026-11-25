<script lang="ts">
  // One grid tile: the card surface (CardShell + the kind's preview), its hover/selection
  // actions and the inline rename field (F2). Mouse: click = paste (or copy, per settings and
  // target), double click = Enter, Ctrl/Shift+click = multi-select, right click = actions,
  // press + move = drag (onto Favorites / a folder / the stack, or out of the shelf).
  import { getLocale, t, type TranslationKey } from "$lib/i18n/index.svelte";
  import type { Clip, Snippet } from "$lib/ipc/types";
  import CardShell from "./CardShell.svelte";
  import CardActions from "./CardActions.svelte";
  import TextCard from "./cards/TextCard.svelte";
  import CodeCard from "./cards/CodeCard.svelte";
  import LinkCard from "./cards/LinkCard.svelte";
  import ColorCard from "./cards/ColorCard.svelte";
  import ContactCard from "./cards/ContactCard.svelte";
  import ImageCard from "./cards/ImageCard.svelte";
  import FileCard from "./cards/FileCard.svelte";
  import { shelf } from "./shelf.svelte";
  import { selection } from "./selection.svelte";
  import { overlays } from "./overlays.svelte";
  import { drag } from "./drag.svelte";
  import { clickPastes, copySnippet, pasteSnippet, runAction } from "./actions.svelte";
  import { autoTitle, baseName, entryFormat } from "./preview";
  import { clickIntent } from "./keymap";
  import { colorValue, linkHost, relativeTime } from "./format";

  interface Props {
    clip?: Clip;
    snippet?: Snippet;
    digit?: number | null;
  }
  let { clip, snippet, digit = null }: Props = $props();

  const id = $derived(clip?.id ?? snippet?.id ?? 0);
  const app = $derived(clip?.appId != null ? shelf.apps.get(clip.appId) : undefined);
  const kindLabel = $derived(clip ? t(`common.kind.${clip.kind}` as TranslationKey) : t("picker.pill.snippets"));
  const title = $derived(
    snippet ? snippet.name : (clip!.title ?? autoTitle(clip!, app?.name ?? null) ?? kindLabel),
  );
  // The meta row names the source; a link's host + path is already the preview itself, so its
  // meta shows the app it was copied from (a user-set title still wins).
  const metaTitle = $derived(clip && clip.kind === "link" && !clip.title ? (app?.name ?? kindLabel) : title);
  const badge = $derived(clip ? entryFormat(clip) : "SNIP");
  const tooltip = $derived.by(() => {
    if (!clip) return snippet!.name;
    const parts = [app?.name ?? kindLabel, relativeTime(clip.copiedAt, Date.now(), getLocale())];
    if (clip.copyCount > 1) parts.push(t("picker.card.copiedTimes", { n: clip.copyCount }));
    return parts.join(" · ");
  });
  const label = $derived(`${kindLabel}: ${title}${clip && clip.preview && clip.kind !== "image" ? ` — ${clip.preview.slice(0, 80)}` : ""}`);

  const current = $derived(selection.current === id);
  const checked = $derived(selection.ids.has(id));
  const hovered = $derived(shelf.hoveredId === id);
  const dragging = $derived(drag.active && drag.ids.includes(id));
  const dropSide = $derived(drag.before?.edgeOf === id ? drag.before.side : null);
  const renaming = $derived(shelf.renamingId === id);
  const showActions = $derived((hovered || current) && !drag.active && !renaming && shelf.open);

  function onpointerdown(e: PointerEvent) {
    if (e.ctrlKey || e.shiftKey || e.metaKey || !clip) return;
    drag.press(id, e);
  }

  function onclick(e: MouseEvent) {
    // A passive (hover-opened) shelf becomes keyboard-interactive first, then acts (W1-D).
    if (!shelf.interactive) {
      void shelf.makeInteractive().then(() => handleClick(e));
      return;
    }
    handleClick(e);
  }

  function handleClick(e: MouseEvent) {
    if (clip) {
      drag.click(id, e);
      return;
    }
    selection.focus(id);
    // Same rule as records: a click pastes (or copies, per settings); a double click pastes —
    // once (`clickIntent`).
    const intent = clickIntent(e.detail, clickPastes());
    if (intent === null || (e.detail >= 2 && !shelf.open)) return;
    if (intent === "paste") void pasteSnippet(snippet!, false);
    else void copySnippet(snippet!);
  }

  function openActions(anchor: { x: number; y: number }) {
    if (!(selection.multi && selection.has(id))) {
      selection.clearMulti();
      selection.focus(id);
    }
    const ids = selection.multi ? shelf.targets() : [id];
    overlays.open("actions", { ids, anchor, snippet: snippet ?? null });
  }

  function oncontextmenu(e: MouseEvent) {
    e.preventDefault();
    openActions({ x: e.clientX, y: e.clientY });
  }

  function onmore(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    openActions({ x: r.left, y: r.bottom + 4 });
  }

  function oncopy() {
    if (snippet) void copySnippet(snippet);
    else void runAction("copy", [id]);
  }

  // ── inline rename (F2) ──
  let renameEl = $state<HTMLInputElement | null>(null);
  $effect(() => {
    if (!renameEl) return;
    renameEl.focus();
    renameEl.select();
  });
  function onRenameKey(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      e.stopPropagation();
      void shelf.commitRename(id, (e.currentTarget as HTMLInputElement).value);
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      shelf.renamingId = null;
      shelf.focusSearch(false);
    }
  }
</script>

<CardShell
  {id}
  title={metaTitle}
  terms={shelf.terms}
  {label}
  {tooltip}
  appIcon={app?.iconUrl ?? null}
  fallbackIcon={snippet ? "document" : "appWindow"}
  {badge}
  favorite={clip?.favorite ?? false}
  {current}
  {checked}
  {hovered}
  {dragging}
  {digit}
  {dropSide}
  {renaming}
  {onpointerdown}
  {onclick}
  {oncontextmenu}
  onpointerenter={() => (shelf.hoveredId = id)}
  onpointerleave={() => {
    if (shelf.hoveredId === id) shelf.hoveredId = null;
  }}
>
  {#if snippet}
    <TextCard text={snippet.body} terms={shelf.terms} />
  {:else if clip}
    {#if clip.kind === "image"}
      <ImageCard src={clip.thumbUrl} width={clip.width} height={clip.height} />
    {:else if clip.kind === "link"}
      <LinkCard host={linkHost(clip)} url={clip.meta.url ?? clip.preview} terms={shelf.terms} />
    {:else if clip.kind === "color"}
      <ColorCard css={colorValue(clip)} value={clip.preview} />
    {:else if clip.kind === "contact"}
      <ContactCard value={clip.preview} email={(clip.meta.contact ?? (clip.preview.includes("@") ? "email" : "phone")) === "email"} terms={shelf.terms} />
    {:else if clip.kind === "file"}
      <FileCard name={baseName(clip.preview)} format={badge} more={Math.max(0, (clip.fileCount ?? 1) - 1)} dir={clip.meta.dir} terms={shelf.terms} />
    {:else if clip.kind === "code"}
      <CodeCard text={clip.preview} terms={shelf.terms} />
    {:else}
      <TextCard text={clip.preview} terms={shelf.terms} />
    {/if}
  {/if}
</CardShell>

{#if showActions}
  <CardActions
    favorite={clip?.favorite ?? false}
    canFavorite={!!clip}
    {oncopy}
    onfavorite={clip ? () => void runAction("toggleFavorite", [id]) : undefined}
    {onmore}
  />
{/if}

{#if renaming && clip}
  <input
    bind:this={renameEl}
    class="rename selectable"
    type="text"
    maxlength="200"
    value={clip.title ?? ""}
    placeholder={autoTitle(clip, app?.name ?? null) ?? kindLabel}
    aria-label={t("picker.card.renameLabel")}
    onkeydown={onRenameKey}
    onblur={(e) => {
      if (shelf.renamingId === id) void shelf.commitRename(id, (e.currentTarget as HTMLInputElement).value);
    }}
  />
{/if}

<style>
  /* F2 rename: a 24 px capsule over the meta row, inset 4 px from the card's bottom corners
     (12 + 4 = 16, concentric). Text entry keeps its focus ring while focused. */
  .rename {
    position: absolute;
    left: var(--pad-card);
    right: var(--pad-card);
    bottom: var(--pad-card);
    z-index: 3;
    height: calc(2 * var(--r-card-in));
    padding: 0 10px;
    border: 0;
    border-radius: 999px;
    background: var(--color-raised);
    box-shadow: var(--shadow-pop);
    color: var(--color-ink);
    font: inherit;
    font-size: var(--text-secondary);
    outline: var(--focus-ring);
    outline-offset: 0;
  }
  .rename::placeholder {
    color: var(--color-ink-3);
  }
</style>
