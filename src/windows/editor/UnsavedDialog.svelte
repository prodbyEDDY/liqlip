<script lang="ts" module>
  export type Answer = "save" | "discard" | "cancel";
</script>

<script lang="ts">
  // "Save changes?" (SPEC §9.5): on closing the window with unsaved changes and on opening
  // another picture while this one has changes. Save / Don't save / Cancel. Native <dialog>
  // (focus trapped, Esc = cancel), the settings window's geometry: r28 · pad 12 → 32 px capsule
  // buttons concentric in the bottom corners.
  import Button from "$lib/components/Button.svelte";
  import { t } from "$lib/i18n/index.svelte";

  interface Props {
    name: string;
    /** opening another picture (instead of closing) */
    switching: boolean;
    busy?: boolean;
    onanswer: (a: Answer) => void;
  }
  let { name, switching, busy = false, onanswer }: Props = $props();

  let el = $state<HTMLDialogElement>();
  $effect(() => {
    if (el && !el.open) el.showModal();
  });
  const titleId = $props.id();
</script>

<dialog
  bind:this={el}
  class="dlg"
  aria-labelledby={titleId}
  oncancel={(e) => {
    e.preventDefault();
    if (!busy) onanswer("cancel");
  }}
>
  <div class="dlg__text">
    <h2 class="dlg__title" id={titleId}>{t("editor.unsaved.title", { name })}</h2>
    <p class="dlg__body">{switching ? t("editor.unsaved.switchDesc") : t("editor.unsaved.desc")}</p>
  </div>
  <div class="dlg__actions">
    <Button variant="ghost" size="sm" disabled={busy} onclick={() => onanswer("cancel")}>{t("common.cancel")}</Button>
    <Button variant="secondary" size="sm" disabled={busy} onclick={() => onanswer("discard")}>{t("editor.unsaved.discard")}</Button>
    <Button size="sm" loading={busy} onclick={() => onanswer("save")} autofocus>{t("editor.unsaved.save")}</Button>
  </div>
</dialog>

<style>
  .dlg {
    width: min(460px, calc(100vw - 32px));
    margin: auto;
    padding: var(--pad-dialog);
    border: 0;
    border-radius: var(--r-dialog);
    corner-shape: squircle;
    background: var(--color-raised);
    color: var(--color-ink);
    box-shadow: var(--shadow-card-lg);
  }
  .dlg[open] {
    animation: dlg-in var(--dur-slow) var(--ease-liquid);
  }
  .dlg::backdrop {
    background: rgb(0 0 0 / 0.32);
  }
  @keyframes dlg-in {
    from {
      opacity: 0;
      transform: scale(0.96);
    }
  }
  .dlg__text {
    padding: 10px 8px 4px;
  }
  .dlg__title {
    margin: 0;
    font-size: var(--text-panel);
    font-weight: 700;
    line-height: var(--text-panel--line-height);
    overflow-wrap: anywhere;
  }
  .dlg__body {
    margin: 6px 0 0;
    color: var(--color-ink-2);
    font-size: var(--text-row);
    line-height: 1.5;
  }
  .dlg__actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 20px;
  }
</style>
