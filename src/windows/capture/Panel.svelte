<script lang="ts" module>
  /** What the panel's own buttons ask the host to do. */
  export type PanelAction = "text" | "pin" | "save_as" | "copy" | "cancel" | "done";
</script>

<script lang="ts">
  // The stage-2 panel (SPEC §9.2, ARCHITECTURE §18.7): the editor's Toolbar and PropsBar (the
  // host passes them as snippets — the overlay imports the editor in one place) plus the host's
  // actions: Text · Pin · Save as… · Copy · Cancel · Done (primary). Docked by the host
  // (`dockPanel`); while a gesture runs under it, it turns see-through and lets the pointer pass.
  // Buttons never keep the focus after a click (mousedown is prevented), so Enter/Space go to the
  // overlay's keys, not back to the last button (LT defect П-25); keyboard focus still works.
  // Layout (INT-5, with the real editor bars): two rows like the editor window — tools · actions
  // on top, the properties bar under them — so the panel keeps its size while the properties bar
  // grows ("changes the last object") and never jumps to another dock; one column on a narrow
  // monitor.
  import type { Snippet } from "svelte";
  import Button from "$lib/components/Button.svelte";
  import Ico from "$lib/components/Ico.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import type { IconName } from "$lib/icons";

  interface Props {
    tools: Snippet | null;
    props: Snippet | null;
    x: number;
    y: number;
    /** see-through: a gesture passes under the panel */
    ghost: boolean;
    /** the pointer passes through (any gesture is running) */
    passthrough: boolean;
    /** committing: every button is inactive */
    busy: boolean;
    /** OCR can't run (no Windows language pack) */
    ocrOff: boolean;
    measuring: boolean;
    width?: number;
    height?: number;
    onaction: (a: PanelAction) => void;
  }
  let { tools, props, x, y, ghost, passthrough, busy, ocrOff, measuring, width = $bindable(0), height = $bindable(0), onaction }: Props = $props();

  const tip = (label: string, keys?: string) => (keys ? `${label} · ${keys}` : label);

  interface IconAction {
    id: Exclude<PanelAction, "done">;
    icon: IconName;
    label: string;
    keys?: string;
    disabled?: boolean;
  }
  const actions = $derived<IconAction[]>([
    { id: "text", icon: "ocr", label: ocrOff ? t("capture.action.textOff") : t("capture.action.text"), disabled: ocrOff },
    { id: "pin", icon: "pin", label: t("capture.action.pin"), keys: "Ctrl P" },
    { id: "save_as", icon: "download", label: t("capture.action.saveAs"), keys: "Ctrl Shift S" },
    { id: "copy", icon: "copy", label: t("common.copy"), keys: "Ctrl C" },
    { id: "cancel", icon: "x", label: t("common.cancel"), keys: "Esc" },
  ]);
</script>

<div
  class="panel squircle"
  class:panel--ghost={ghost}
  class:panel--through={passthrough}
  class:panel--busy={busy}
  class:panel--measuring={measuring}
  style="left:{x}px; top:{y}px"
  role="toolbar"
  aria-label={t("capture.panel")}
  bind:clientWidth={width}
  bind:clientHeight={height}
>
  {#if tools}<div class="panel__group panel__tools">{@render tools()}</div>{/if}
  {#if props}<div class="panel__group panel__props">{@render props()}</div>{/if}
  <div class="panel__group panel__actions">
    {#each actions as a (a.id)}
      <button
        type="button"
        class="act"
        title={tip(a.label, a.keys)}
        aria-label={a.label}
        disabled={busy}
        aria-disabled={a.disabled || undefined}
        onmousedown={(e) => e.preventDefault()}
        onclick={() => !a.disabled && onaction(a.id)}
      >
        <Ico name={a.icon} size={16} />
      </button>
    {/each}
    <Button variant="primary" size="sm" icon="check" loading={busy} title={tip(t("common.done"), "Enter")} onmousedown={(e: MouseEvent) => e.preventDefault()} onclick={() => onaction("done")}>
      {t("common.done")}
    </Button>
  </div>
</div>

<style>
  /* popover pair: r24 / pad 8 → 32 px capsules concentric in the corners (design-system §3.2) */
  .panel {
    position: fixed;
    z-index: 5;
    display: grid;
    grid-template-columns: auto auto;
    grid-template-areas:
      "tools actions"
      "props props";
    align-items: center;
    justify-items: start;
    gap: 8px 12px;
    max-width: calc(100vw - 16px);
    padding: var(--pad-popover);
    border-radius: var(--r-popover);
    background: var(--color-raised);
    box-shadow: var(--shadow-pop);
    color: var(--color-ink);
    transition: opacity var(--dur-fast) var(--ease-out-soft);
  }
  .panel--ghost {
    opacity: 0.25;
  }
  .panel--through {
    pointer-events: none;
  }
  .panel--busy {
    cursor: wait;
  }
  .panel--busy :global(button) {
    cursor: wait;
  }
  .panel--measuring {
    visibility: hidden;
  }
  .panel__group {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .panel__tools {
    grid-area: tools;
  }
  .panel__props {
    grid-area: props;
  }
  .panel__actions {
    grid-area: actions;
    justify-self: end;
  }
  /* a monitor narrower than tools + actions: one column */
  @media (max-width: 860px) {
    .panel {
      grid-template-columns: auto;
      grid-template-areas:
        "tools"
        "props"
        "actions";
    }
  }
  .act {
    display: inline-flex;
    flex: none;
    align-items: center;
    justify-content: center;
    width: var(--h-md);
    height: var(--h-md);
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: var(--color-ink-2);
    transition:
      background-color var(--dur-fast) var(--ease-out-soft),
      color var(--dur-fast) var(--ease-out-soft),
      transform var(--dur-fast) var(--ease-out-soft);
  }
  .act:hover:not(:disabled, [aria-disabled="true"]) {
    background: var(--color-track);
    color: var(--color-ink);
  }
  .act:active:not(:disabled, [aria-disabled="true"]) {
    background: var(--color-hover);
    transform: scale(0.97);
  }
  /* OCR off is aria-disabled (not disabled), so its tooltip still explains why */
  .act:disabled,
  .act[aria-disabled="true"] {
    opacity: 0.45;
  }
  .panel__actions :global(.btn) {
    margin-left: 4px;
  }
</style>
