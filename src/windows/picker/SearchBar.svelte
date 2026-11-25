<script lang="ts">
  // The search row — the "invisible" search of Liquid Tools (an icon + a bare 14 px input,
  // no box) on a 32 px row whose right end holds the settings circle, concentric in the
  // panel's top-right corner (r = 28 − 12). Filter tokens are 24 px capsule chips (Backspace
  // on an empty field removes the last one); autocomplete after `key:` (↑↓ / Enter / Tab,
  // Esc closes) opens a raised list built like a layer menu. The input is the combobox that
  // owns keyboard focus; the current card is its `aria-activedescendant`.
  import Ico from "$lib/components/Ico.svelte";
  import { settingsOpen } from "$lib/ipc/commands";
  import { errorMessage } from "$lib/ipc/errors";
  import { t, type TranslationKey } from "$lib/i18n/index.svelte";
  import type { IconName } from "$lib/icons";
  import type { Kind } from "$lib/ipc/types";
  import { shelf } from "./shelf.svelte";
  import { selection } from "./selection.svelte";
  import { feed } from "./feed.svelte";
  import { toasts } from "./toasts.svelte";
  import { chipToken, completeToken, suggest, wordAt, type Suggestion } from "./query";

  const KIND_ICON: Record<Kind, IconName> = {
    text: "text",
    link: "link",
    code: "code",
    color: "palette",
    contact: "at",
    image: "image",
    file: "file",
  };

  let input = $state<HTMLInputElement | null>(null);
  let caret = $state(0);
  let focused = $state(false);
  let dismissedAt = $state<string | null>(null);
  let active = $state(0);

  $effect(() => {
    shelf.searchEl = input;
    return () => {
      if (shelf.searchEl === input) shelf.searchEl = null;
    };
  });

  const word = $derived(wordAt(shelf.draft, Math.min(caret, shelf.draft.length)));
  const suggestions: Suggestion[] = $derived(
    focused && word.word.includes(":") && dismissedAt !== word.word ? suggest(word.word, { ...shelf.queryCtx, apps: [...shelf.apps.values()] }) : [],
  );
  const open = $derived(suggestions.length > 0);
  $effect(() => {
    void suggestions;
    active = 0;
  });

  const chips = $derived(shelf.chips.map((raw) => ({ raw, token: chipToken(raw, shelf.queryCtx) })));

  function syncCaret() {
    if (input) caret = input.selectionStart ?? input.value.length;
  }

  function onInput(e: Event) {
    const el = e.currentTarget as HTMLInputElement;
    let value = el.value;
    // Space right after an unfinished token with a unique prefix match completes it.
    const at = el.selectionStart ?? value.length;
    if (at > 0 && /\s/.test(value[at - 1])) {
      const w = wordAt(value, at - 1);
      const done = w.word.includes(":") ? completeToken(w.word, shelf.queryCtx) : null;
      if (done) value = value.slice(0, w.start) + done + value.slice(at - 1);
    }
    const next = shelf.setDraft(value);
    if (next !== el.value) {
      el.value = next;
      el.setSelectionRange(next.length, next.length);
    }
    dismissedAt = null;
    syncCaret();
  }

  function accept(s: Suggestion) {
    const start = word.start;
    const next = shelf.acceptToken(s.insert, start, start + word.word.length);
    const pos = Math.min(start, next.length);
    if (input) {
      input.value = next;
      input.setSelectionRange(pos, pos);
    }
    caret = pos;
    dismissedAt = null;
  }

  function onKeydown(e: KeyboardEvent) {
    if (open) {
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        e.stopPropagation();
        const n = suggestions.length;
        active = (active + (e.key === "ArrowDown" ? 1 : -1) + n) % n;
        return;
      }
      if ((e.key === "Enter" || e.key === "Tab") && !e.ctrlKey && !e.altKey) {
        e.preventDefault();
        e.stopPropagation();
        accept(suggestions[active]);
        return;
      }
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        dismissedAt = word.word;
        return;
      }
    }
    if (e.key === "Backspace" && input && input.selectionStart === 0 && input.selectionEnd === 0 && shelf.chips.length) {
      e.preventDefault();
      shelf.removeChip(shelf.chips.length - 1);
    }
  }

  async function openSettings() {
    try {
      await settingsOpen();
    } catch (e) {
      toasts.error(errorMessage(e));
    }
  }

  function suggestionLabel(s: Suggestion): string {
    if (s.label) return s.label;
    if (s.key === "type") return t(`common.kind.${s.value}` as TranslationKey);
    return t(`picker.suggest.${s.key}.${s.value}` as TranslationKey);
  }

  function suggestionIcon(s: Suggestion): IconName {
    switch (s.key) {
      case "type":
        return KIND_ICON[s.value as Kind] ?? "text";
      case "in":
        return "folder";
      case "is":
        return s.value === "fav" ? "heart" : "document";
      case "date":
        return "clock";
      default:
        return "appWindow";
    }
  }

  function chipLabel(c: (typeof chips)[number]): { key: string; value: string } {
    const tok = c.token;
    if (!tok) return { key: "", value: c.raw };
    const key = t(`picker.suggest.key.${tok.key}` as TranslationKey);
    if (tok.key === "type") return { key, value: tok.value.split(",").map((k) => t(`common.kind.${k}` as TranslationKey)).join(", ") };
    if (tok.key === "is" || tok.key === "date") return { key, value: t(`picker.suggest.${tok.key}.${tok.value}` as TranslationKey) };
    if (tok.key === "app") {
      const app = [...shelf.apps.values()].find((a) => a.exe.toLowerCase().includes(tok.value.toLowerCase()) || a.name.toLowerCase().includes(tok.value.toLowerCase()));
      return { key, value: app?.name ?? tok.value };
    }
    return { key, value: tok.value };
  }

  const activeDescendant = $derived(
    open ? `sug-${active}` : selection.current !== null && feed.status === "ready" ? `card-${selection.current}` : undefined,
  );
