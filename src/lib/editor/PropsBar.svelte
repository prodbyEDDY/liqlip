<script lang="ts">
  // The properties bar (SPEC §9.3 "Свойства"): only what the current tool / selection has —
  // colour (presets + palette + recent, HEX), size (5 steps), outline/fill, rounded corners and
  // the style of arrows, text, "hide" and spotlight. A change goes to the selection, else to the
  // just-created object, and always becomes the tool's default (rule 4) — the caption says
  // which. One capsule of 32 px controls, like the toolbar; the host places it.
  import { t } from "$lib/i18n/index.svelte";
  import ColorPopover from "./ColorPopover.svelte";
  import Glyph from "./glyphs/Glyph.svelte";
  import Tip from "./Tip.svelte";
  import { ARROW_STYLES, REDACT_MODES, SPOT_SHAPES, TEXT_STYLES } from "./ui";
  import { SIZES } from "./doc";
  import type { EditorState } from "./state.svelte";
  import type { ToolStyle } from "./defaults";
  import type { StyleProp } from "./tools/transform";

  interface Props {
    state: EditorState;
    tips?: "below" | "above";
  }
  let { state: st, tips = "below" }: Props = $props();

  const style = $derived(st.style);
  const allowed = $derived(new Set<StyleProp>(st.props));
  const target = $derived(st.target);
  const selCount = $derived(st.ed.sel.length);
  const keep = (e: MouseEvent) => e.preventDefault();
  const set = <P extends StyleProp>(p: P, v: ToolStyle[P]) => st.setProp(p, v);
</script>

