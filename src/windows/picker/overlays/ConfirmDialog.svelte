<script lang="ts" module>
  import type { IconName } from "$lib/icons";

  export type ConfirmChoice = {
    label: string;
    variant: "primary" | "secondary" | "danger" | "ghost";
    icon?: IconName;
    /** Focused when the dialog opens (Enter picks it). */
    autofocus?: boolean;
    run: () => void;
  };

  /** `overlays.open("confirm", ctx)` — one dialog for every irreversible step (SPEC §2.4.6
   *  "delete recent", §3.9 folder deletion, unsaved edits, snippet deletion). */
  export type ConfirmCtx = {
    title: string;
    desc?: string;
    icon?: IconName;
    choices: ConfirmChoice[];
    /** Label of the cancel button (default "Cancel"). */
    cancelLabel?: string;
    onCancel?: () => void;
    /** Async part of the description (e.g. how many items will go); the choices other than
     *  cancel are disabled while it loads and when it reports `disabled`. */
    load?: () => Promise<{ desc: string; disabled?: boolean }>;
  };
</script>

<script lang="ts">
  // The confirmation dialog: centred `dialog` surface (r28 · pad 12), an icon well, title,
  // description and stacked full-width capsule buttons (labels in Russian get long). Focus is
  // trapped inside; Esc and a click outside cancel; the `autofocus` choice takes Enter.
  import { onMount, tick } from "svelte";
  import Ico from "$lib/components/Ico.svelte";
  import Button from "$lib/components/Button.svelte";
  import Skeleton from "$lib/components/Skeleton.svelte";
  import { LayerSurface } from "./kit";
  import { t } from "$lib/i18n/index.svelte";
  import type { LayerProps } from "../overlays.svelte";
  import { focusLayer, focusOnce, trapTab } from "./focus";

  let { layer, close }: LayerProps = $props();

  const ctx = $derived(layer.ctx as unknown as ConfirmCtx);
  const ids = $props.id();

  let dialog = $state<HTMLDivElement | null>(null);
  let loaded = $state<{ desc: string; disabled?: boolean } | null>(null);
  let loadFailed = $state(false);

  const focused = { v: false };
  $effect(() => focusOnce(dialog, focused));

  onMount(() => {
    const load = ctx.load;
    if (!load) return;
    load()
      .then((r) => (loaded = r))
      .catch(() => (loadFailed = true));
  });

  const waiting = $derived(!!ctx.load && loaded === null && !loadFailed);
  const blocked = $derived(waiting || !!loaded?.disabled);

  // Counted: the confirming choice takes the focus back from "Cancel".
  $effect(() => {
    if (loaded && !loaded.disabled) void tick().then(() => focusLayer(dialog));
  });

  function choose(c: ConfirmChoice) {
    close();
    c.run();
  }

  function cancel() {
    close();
    ctx.onCancel?.();
  }

  $effect(() => {
    layer.onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        cancel();
        return true;
      }
      return trapTab(e, dialog); // Enter / Space activate the focused button natively
    };
  });
</script>

<LayerSurface variant="dialog" role="alertdialog" labelledby="{ids}-title" describedby="{ids}-desc" onclose={cancel} bind:el={dialog}>
  <div class="body">
    {#if ctx.icon}<div class="icon squircle" aria-hidden="true"><Ico name={ctx.icon} size={20} /></div>{/if}
    <h2 id="{ids}-title" class="title">{ctx.title}</h2>
    <div id="{ids}-desc" class="desc" aria-live="polite">
      {#if ctx.desc}<p>{ctx.desc}</p>{/if}
      {#if waiting}
        <span class="desc__wait"><Skeleton width="80%" height="12px" radius={6} /><Skeleton width="56%" height="12px" radius={6} /></span>
      {:else if loaded}
        <p>{loaded.desc}</p>
      {/if}
    </div>
  </div>
  <div class="actions">
    {#each ctx.choices as c (c.label)}
      <Button variant={c.variant} size="sm" icon={c.icon} disabled={blocked} data-autofocus={c.autofocus && !blocked ? "" : undefined} onclick={() => choose(c)}>
        {c.label}
      </Button>
    {/each}
    <Button variant="ghost" size="sm" data-autofocus={blocked || !ctx.choices.some((c) => c.autofocus) ? "" : undefined} onclick={cancel}>
      {ctx.cancelLabel ?? t("common.cancel")}
    </Button>
  </div>
</LayerSurface>

<style>
  .body {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 12px 8px 14px;
    text-align: center;
  }
  .icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 44px;
    height: 44px;
    margin-bottom: 6px;
    border-radius: var(--radius-md);
    background: var(--color-track);
    color: var(--color-ink-2);
  }
  .title {
    margin: 0;
    color: var(--color-ink);
    font-size: var(--text-panel);
    font-weight: 600;
    line-height: var(--text-panel--line-height);
    overflow-wrap: anywhere;
    text-wrap: balance;
  }
  .desc {
    display: flex;
    flex-direction: column;
    gap: 4px;
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
    line-height: var(--text-secondary--line-height);
    text-wrap: pretty;
  }
  .desc p {
    margin: 0;
  }
  .desc__wait {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding-top: 4px;
  }
  .actions {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .actions :global(.btn) {
    width: 100%;
  }
</style>
