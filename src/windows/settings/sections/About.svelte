<script lang="ts">
  // About Liqlip: the mark, version, data and log folders (`app_info`, `open_path`) and the
  // licenses — Liqlip's own and the third-party list (W2-D `public/licenses/`), shown inside
  // the window on demand (Tauri doesn't open target="_blank" links).
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
  import BrandMark from "../ui/BrandMark.svelte";
  import LicenseText from "../ui/LicenseText.svelte";

  // THIRD-PARTY.txt points to heroicons.txt for the icons' MIT notice: show it here too.
  const LICENSES = [
    { key: "own", src: "licenses/LICENSE.txt", icon: "code" },
    { key: "third", src: "licenses/THIRD-PARTY.txt", icon: "document" },
    { key: "icons", src: "licenses/heroicons.txt", icon: "sparkles" },
  ] as const;
  let openLicense = $state<(typeof LICENSES)[number]["key"] | null>(null);
  const licId = $props.id();

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
  <div class="mark squircle"><BrandMark size={36} /></div>
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

<Group title={t("settings.about.licenses")} desc={t("settings.about.licenses.desc")}>
  {#each LICENSES as lic (lic.key)}
    {@const open = openLicense === lic.key}
    <Row label={t(`settings.about.license.${lic.key}`)} desc={t(`settings.about.license.${lic.key}.desc`)} icon={lic.icon}>
      {#snippet control()}
        <Button
          variant="secondary"
          size="sm"
          icon={open ? "chevronUp" : "chevronDown"}
          aria-expanded={open}
          aria-controls="{licId}-{lic.key}"
          onclick={() => (openLicense = open ? null : lic.key)}
        >
          {open ? t("settings.about.license.hide") : t("settings.about.license.show")}
        </Button>
      {/snippet}
      {#if open}
        <LicenseText src={lic.src} id="{licId}-{lic.key}" label={t(`settings.about.license.${lic.key}`)} {open} />
      {/if}
    </Row>
  {/each}
</Group>

<style>
  /* Hero tile: r16 · padding 16 → the 56 px mark sits 16 px in (≥ radius → scale radius). */
  .hero {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 16px;
    border-radius: var(--r-tile);
    background: var(--color-tile);
  }
  .mark {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    justify-content: center;
    width: 56px;
    height: 56px;
    border-radius: var(--radius-lg);
    background: var(--color-raised);
    box-shadow: var(--shadow-card);
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
    color: var(--color-ink-2);
    font-size: var(--text-row);
  }
  .hero__ver {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 28px;
    color: var(--color-ink-3);
    font-family: var(--font-mono);
    font-size: var(--text-secondary);
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
    margin: 12px 4px 24px;
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
  }
  .path {
    display: block;
    padding: 6px 12px;
    border-radius: var(--radius-sm);
    background: var(--color-track);
    color: var(--color-ink-2);
    font-family: var(--font-mono);
    font-size: var(--text-meta);
    overflow-wrap: anywhere;
  }
</style>