<div class="pb" role="toolbar" aria-label={t("editor.props")}>
  {#if st.tool === "crop"}
    <span class="pb__hint" title={t("editor.crop.hint")}>{t("editor.crop.hint")}</span>
  {:else if !style}
    <span class="pb__hint" title={t("editor.target.none")}>{t("editor.target.none")}</span>
  {:else}
    {#if allowed.has("color")}
      <span class="pb__anchor">
        <button
          type="button"
          class="pb__btn pb__color"
          data-color-trigger
          aria-haspopup="dialog"
          aria-expanded={st.colorOpen}
          aria-label={t("editor.prop.color")}
          onmousedown={keep}
          onclick={() => (st.colorOpen = !st.colorOpen)}
        >
          <span class="pb__swatch" style:--sw={style.color}></span>
          <Tip label={t("editor.prop.color")} keys={["Shift", "1…8"]} side={tips} />
        </button>
        {#if st.colorOpen}
          <ColorPopover value={style.color} recent={st.recent} onpick={(c) => set("color", c)} onclose={() => (st.colorOpen = false)} />
        {/if}
      </span>
    {/if}
    {#if allowed.has("size")}
      <span class="pb__seg" role="group" aria-label={t("editor.prop.size")}>
        {#each SIZES as n (n)}
          <button type="button" class="pb__btn" class:is-on={style.size === n} aria-pressed={style.size === n} aria-label={t("editor.prop.sizeN", { n })} onmousedown={keep} onclick={() => set("size", n)}>
            <span class="pb__dot" style:--d={`${3 + n * 2.4}px`}></span>
            <Tip label={t("editor.prop.sizeN", { n })} keys={[String(n)]} side={tips} />
          </button>
        {/each}
      </span>
    {/if}
    {#if allowed.has("fill")}
      <span class="pb__seg" role="group" aria-label={t("editor.prop.fill")}>
        <button type="button" class="pb__btn" class:is-on={!style.fill} aria-pressed={!style.fill} aria-label={t("editor.prop.outline")} onmousedown={keep} onclick={() => set("fill", false)}>
          <Glyph name="outline" />
          <Tip label={t("editor.prop.outline")} keys={["F"]} side={tips} />
        </button>
        <button type="button" class="pb__btn" class:is-on={style.fill} aria-pressed={style.fill} aria-label={t("editor.prop.fill")} onmousedown={keep} onclick={() => set("fill", true)}>
          <Glyph name="fill" />
          <Tip label={t("editor.prop.fill")} keys={["F"]} side={tips} />
        </button>
      </span>
    {/if}
    {#if allowed.has("round")}
      <button type="button" class="pb__btn" class:is-on={style.round} aria-pressed={style.round} aria-label={t("editor.prop.round")} onmousedown={keep} onclick={() => set("round", !style.round)}>
        <Glyph name={style.round ? "cornerRound" : "cornerSquare"} />
        <Tip label={t("editor.prop.round")} side={tips} />
      </button>
    {/if}
    {#if allowed.has("arrowStyle")}
      <span class="pb__seg" role="group" aria-label={t("editor.prop.arrowStyle")}>
        {#each ARROW_STYLES as a (a.v)}
          <button type="button" class="pb__btn" class:is-on={style.arrowStyle === a.v} aria-pressed={style.arrowStyle === a.v} aria-label={t(a.label)} onmousedown={keep} onclick={() => set("arrowStyle", a.v)}>
            <Glyph name={a.glyph} />
            <Tip label={t(a.label)} keys={["Shift", "A"]} side={tips} />
          </button>
        {/each}
      </span>
    {/if}
    {#if allowed.has("textStyle")}
      <span class="pb__seg" role="group" aria-label={t("editor.prop.textStyle")}>
        {#each TEXT_STYLES as a (a.v)}
          <button type="button" class="pb__btn" class:is-on={style.textStyle === a.v} aria-pressed={style.textStyle === a.v} aria-label={t(a.label)} onmousedown={keep} onclick={() => set("textStyle", a.v)}>
            <Glyph name={a.glyph} />
            <Tip label={t(a.label)} keys={["Shift", "T"]} side={tips} />
          </button>
        {/each}
      </span>
    {/if}
    {#if allowed.has("redactMode")}
      <span class="pb__seg" role="group" aria-label={t("editor.prop.redactMode")}>
        {#each REDACT_MODES as a (a.v)}
          <button type="button" class="pb__btn" class:is-on={style.redactMode === a.v} aria-pressed={style.redactMode === a.v} aria-label={t(a.label)} onmousedown={keep} onclick={() => set("redactMode", a.v)}>
            <Glyph name={a.glyph} />
            <Tip label={t(a.label)} keys={["Shift", "B"]} side={tips} />
          </button>
        {/each}
      </span>
      {#if style.redactMode !== "solid"}<span class="pb__hint pb__hint--warn" title={t("editor.redact.hint")}>{t("editor.redact.hint")}</span>{/if}
    {/if}
    {#if allowed.has("spotShape")}
      <span class="pb__seg" role="group" aria-label={t("editor.prop.spotShape")}>
        {#each SPOT_SHAPES as a (a.v)}
          <button type="button" class="pb__btn" class:is-on={style.spotShape === a.v} aria-pressed={style.spotShape === a.v} aria-label={t(a.label)} onmousedown={keep} onclick={() => set("spotShape", a.v)}>
            <Glyph name={a.glyph} />
            <Tip label={t(a.label)} keys={["Shift", "S"]} side={tips} />
          </button>
        {/each}
      </span>
    {/if}
    {#if target === "last"}
      <span class="pb__hint" title={t("editor.target.last")}>{t("editor.target.last")}</span>
    {:else if target === "selection"}
      <span class="pb__hint">{t("editor.target.selection", { n: selCount })}</span>
    {/if}
  {/if}
</div>

<style>
  .pb {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    height: 40px;
    padding: 4px;
    border-radius: 999px;
    background: var(--color-raised);
    box-shadow: var(--shadow-card);
  }
  .pb__anchor {
    position: relative;
    display: inline-flex;
  }
  .pb__seg {
    display: inline-flex;
    gap: 2px;
    padding: 0;
    border-radius: 999px;
  }
  .pb__btn {
    position: relative;
    display: inline-flex;
    flex: none;
    align-items: center;
    justify-content: center;
    width: var(--h-md);
    height: var(--h-md);
    padding: 0;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: var(--color-ink-2);
    transition:
      background-color var(--dur-fast) var(--ease-out-soft),
      color var(--dur-fast) var(--ease-out-soft),
      transform var(--dur-fast) var(--ease-out-soft);
  }
  .pb__btn:hover {
    background: var(--color-track);
    color: var(--color-ink);
  }
  .pb__btn:active {
    background: var(--color-hover);
    transform: scale(0.94);
  }
  .pb__btn.is-on {
    background: var(--color-selected);
    color: var(--color-ink);
  }
  .pb__color {
    background: var(--color-hover);
  }
  .pb__swatch {
    width: 20px;
    height: 20px;
    border-radius: 999px;
    background: var(--sw);
  }
  .pb__dot {
    width: var(--d);
    height: var(--d);
    border-radius: 999px;
    background: currentColor;
  }
  .pb__hint {
    padding: 0 8px 0 4px;
    overflow: hidden;
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pb__hint:first-child {
    padding-left: 12px;
  }
  .pb__hint--warn {
    max-width: 260px;
  }
</style>
