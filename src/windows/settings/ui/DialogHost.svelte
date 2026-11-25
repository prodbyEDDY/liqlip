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
  class="dlg"
  aria-labelledby={titleId}
  oncancel={(e) => {
    e.preventDefault();
    dialogs.settle(null);
  }}
>
  {#if spec}
    {#key spec.id}
      <div class="dlg__text">
        <h2 class="dlg__title" id={titleId}>{spec.title}</h2>
        {#if spec.body}<p class="dlg__body">{spec.body}</p>{/if}
      </div>
      <div class="dlg__actions">
        <!-- Destructive dialogs focus the safe choice first. -->
        <Button variant="ghost" size="sm" onclick={() => dialogs.settle(null)} autofocus={destructive}>
          {spec.cancel ?? t("common.cancel")}
        </Button>
        {#each spec.actions as a, i (a.value)}
          <Button
            size="sm"
            variant={a.variant === "primary" ? "primary" : a.variant === "danger" ? "danger" : "secondary"}
            onclick={() => dialogs.settle(a.value)}
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
  /* Dialog geometry: r28 · pad 12 → the 32 px capsule buttons in the bottom corners are
     concentric (28 − 12 = 16); the text gets 8 px more so it breathes. */
  .dlg {
    width: min(460px, calc(100vw - 32px));
    margin: auto; /* Tailwind preflight zeroes the UA centering margin */
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
  .dlg[open]::backdrop {
    animation: dlg-fade var(--dur-base) var(--ease-out-soft);
  }
  @keyframes dlg-in {
    from {
      opacity: 0;
      transform: scale(0.96);
    }
  }
  @keyframes dlg-fade {
    from {
      opacity: 0;
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
