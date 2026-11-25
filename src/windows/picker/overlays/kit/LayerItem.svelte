<script lang="ts">
  // One row of a layer list (menu item, stack entry, command): a 32 px capsule concentric with
  // its layer (`--layer-r-in`). Leading icon (or custom `lead`), label (+ optional second
  // line), trailing shortcut / `trail` content / submenu chevron / check. Highlight is
  // driven by `active` (keyboard + pointer, the layer owns the index) — never by :focus.
  import type { Snippet } from "svelte";
  import Ico from "$lib/components/Ico.svelte";
  import type { IconName } from "$lib/icons";

  interface Props {
    label: string;
    icon?: IconName;
    /** Shortcut shown on the right ("Ctrl D"). */
    keys?: string;
    /** Second, quieter line (a path, a preview). Makes the row taller. */
    hint?: string;
    danger?: boolean;
    disabled?: boolean;
    active?: boolean;
    checked?: boolean;
    /** An on/off row (role "switch"): a small capsule switch on the right instead of ✓. */
    toggle?: boolean;
    submenu?: boolean;
    role?: "menuitem" | "menuitemcheckbox" | "menuitemradio" | "option" | "button" | "listitem" | "switch";
    /** A display row (not actionable): rendered as a <div>, no hover/press states. */
    plain?: boolean;
    id?: string;
    lead?: Snippet;
    trail?: Snippet;
    onclick?: (e: MouseEvent) => void;
    onpointermove?: (e: PointerEvent) => void;
  }
  let {
    label,
    icon,
    keys,
    hint,
    danger = false,
    disabled = false,
    active = false,
    checked,
    toggle,
    submenu = false,
    role = toggle !== undefined ? "switch" : "menuitem",
    plain = false,
    id,
    lead,
    trail,
    onclick,
    onpointermove,
  }: Props = $props();
</script>

<svelte:element
  this={plain ? "div" : "button"}
  type={plain ? undefined : "button"}
  {id}
  role={plain ? undefined : role}
  class="item"
  class:item--plain={plain}
  class:item--active={active}
  class:item--danger={danger}
  class:item--tall={!!hint}
  aria-checked={toggle ?? checked}
  title={hint ? `${label} — ${hint}` : undefined}
  aria-haspopup={submenu ? "menu" : undefined}
  disabled={plain ? undefined : disabled}
  onmousedown={(e: MouseEvent) => e.preventDefault()}
  {onpointermove}
  {onclick}
>
  {#if lead}
    <span class="item__lead">{@render lead()}</span>
  {:else if icon}
    <span class="item__lead"><Ico name={icon} size={16} /></span>
  {/if}
  <span class="item__text">
    <span class="item__label">{label}</span>
    {#if hint}<span class="item__hint">{hint}</span>{/if}
  </span>
  {#if trail}{@render trail()}{/if}
  {#if keys}<span class="item__keys">{keys}</span>{/if}
  {#if toggle !== undefined}
    <span class="item__switch" class:item__switch--on={toggle} aria-hidden="true"><span class="item__knob"></span></span>
  {:else if checked}<span class="item__mark"><Ico name="check" size={14} /></span>{/if}
  {#if submenu}<span class="item__mark"><Ico name="chevronRight" size={14} /></span>{/if}
</svelte:element>

<style>
  .item {
    display: flex;
    flex: none;
    align-items: center;
    gap: 10px;
    width: 100%;
    min-height: var(--h-md);
    padding: 0 12px 0 10px;
    border: 0;
    border-radius: var(--layer-r-in, 16px);
    background: transparent;
    color: var(--color-ink);
    font: inherit;
    font-size: var(--text-row);
    font-weight: 500;
    text-align: left;
  }
  .item--tall {
    min-height: 44px;
    padding-block: 6px;
  }
  .item--active {
    background: var(--color-track);
  }
  .item--plain {
    cursor: default;
  }
  .item:active:not(:disabled):not(.item--plain) {
    background: var(--color-hover);
  }
  .item:focus-visible {
    outline-offset: -2px;
  }
  .item:disabled {
    opacity: 0.45;
  }
  .item__lead {
    display: inline-flex;
    flex: 0 0 auto;
    align-items: center;
    justify-content: center;
    width: 18px;
    color: var(--color-ink-2);
  }
  .item--danger,
  .item--danger .item__lead {
    color: var(--color-danger);
  }
  .item__text {
    display: flex;
    flex: 1 1 auto;
    flex-direction: column;
    min-width: 0;
  }
  .item__label,
  .item__hint {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .item__hint {
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
    font-weight: 400;
  }
  .item__keys {
    flex: 0 0 auto;
    color: var(--color-ink-3);
    font-family: var(--font-mono);
    font-size: var(--text-meta);
  }
  .item__mark {
    display: inline-flex;
    flex: 0 0 auto;
    color: var(--color-ink-3);
  }
  /* Switch: a 28 × 16 capsule, knob 12 inset 2 (concentric: 8 − 2 = 6). */
  .item__switch {
    position: relative;
    flex: 0 0 auto;
    width: 28px;
    height: 16px;
    border-radius: 999px;
    background: var(--color-toggle-track);
    transition: background-color var(--dur-fast) var(--ease-out-soft);
  }
  .item__knob {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 12px;
    height: 12px;
    border-radius: 999px;
    background: var(--color-raised);
    box-shadow: var(--shadow-on-lift);
    transition: transform var(--dur-base) var(--ease-pill);
  }
  .item__switch--on {
    background: var(--color-ink);
  }
  .item__switch--on .item__knob {
    background: var(--color-paper);
    transform: translateX(12px);
  }
</style>
