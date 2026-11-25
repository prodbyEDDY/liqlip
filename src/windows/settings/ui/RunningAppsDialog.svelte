<script lang="ts">
  // "Add from running…": a searchable list of apps with open windows (`apps_running`).
  // Picking adds the app and keeps the dialog open (several apps in one go); already
  // listed apps show as added. States: loading skeleton · error + retry · empty · no match.
  import Button from "$lib/components/Button.svelte";
  import EmptyState from "$lib/components/EmptyState.svelte";
  import Ico from "$lib/components/Ico.svelte";
  import SearchField from "$lib/components/SearchField.svelte";
  import Skeleton from "$lib/components/Skeleton.svelte";
  import { appsRunning } from "$lib/ipc/commands";
  import type { RunningApp } from "$lib/ipc/types";
  import { t } from "$lib/i18n/index.svelte";
  import { appDirectory } from "../apps.svelte";
  import { exeFileName } from "../logic";
  import AppIcon from "./AppIcon.svelte";

  interface Props {
    open: boolean;
    isAdded: (exe: string) => boolean;
    onPick: (app: RunningApp) => void;
  }
  let { open = $bindable(), isAdded, onPick }: Props = $props();

  let el = $state<HTMLDialogElement>();
  let apps = $state<RunningApp[]>([]);
  let status = $state<"loading" | "ready" | "error">("loading");
  let query = $state("");
  const titleId = $props.id();

  async function load() {
    status = "loading";
    try {
      const list = await appsRunning();
      // One row per executable, sorted by name.
      const seen = new Set<string>();
      apps = list
        .filter((a) => {
          const k = exeFileName(a.exe);
          if (seen.has(k)) return false;
          seen.add(k);
          return true;
        })
        .sort((a, b) => a.name.localeCompare(b.name));
      appDirectory.merge(apps);
      status = "ready";
    } catch {
      status = "error";
    }
  }

  $effect(() => {
    if (!el) return;
    if (open && !el.open) {
      query = "";
      el.showModal();
      void load();
    } else if (!open && el.open) {
      el.close();
    }
  });

  const q = $derived(query.trim().toLocaleLowerCase());
  const shown = $derived(q ? apps.filter((a) => a.name.toLocaleLowerCase().includes(q) || exeFileName(a.exe).includes(q)) : apps);
</script>

<dialog
  bind:this={el}
  class="dlg"
  aria-labelledby={titleId}
  oncancel={(e) => {
    e.preventDefault();
    open = false;
  }}
>
  <header class="dlg__head">
    <div>
      <h2 class="dlg__title" id={titleId}>{t("settings.apps.dialog.title")}</h2>
      <p class="dlg__desc">{t("settings.apps.dialog.desc")}</p>
    </div>
  </header>
  <div class="dlg__search">
    <SearchField bind:value={query} placeholder={t("settings.apps.dialog.search")} />
  </div>
  <div class="dlg__list">
    {#if status === "loading"}
      {#each [0, 1, 2, 3, 4] as i (i)}
        <div class="sk"><Skeleton width="28px" height="28px" radius={8} /><Skeleton width={`${40 + ((i * 13) % 30)}%`} height="12px" /></div>
      {/each}
    {:else if status === "error"}
      <EmptyState icon="warning" title={t("settings.apps.dialog.error")}>
        <Button variant="secondary" size="sm" icon="refresh" onclick={load}>{t("common.retry")}</Button>
      </EmptyState>
    {:else if apps.length === 0}
      <EmptyState icon="appWindow" title={t("settings.apps.dialog.empty")} />
    {:else if shown.length === 0}
      <EmptyState icon="search" title={t("settings.apps.dialog.noMatch", { q: query.trim() })} />
    {:else}
      <ul class="apps" aria-label={t("settings.apps.dialog.title")}>
        {#each shown as app (app.exe)}
          {@const added = isAdded(app.exe)}
          <li>
            <button type="button" class="app" disabled={added} onclick={() => onPick(app)}>
              <AppIcon name={app.name} url={app.iconUrl} />
              <span class="app__text">
                <span class="app__name">{app.name}</span>
                <span class="app__exe">{exeFileName(app.exe)}</span>
              </span>
              {#if added}
                <span class="app__added"><Ico name="check" size={14} />{t("settings.apps.dialog.added")}</span>
              {:else}
                <span class="app__plus"><Ico name="plus" size={16} /></span>
              {/if}
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
  <footer class="dlg__foot">
    <Button variant="primary" size="sm" onclick={() => (open = false)}>{t("common.done")}</Button>
  </footer>
</dialog>

<style>
  /* Dialog geometry: r28 · pad 12; list rows (r16) and the Done capsule (32 px) sit 12 px
     from the edges → concentric. */
  .dlg {
    display: flex;
    flex-direction: column;
    width: min(480px, calc(100vw - 32px));
    max-height: min(560px, calc(100vh - 48px));
    margin: auto; /* Tailwind preflight zeroes the UA centering margin */
    padding: var(--pad-dialog);
    border: 0;
    border-radius: var(--r-dialog);
    corner-shape: squircle;
    background: var(--color-raised);
    color: var(--color-ink);
    box-shadow: var(--shadow-card-lg);
  }
  .dlg:not([open]) {
    display: none;
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
  .dlg__head {
    padding: 10px 8px 0;
  }
  .dlg__title {
    margin: 0;
    font-size: var(--text-panel);
    font-weight: 700;
    line-height: var(--text-panel--line-height);
  }
  .dlg__desc {
    margin: 4px 0 0;
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
    line-height: var(--text-secondary--line-height);
  }
  .dlg__search {
    padding: 14px 0 8px;
  }
  .dlg__list {
    flex: 1 1 auto;
    min-height: 200px;
    margin: 0 calc(-1 * var(--pad-dialog));
    padding: 4px var(--pad-dialog);
    overflow-y: auto;
  }
  .dlg__foot {
    display: flex;
    justify-content: flex-end;
    padding-top: 12px;
  }
  .sk {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 48px;
    padding: 0 10px;
  }
  .apps {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .app {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    height: 48px;
    padding: 0 12px 0 10px;
    border: 0;
    border-radius: var(--r-dialog-in);
    background: transparent;
    color: var(--color-ink);
    font: inherit;
    text-align: left;
    transition: background-color var(--dur-fast) var(--ease-out-soft);
  }
  .app:hover:not(:disabled) {
    background: var(--color-track);
  }
  .app:active:not(:disabled) {
    background: var(--color-hover);
  }
  .app:disabled {
    cursor: default;
  }
  .app:focus-visible {
    outline-offset: -2px;
  }
  .app__text {
    display: flex;
    flex: 1 1 auto;
    flex-direction: column;
    min-width: 0;
  }
  .app__name,
  .app__exe {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .app__name {
    font-size: var(--text-row);
    font-weight: 600;
  }
  .app__exe {
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
  }
  .app__plus {
    display: inline-flex;
    color: var(--color-ink-3);
  }
  .app:hover .app__plus {
    color: var(--color-ink);
  }
  .app__added {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--color-ink-2);
    font-size: var(--text-secondary);
    font-weight: 600;
  }
</style>
