<script lang="ts">
  // The key cheatsheet (`?`, SPEC §9.3): tools with their keys, drawing modifiers, editing,
  // view and actions. A dialog over the editor (r28 / pad 12); Esc, `?` or a click outside
  // closes it (Esc through `editor.escape()`).
  import Ico from "$lib/components/Ico.svelte";
  import Kbd from "$lib/components/Kbd.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { scrollFade } from "$lib/scrollFade";
  import Glyph from "./glyphs/Glyph.svelte";
  import { KEY_GROUPS, TOOL_META } from "./ui";
  import type { EditorState } from "./state.svelte";

  interface Props {
    state: EditorState;
  }
  let { state: st }: Props = $props();

  const win = $derived(st.mode === "window");
  const tools = $derived(TOOL_META.filter((m) => m.id !== "crop" || win));
  const close = () => (st.cheatsheet = false);
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="cs" onpointerdown={(e) => e.stopPropagation()} onclick={(e) => e.target === e.currentTarget && close()}>
  <div class="cs__card squircle" role="dialog" aria-modal="true" aria-labelledby="ed-cs-title">
    <header class="cs__head">
      <h2 id="ed-cs-title" class="cs__title">{t("editor.keys.title")}</h2>
      <button type="button" class="cs__close" aria-label={t("common.close")} title={t("common.close")} onmousedown={(e) => e.preventDefault()} onclick={close}>
        <Ico name="x" size={16} />
      </button>
    </header>
    <div class="cs__cols" use:scrollFade>
      <section class="cs__group">
        <h3 class="cs__cap">{t("editor.keys.group.tools")}</h3>
        <ul class="cs__list">
          {#each tools as m (m.id)}
            <li class="cs__row">
              <span class="cs__icon"><Glyph name={m.glyph} size={16} /></span>
              <span class="cs__label">{t(m.label)}</span>
              <Kbd size="sm">{m.key}</Kbd>
            </li>
          {/each}
        </ul>
      </section>
      {#each KEY_GROUPS as g (g.title)}
        <section class="cs__group">
          <h3 class="cs__cap">{t(g.title)}</h3>
          <ul class="cs__list">
            {#each g.rows.filter((r) => !r.window || win) as r (r.label)}
              <li class="cs__row">
                <span class="cs__label">{t(r.label)}</span>
                <span class="cs__keys">{#each r.keys as k, i (i)}<Kbd size="sm">{k}</Kbd>{/each}</span>
              </li>
            {/each}
          </ul>
        </section>
      {/each}
    </div>
  </div>
</div>

<style>
  .cs {
    position: absolute;
    inset: 0;
    z-index: 50;
    display: grid;
    place-items: center;
    padding: 16px;
    background: var(--color-scrim);
    animation: cs-fade var(--dur-base) var(--ease-out-soft);
  }
  @keyframes cs-fade {
    from {
      opacity: 0;
    }
  }
  .cs__card {
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: min(880px, 100%);
    max-height: 100%;
    min-height: 0;
    padding: var(--pad-dialog);
    border-radius: var(--r-dialog);
    background: var(--color-raised);
    box-shadow: var(--shadow-card-lg);
    animation: cs-in var(--dur-slow) var(--ease-liquid);
  }
  @keyframes cs-in {
    from {
      transform: translateY(6px) scale(0.98);
    }
  }
  .cs__head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-left: 8px;
  }
  .cs__title {
    margin: 0;
    font-size: var(--text-panel);
    font-weight: 600;
  }
  .cs__close {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--h-md);
    height: var(--h-md);
    border: 0;
    border-radius: 999px;
    background: var(--color-track);
    color: var(--color-ink-2);
  }
  .cs__close:hover {
    background: var(--color-hover);
    color: var(--color-ink);
  }
  .cs__cols {
    flex: 1;
    min-height: 0;
    overflow: auto;
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 8px;
  }
  .cs__group {
    padding: 8px;
    border-radius: var(--r-dialog-in);
    background: var(--color-track);
  }
  .cs__cap {
    margin: 0 0 4px;
    padding: 0 4px;
    color: var(--color-ink-3);
    font-size: var(--text-micro);
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .cs__list {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .cs__row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 28px;
    padding: 0 4px;
  }
  .cs__icon {
    display: inline-flex;
    color: var(--color-ink-2);
  }
  .cs__label {
    flex: 1;
    min-width: 0;
    font-size: var(--text-secondary);
  }
  .cs__keys {
    display: inline-flex;
    gap: 3px;
  }
</style>
