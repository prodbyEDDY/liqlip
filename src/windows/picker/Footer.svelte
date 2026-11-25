<script lang="ts">
  // Hint row (SPEC §3.1): "↵ Paste into <icon> <app>" for the real target (no target →
  // "↵ Copy"; elevated → "↵ Copy · administrator window"), ⇧↵ plain text, Space preview,
  // Ctrl K actions; on the right the pause indicator (click → resume) and "Stack N" (click →
  // the stack layer; also a drop target for dragged cards). Labels shorten on narrow strips
  // (container queries). Hidden when `settings.picker.hints` is off.
  import Ico from "$lib/components/Ico.svelte";
  import Kbd from "$lib/components/Kbd.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { shelf } from "./shelf.svelte";
  import { selection } from "./selection.svelte";
  import { overlays } from "./overlays.svelte";
  import { drag } from "./drag.svelte";
  import { feed } from "./feed.svelte";

  let iconFailed = $state(false);
  $effect(() => {
    void shelf.target?.iconUrl;
    iconFailed = false;
  });

  const target = $derived(shelf.target);
  const stackCount = $derived(shelf.stack.items.length);
  const selected = $derived(selection.ids.size);
</script>

<footer class="footer">
  <div class="hints">
    <span class="hint hint--main">
      <Kbd>↵</Kbd>
      {#if target && !target.elevated}
        <span class="hint__label">{t("picker.hint.pasteInto")}</span>
        <span class="target" title={target.title}>
          {#if target.iconUrl && !iconFailed}
            <img src={target.iconUrl} alt="" onerror={() => (iconFailed = true)} />
          {:else}
            <Ico name="appWindow" size={13} />
          {/if}
          <span class="target__name">{target.name}</span>
        </span>
      {:else if target?.elevated}
        <span class="hint__label">{t("picker.hint.copy")}</span>
        <span class="hint__sub" title={target.title}><Ico name="shield" size={12} />{t("picker.hint.elevated")}</span>
      {:else}
        <span class="hint__label">{t("picker.hint.copy")}</span>
      {/if}
    </span>
    {#if selected > 1}
      <span class="hint hint--selected">{t("picker.hint.selected", { n: selected })}</span>
    {/if}
    {#if feed.mode === "clips"}
      <span class="hint hint--plain"><Kbd>⇧↵</Kbd><span class="hint__label">{t("picker.hint.plain")}</span></span>
    {/if}
    <span class="hint hint--preview"><Kbd>Space</Kbd><span class="hint__label">{t("picker.hint.preview")}</span></span>
    <span class="hint hint--actions"><Kbd>Ctrl K</Kbd><span class="hint__label">{t("picker.hint.actions")}</span></span>
  </div>

  <div class="status">
    {#if shelf.paused}
      <button type="button" class="status__btn" title={t("picker.hint.resumeTitle")} onclick={() => void shelf.resume()}>
        <Ico name="pause" size={13} />
        <span class="status__label">{t("picker.hint.paused")}</span>
      </button>
    {/if}
    {#if stackCount > 0 || drag.active}
      <button
        type="button"
        class="status__btn"
        class:status__btn--drop={drag.active}
        class:status__btn--over={drag.active && drag.over === "stack"}
        data-drop={drag.active ? "stack" : undefined}
        title={t("picker.hint.stackTitle")}
        aria-haspopup="dialog"
        onclick={() => overlays.open("stack")}
      >
        <Ico name="stack" size={13} />
        <span class="status__label">{t("picker.hint.stack")}</span>
        <span class="status__n">{stackCount}</span>
      </button>
    {/if}
  </div>
</footer>

<style>
  .footer {
    container-type: inline-size;
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    height: 32px;
    padding: 4px 2px 0;
    color: var(--color-ink-3);
    font-size: var(--text-meta);
    white-space: nowrap;
  }
  .hints {
    display: flex;
    flex: 1 1 auto;
    align-items: center;
    gap: 14px;
    min-width: 0;
    overflow: hidden;
  }
  .hint {
    display: inline-flex;
    flex: 0 0 auto;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .hint--main {
    flex: 0 1 auto;
  }
  .hint :global(.kbd) {
    height: 20px;
    min-width: 20px;
    padding: 0 5px;
    border-radius: 6px;
  }
  .hint__label {
    color: var(--color-ink-2);
  }
  .hint__sub {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .hint--selected {
    color: var(--color-ink);
    font-weight: 600;
  }
  .target {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    min-width: 0;
    color: var(--color-ink);
    font-weight: 600;
  }
  .target img {
    flex: 0 0 auto;
    width: 14px;
    height: 14px;
    object-fit: contain;
  }
  .target__name {
    min-width: 0;
    max-width: 180px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .status {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    gap: 6px;
  }
  .status__btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 24px;
    padding: 0 9px;
    border: 0;
    border-radius: 999px;
    background: var(--color-track);
    color: var(--color-ink-2);
    font: inherit;
    font-size: var(--text-meta);
    font-weight: 600;
    cursor: pointer;
    transition:
      background-color var(--dur-fast) var(--ease-out-soft),
      color var(--dur-fast) var(--ease-out-soft),
      transform var(--dur-fast) var(--ease-out-soft);
  }
  .status__btn:hover {
    background: var(--color-hover);
    color: var(--color-ink);
  }
  .status__btn:active {
    transform: scale(0.96);
  }
  .status__btn:focus-visible {
    outline: 2px solid var(--color-ink);
    outline-offset: 2px;
  }
  .status__n {
    font-variant-numeric: tabular-nums;
  }
  .status__btn--drop {
    transform: scale(1.04);
  }
  .status__btn--over {
    background: var(--color-ink);
    color: var(--color-paper);
  }

  /* Narrow panels / strips: drop the labels of the secondary hints, then the hints. */
  @container (max-width: 560px) {
    .hint--plain .hint__label,
    .hint--preview .hint__label,
    .hint--actions .hint__label {
      display: none;
    }
    .hints {
      gap: 10px;
    }
  }
  @container (max-width: 400px) {
    .hint--plain,
    .hint--preview,
    .hint--actions,
    .status__label {
      display: none;
    }
    .target__name {
      max-width: 110px;
    }
  }
</style>
