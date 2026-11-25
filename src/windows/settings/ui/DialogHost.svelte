<script lang="ts">
  // The single modal dialog of the settings window (native <dialog>: focus is trapped,
  // Esc fires `cancel`, the rest of the window is inert). Content comes from `dialogs`.
  import Button from "$lib/components/Button.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { dialogs } from "../ui.svelte";

  let el = $state<HTMLDialogElement>();
  const spec = $derived(dialogs.current);
  const destructive = $derived(spec?.actions.some((a) => a.variant === "danger") ?? false);

  $effect(() => {
    if (!el) return;
    if (spec && !el.open) el.showModal();
    if (!spec && el.open) el.close();
  });

  const titleId = $props.id();
</script>

<dialog
  bind:this={el}
  class="dlg squircle"
  aria-labelledby={titleId}
  oncancel={(e) => {
    e.preventDefault();
    dialogs.settle(null);
  }}
>
  {#if spec}
    {#key spec.id}
      <h2 class="dlg__title" id={titleId}>{spec.title}</h2>
      {#if spec.body}<p class="dlg__body">{spec.body}</p>{/if}
      <div class="dlg__actions">
        <!-- Destructive dialogs focus the safe choice first. -->
        <Button variant="ghost" onclick={() => dialogs.settle(null)} autofocus={destructive}>
          {spec.cancel ?? t("common.cancel")}
        </Button>
        {#each spec.actions as a, i (a.value)}
          <Button
            variant={a.variant === "primary" ? "primary" : "secondary"}
            onclick={() => dialogs.settle(a.value)}
            style={a.variant === "danger" ? "color: var(--color-danger)" : undefined}
            autofocus={!destructive && i === spec.actions.length - 1}
          >
            {a.label}
          </Button>
        {/each}
      </div>
    {/key}
  {/if}
</dialog>

<style>
  .dlg {
    width: min(460px, calc(100vw - 32px));
    margin: auto; /* Tailwind preflight zeroes the UA centering margin */
    padding: 22px 22px 18px;
    border: 0;
    border-radius: 22px;
    background: var(--s-dialog);
    color: var(--color-ink);
    box-shadow: var(--shadow-pop);
  }
  .dlg[open] {
    animation: dlg-in var(--dur-base) var(--ease-out-soft);
  }
  .dlg::backdrop {
    background: rgb(0 0 0 / 0.32);
  }
  .dlg[open]::backdrop {
    animation: dlg-fade var(--dur-base) var(--ease-out-soft);
  }
  @keyframes dlg-in {
    from {
      opacity: 0;
      transform: translateY(6px) scale(0.98);
    }
  }
  @keyframes dlg-fade {
    from {
      opacity: 0;
    }
  }
  .dlg__title {
    margin: 0;
    font-size: 15px;
    font-weight: 700;
    line-height: 1.35;
  }
  .dlg__body {
    margin: 8px 0 0;
    font-size: var(--text-row);
    line-height: 1.5;
    color: var(--color-ink-2);
  }
  .dlg__actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 20px;
  }
</style>
