<script lang="ts">
  // Colour picker of the properties bar (SPEC §9.3 "Свойства"): 8 presets (Shift+1…8), the
  // system palette, a HEX field and the 3 most recent custom colours. One palette (LT had two).
  // The swatches are user content — the only colour in the editor's chrome. A raised popover
  // (r24 / pad 8 → r16 inside); Esc closes it without reaching the editor's Esc ladder.
  import Ico from "$lib/components/Ico.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { PRESETS, PRESET_KEYS } from "./defaults";
  import { contrastInk } from "./textLayout";

  interface Props {
    value: string;
    recent: readonly string[];
    onpick: (color: string) => void;
    onclose: () => void;
  }
  let { value, recent, onpick, onclose }: Props = $props();

  let hex = $state("");
  let hexBad = $state(false);
  let root: HTMLDivElement;
  $effect(() => {
    hex = value.replace(/^#/, "").toUpperCase();
    hexBad = false;
  });

  const HEX = /^#?([0-9a-f]{3}|[0-9a-f]{6})$/i;
  function parseHex(v: string): string | null {
    const m = HEX.exec(v.trim());
    if (!m) return null;
    const h = m[1].length === 3 ? [...m[1]].map((c) => c + c).join("") : m[1];
    return `#${h.toLowerCase()}`;
  }
  function commitHex(): void {
    const c = parseHex(hex);
    hexBad = c === null;
    if (c && c !== value) onpick(c);
  }

  function onKeydown(e: KeyboardEvent): void {
    // nothing typed here reaches the editor's shortcuts
    e.stopPropagation();
    if (e.code === "Escape") {
      e.preventDefault();
      onclose();
    } else if ((e.code === "Enter" || e.code === "NumpadEnter") && (e.target as HTMLElement).tagName === "INPUT") {
      e.preventDefault();
      commitHex();
    }
  }

  function onWindowDown(e: PointerEvent): void {
    if (root && !root.contains(e.target as Node) && !(e.target as HTMLElement).closest?.("[data-color-trigger]")) onclose();
  }

  const name = (c: string) => {
    const i = (PRESETS as readonly string[]).indexOf(c.toLowerCase());
    return i >= 0 ? t(`editor.color.${PRESET_KEYS[i]}`) : c.toUpperCase();
  };
  const keep = (e: MouseEvent) => {
    if ((e.target as HTMLElement).tagName !== "INPUT") e.preventDefault();
  };
</script>

<svelte:window onpointerdown={onWindowDown} />

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="pop squircle" role="dialog" aria-label={t("editor.prop.color")} tabindex="-1" bind:this={root} onkeydown={onKeydown} onmousedown={keep}>
  <div class="pop__grid" role="group" aria-label={t("editor.color.presets")}>
    {#each PRESETS as c, i (c)}
      <button
        type="button"
        class="sw"
        class:is-on={value.toLowerCase() === c}
        style:--sw={c}
        aria-pressed={value.toLowerCase() === c}
        aria-label={`${name(c)} (Shift+${i + 1})`}
        title={`${name(c)} · Shift+${i + 1}`}
        onclick={() => onpick(c)}
      >
        {#if value.toLowerCase() === c}<span class="sw__check" style:color={contrastInk(c)}><Ico name="check" size={14} /></span>{/if}
      </button>
    {/each}
  </div>
  <div class="pop__row">
    <span class="pop__cap">{t("editor.color.recent")}</span>
    <div class="pop__recent">
      {#each recent as c (c)}
        <button type="button" class="sw sw--sm" style:--sw={c} aria-pressed={value.toLowerCase() === c} aria-label={c.toUpperCase()} title={c.toUpperCase()} onclick={() => onpick(c)}>
          {#if value.toLowerCase() === c}<span class="sw__check" style:color={contrastInk(c)}><Ico name="check" size={12} /></span>{/if}
        </button>
      {:else}
        <span class="pop__none">{t("editor.color.noRecent")}</span>
      {/each}
    </div>
  </div>
  <div class="pop__row">
    <label class="custom" title={t("editor.color.custom")}>
      <input type="color" value={value} aria-label={t("editor.color.custom")} oninput={(e) => onpick((e.currentTarget as HTMLInputElement).value.toLowerCase())} />
      <span class="custom__icon" aria-hidden="true"><Ico name="plus" size={16} /></span>
    </label>
    <span class="hex" class:hex--bad={hexBad}>
      <span class="hex__hash" aria-hidden="true">#</span>
      <input
        class="hex__field"
        type="text"
        spellcheck="false"
        maxlength="7"
        aria-label={t("editor.color.hex")}
        aria-invalid={hexBad}
        bind:value={hex}
        onblur={commitHex}
      />
    </span>
  </div>
</div>

<style>
  .pop {
    position: absolute;
    top: calc(100% + 8px);
    left: 0;
    z-index: 40;
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: 216px;
    padding: var(--pad-popover);
    border-radius: var(--r-popover);
    background: var(--color-raised);
    box-shadow: var(--shadow-pop);
    animation: pop-in var(--dur-base) var(--ease-liquid);
  }
  @keyframes pop-in {
    from {
      opacity: 0;
      transform: translateY(-4px) scale(0.98);
    }
  }
  .pop__grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 4px;
    padding: 4px;
    border-radius: var(--r-popover-in);
    background: var(--color-track);
  }
  .sw {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    justify-self: center;
    width: 32px;
    height: 32px;
    padding: 0;
    border: 0;
    border-radius: 999px;
    background: var(--sw);
    transition: transform var(--dur-fast) var(--ease-out-soft);
  }
  .sw:hover {
    transform: scale(1.08);
  }
  .sw:active {
    transform: scale(0.94);
  }
  .sw__check {
    display: inline-flex;
  }
  .sw--sm {
    width: 24px;
    height: 24px;
  }
  .pop__row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 32px;
    padding: 0 4px;
  }
  .pop__cap {
    flex: none;
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
  }
  .pop__recent {
    display: flex;
    gap: 4px;
    margin-left: auto;
    padding: 4px;
    border-radius: 999px;
    background: var(--color-track);
  }
  .pop__recent:has(.pop__none) {
    background: transparent;
  }
  .pop__none {
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
    text-align: right;
  }
  .custom {
    position: relative;
    overflow: hidden;
    display: inline-flex;
    flex: none;
    width: 32px;
    height: 32px;
    border-radius: 999px;
    cursor: pointer;
  }
  .custom input {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    opacity: 0;
    cursor: pointer;
  }
  .custom__icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    height: 100%;
    border-radius: 999px;
    background: var(--color-track);
    color: var(--color-ink-2);
    transition: background-color var(--dur-fast) var(--ease-out-soft);
  }
  .custom:hover .custom__icon {
    background: var(--color-hover);
    color: var(--color-ink);
  }
  .custom:has(input:focus-visible) {
    outline: var(--focus-ring);
    outline-offset: var(--focus-offset);
  }
  .hex {
    display: inline-flex;
    flex: 1;
    align-items: center;
    min-width: 0;
    height: 32px;
    padding: 0 12px;
    border-radius: 999px;
    background: var(--color-track);
    font-family: var(--font-mono);
    font-size: var(--text-secondary);
  }
  .hex:has(input:focus) {
    outline: var(--focus-ring);
    outline-offset: var(--focus-offset);
  }
  .hex--bad {
    color: var(--color-danger);
  }
  .hex__hash {
    color: var(--color-ink-3);
  }
  .hex__field {
    flex: 1;
    min-width: 0;
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    text-transform: uppercase;
    outline: none;
  }
</style>
