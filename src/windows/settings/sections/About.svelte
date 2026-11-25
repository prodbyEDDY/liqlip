<script lang="ts">
  // About Liqlip: version, data and log folders (`app_info`, `open_path`), licenses.
  // The licenses link opens the bundled notice file (stub until W2-D packages the full
  // third-party list).
  import Button from "$lib/components/Button.svelte";
  import Ico from "$lib/components/Ico.svelte";
  import Skeleton from "$lib/components/Skeleton.svelte";
  import { appInfo, openPath } from "$lib/ipc/commands";
  import { errorMessage } from "$lib/ipc/errors";
  import type { AppMeta, PathKind } from "$lib/ipc/types";
  import { t } from "$lib/i18n/index.svelte";
  import { toasts } from "../ui.svelte";
  import Group from "../ui/Group.svelte";
  import Row from "../ui/Row.svelte";

  let info = $state<AppMeta | null>(null);
  let status = $state<"loading" | "ready" | "error">("loading");

  async function load() {
    status = "loading";
    try {
      info = await appInfo();
      status = "ready";
    } catch {
      status = "error";
    }
  }
  $effect(() => {
    void load();
  });

  async function open(which: PathKind) {
    try {
      await openPath(which);
    } catch (e) {
      toasts.show({ message: errorMessage(e), icon: "warning" });
    }
  }
</script>

<div class="hero squircle">
  <div class="mark squircle" aria-hidden="true">L</div>
  <div class="hero__text">
    <div class="hero__name">Liqlip</div>
    <div class="hero__tag">{t("settings.about.tagline")}</div>
    <div class="hero__ver">
      {#if status === "loading"}
        <Skeleton width="90px" height="12px" />
      {:else if status === "error"}
        <span class="hero__err"><Ico name="warning" size={14} />{t("settings.about.error")}</span>
        <Button variant="ghost" size="sm" icon="refresh" onclick={load}>{t("common.retry")}</Button>
      {:else if info}
        <span class="selectable">{t("settings.about.version", { version: info.version })}</span>
      {/if}
    </div>
  </div>
</div>
<p class="local"><Ico name="lock" size={14} />{t("settings.about.local")}</p>

<Group title={t("settings.about.folders")}>
  {#each [["data", "settings.about.dataDir", info?.dataDir], ["logs", "settings.about.logDir", info?.logDir]] as const as [which, key, path] (which)}
    <Row label={t(key)} desc={t(`${key}.desc`)} icon={which === "data" ? "database" : "document"}>
      {#snippet control()}
        <Button variant="secondary" size="sm" icon="folderOpen" disabled={status !== "ready"} onclick={() => open(which)}>{t("common.open")}</Button>
      {/snippet}
      {#if status === "loading"}
        <Skeleton width="60%" height="11px" />
      {:else if path}
        <code class="path selectable">{path}</code>
      {/if}
    </Row>
  {/each}
</Group>

<Group title={t("settings.about.licenses")}>
  <Row label={t("settings.about.licenses")} desc={t("settings.about.licenses.desc")} icon="info">
    {#snippet control()}
      <a class="link" href="licenses/heroicons.txt" target="_blank" rel="noreferrer">
        {t("settings.about.licenses.open")}<Ico name="external" size={14} />
      </a>
    {/snippet}
  </Row>
</Group>

<style>
  .hero {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 18px;
    border-radius: 16px;
    background: var(--s-tile);
  }
  .mark {
    display: flex;
    align-items: center;
    justify-content: center;
    flex: 0 0 auto;
    width: 56px;
    height: 56px;
    border-radius: 16px;
    background: var(--color-ink);
    color: var(--color-paper);
    font-size: 28px;
    font-weight: 800;
    letter-spacing: -0.02em;
  }
  .hero__text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .hero__name {
    font-size: var(--text-section);
    font-weight: 700;
  }
  .hero__tag {
    font-size: var(--text-row);
    color: var(--color-ink-2);
  }
  .hero__ver {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 26px;
    font-family: var(--font-mono);
    font-size: var(--text-secondary);
    color: var(--color-ink-3);
  }
  .hero__err {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-family: var(--font-sans);
  }
  .local {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 12px 4px 26px;
    font-size: var(--text-secondary);
    color: var(--color-ink-3);
  }
  .path {
    display: block;
    padding: 6px 10px;
    border-radius: 8px;
    background: var(--s-inset);
    font-family: var(--font-mono);
    font-size: var(--text-meta);
    color: var(--color-ink-2);
    overflow-wrap: anywhere;
  }
  .link {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    padding: 0 12px;
    border-radius: 12px;
    background: var(--color-track);
    color: var(--color-ink);
    font-size: var(--text-secondary);
    font-weight: 600;
    text-decoration: none;
    transition: background-color var(--dur-fast) var(--ease-out-soft);
  }
  .link:hover {
    background: var(--color-hover);
  }
</style>