</script>

<div class="searchrow">
  <Ico name="search" size={18} class="searchrow__icon" />
  <div class="field" role="presentation">
    {#each chips as chip, i (chip.raw + i)}
      {@const l = chipLabel(chip)}
      <span class="chip" title={chip.raw}>
        <span class="chip__key">{l.key}</span>
        <span class="chip__value">{l.value}</span>
        <button
          type="button"
          class="chip__x"
          aria-label={t("picker.search.removeChip", { token: chip.raw })}
          onmousedown={(e) => e.preventDefault()}
          onclick={() => {
            shelf.removeChip(i);
            shelf.focusSearch(false);
          }}
        >
          <Ico name="x" size={11} />
        </button>
      </span>
    {/each}
    <input
      bind:this={input}
      class="searchrow__input selectable"
      type="text"
      role="combobox"
      aria-label={t("picker.search.label")}
      aria-autocomplete="list"
      aria-expanded={open}
      aria-controls={open ? "picker-suggest" : feed.status === "ready" && feed.count > 0 ? "picker-grid" : undefined}
      aria-activedescendant={activeDescendant}
      autocomplete="off"
      spellcheck="false"
      placeholder={shelf.chips.length ? "" : t("picker.search")}
      value={shelf.draft}
      oninput={onInput}
      onkeydown={onKeydown}
      onkeyup={syncCaret}
      onclick={syncCaret}
      onselect={syncCaret}
      onfocus={() => {
        focused = true;
        syncCaret();
      }}
      onblur={() => (focused = false)}
      onpointerdown={() => void shelf.makeInteractive()}
    />
  </div>

  <div class="hdr">
    {#if !shelf.searchEmpty}
      <button
        type="button"
        class="hdrbtn hdrbtn--ghost"
        title={t("picker.search.clear")}
        aria-label={t("picker.search.clear")}
        onmousedown={(e) => e.preventDefault()}
        onclick={() => {
          shelf.clearSearch();
          shelf.focusSearch(false);
        }}
      >
        <Ico name="x" size={14} />
      </button>
    {/if}
    <!-- Capsules use a plain 999px radius — never the squircle utility. -->
    <button type="button" class="hdrbtn" title={t("picker.settings")} aria-label={t("picker.settings")} onclick={() => void openSettings()}>
      <Ico name="sliders" size={16} />
    </button>
  </div>

  {#if open}
    <ul class="suggest" id="picker-suggest" role="listbox" aria-label={t("picker.suggest.label")}>
      {#each suggestions as s, i (s.insert)}
        <li
          id={`sug-${i}`}
          role="option"
          aria-selected={i === active}
          class="suggest__item"
          class:suggest__item--active={i === active}
          onmousedown={(e) => e.preventDefault()}
          onclick={() => accept(s)}
          onpointermove={() => (active = i)}
          onkeydown={() => {}}
        >
          <span class="suggest__icon">
            {#if s.key === "app" && s.icon}
              <img src={s.icon} alt="" />
            {:else}
              <Ico name={suggestionIcon(s)} size={16} />
            {/if}
          </span>
          <span class="suggest__label">{suggestionLabel(s)}</span>
          <span class="suggest__token">{s.insert}</span>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .searchrow {
    position: relative;
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    gap: 10px;
    height: var(--h-md);
    padding-left: 4px;
  }
  .searchrow :global(.searchrow__icon) {
    flex: 0 0 auto;
    color: var(--color-ink-3);
  }
  /* Chips + input scroll sideways if they overflow; the vertical padding keeps the chips'
     focus rings inside the scroll box (no clipping). */
  .field {
    display: flex;
    flex: 1 1 auto;
    align-items: center;
    gap: 6px;
    min-width: 0;
    margin-block: -4px;
    padding-block: 4px;
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: none;
  }
  .searchrow__input {
    flex: 1 1 auto;
    min-width: 80px;
    height: var(--h-md);
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--color-ink);
    font: inherit;
    font-size: var(--text-panel);
    outline: none;
  }
  .searchrow__input::placeholder {
    color: var(--color-ink-3);
  }

  /* Token chip: 24 px capsule; its remove circle (20 px) is inset 2 px — concentric. */
  .chip {
    display: inline-flex;
    flex: 0 0 auto;
    align-items: center;
    gap: 4px;
    min-width: 0;
    max-width: 240px;
    height: var(--h-sm);
    padding: 0 2px 0 10px;
    border-radius: 999px;
    background: var(--color-track);
    font-size: var(--text-secondary);
    white-space: nowrap;
    animation: pop-select 170ms var(--ease-out-soft);
  }
  .chip__key {
    color: var(--color-ink-3);
    font-weight: 500;
  }
  .chip__value {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--color-ink);
    font-weight: 600;
  }
  .chip__x {
    display: inline-flex;
    flex: 0 0 auto;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    padding: 0;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: var(--color-ink-3);
    transition:
      background-color var(--dur-fast) var(--ease-out-soft),
      color var(--dur-fast) var(--ease-out-soft);
  }
  .chip__x:hover {
    background: var(--color-hover);
    color: var(--color-ink);
  }
  .chip__x:active {
    background: var(--color-selected);
  }
  .chip__x:focus-visible {
    outline-offset: 0;
  }

  .hdr {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    gap: 4px;
  }
  /* 32 px circles; the settings one sits in the panel's corner (12 + 16 = 28). */
  .hdrbtn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--h-md);
    height: var(--h-md);
    padding: 0;
    border: 0;
    border-radius: 999px;
    background: var(--color-track);
    color: var(--color-ink-2);
    transition:
      background-color var(--dur-fast) var(--ease-out-soft),
      color var(--dur-fast) var(--ease-out-soft),
      transform var(--dur-fast) var(--ease-out-soft);
  }
  .hdrbtn--ghost {
    background: transparent;
    color: var(--color-ink-3);
  }
  .hdrbtn:hover {
    background: var(--color-hover);
    color: var(--color-ink);
  }
  .hdrbtn--ghost:hover {
    background: var(--color-track);
  }
  .hdrbtn:active {
    transform: scale(0.94);
  }

  /* Autocomplete: a raised list with the layer-menu geometry (r22 · pad 6 · 32 px capsule
     rows), aligned under the input. */
  .suggest {
    position: absolute;
    top: calc(100% + 6px);
    left: 0;
    z-index: 20;
    display: flex;
    flex-direction: column;
    width: min(360px, calc(100% - 40px));
    margin: 0;
    padding: var(--pad-layer);
    border-radius: var(--r-layer);
    corner-shape: squircle;
    background: var(--color-raised);
    box-shadow: var(--shadow-pop);
    list-style: none;
    animation: suggest-in var(--dur-base) var(--ease-out-soft);
  }
  @keyframes suggest-in {
    from {
      opacity: 0;
      transform: translateY(-4px) scale(0.98);
    }
  }
  .suggest__item {
    display: flex;
    flex: none;
    align-items: center;
    gap: 10px;
    height: var(--h-md);
    padding: 0 12px 0 10px;
    border-radius: var(--r-layer-in);
    color: var(--color-ink);
    font-size: var(--text-row);
    font-weight: 500;
    cursor: pointer;
  }
  .suggest__item--active {
    background: var(--color-track);
  }
  .suggest__icon {
    display: inline-flex;
    flex: 0 0 18px;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    color: var(--color-ink-2);
  }
  .suggest__icon img {
    width: 16px;
    height: 16px;
    object-fit: contain;
  }
  .suggest__label {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .suggest__token {
    flex: 0 0 auto;
    max-width: 45%;
    overflow: hidden;
    color: var(--color-ink-3);
    font-family: var(--font-mono);
    font-size: var(--text-meta);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
