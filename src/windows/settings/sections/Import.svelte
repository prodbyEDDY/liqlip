<script lang="ts">
  // Import and export (SPEC §4) — skeleton with real controls; the commands are W2-B.
  // Liquid Tools import: probe (found / not found / error), run with progress, report.
  // Backup: export a `.liqlip`, restore with merge or replace (replace confirmed twice).
  // While the commands answer `unimplemented`, buttons are disabled with a hint — no error.
  import Button from "$lib/components/Button.svelte";
  import Ico from "$lib/components/Ico.svelte";
  import Skeleton from "$lib/components/Skeleton.svelte";
  import { backupExport, backupImport } from "$lib/ipc/commands";
  import { errorCode, errorMessage } from "$lib/ipc/errors";
  import type { BackupMode } from "$lib/ipc/types";
  import { getLocale, t } from "$lib/i18n/index.svelte";
  import { importer } from "../importer.svelte";
  import { formatCount } from "../logic";
  import { dialogs, toasts } from "../ui.svelte";
  import Group from "../ui/Group.svelte";
  import ImportProgressBar from "../ui/ImportProgressBar.svelte";
  import Row from "../ui/Row.svelte";

  $effect(() => {
    void importer.check(true);
  });

  const n = (v: number) => formatCount(v, getLocale());
  const ps = $derived(importer.probeState);
  const unavailable = $derived(ps === "unavailable");

  // ── backup ──
  let backupBusy = $state<"export" | "import" | null>(null);
  let backupUnavailable = $state(false);
  const backupOff = $derived(unavailable || backupUnavailable);

  async function exportBackup() {
    backupBusy = "export";
    try {
      const path = await backupExport();
      if (path) toasts.show({ message: t("settings.import.exported", { path }), icon: "check", duration: 6000 });
    } catch (e) {
      if (errorCode(e) === "unimplemented") backupUnavailable = true;
      else toasts.show({ message: errorMessage(e), icon: "warning" });
    } finally {
      backupBusy = null;
    }
  }

  async function restore() {
    const choice = await dialogs.ask({
      title: t("settings.import.restore.title"),
      body: t("settings.import.restore.body"),
      actions: [
        { value: "replace", label: t("settings.import.restore.replace"), variant: "danger" },
        { value: "merge", label: t("settings.import.restore.merge"), variant: "primary" },
      ],
    });
    if (!choice) return;
    if (choice === "replace") {
      const ok = await dialogs.confirm({
        title: t("settings.import.restore.replaceTitle"),
        body: t("settings.import.restore.replaceBody"),
        confirm: t("settings.import.restore.replaceConfirm"),
        danger: true,
      });
      if (!ok) return;
    }
    backupBusy = "import";
    try {
      const r = await backupImport(choice as BackupMode);
      toasts.show({ message: t("settings.import.restored", { imported: n(r.imported), skipped: n(r.skipped) }), icon: "check", duration: 6000 });
    } catch (e) {
      if (errorCode(e) === "unimplemented") backupUnavailable = true;
      else toasts.show({ message: errorMessage(e), icon: "warning" });
    } finally {
      backupBusy = null;
    }
  }
</script>

<Group title={t("settings.import.lt")}>
  <Row label={t("settings.import.lt")} desc={t("settings.import.lt.desc")} icon="download" stack>
    {#snippet control()}
      <div class="card">
        {#if ps === "loading" || ps === "idle"}
          <div class="card__line"><Skeleton width="16px" height="16px" circle /><Skeleton width="260px" height="12px" /></div>
        {:else if ps === "found" && importer.probe}
          <div class="card__line">
            <Ico name="checkCircle" size={16} />
            <span>
              {t("settings.import.lt.found", {
                clips: n(importer.probe.clips),
                favorites: n(importer.probe.favorites),
                folders: n(importer.probe.folders),
              })}
            </span>
          </div>
          {#if importer.probe.path}<code class="card__path selectable">{importer.probe.path}</code>{/if}
        {:else if ps === "notFound"}
          <div class="card__line"><Ico name="info" size={16} /><span>{t("settings.import.lt.notFound")}</span></div>
        {:else if ps === "error"}
          <div class="card__line"><Ico name="warning" size={16} /><span>{t("settings.import.lt.error")}</span></div>
        {:else}
          <div class="card__line card__line--muted"><Ico name="clock" size={16} /><span>{t("settings.import.unavailable")}</span></div>
        {/if}

        {#if importer.running}
          <ImportProgressBar progress={importer.progress} />
        {:else if importer.report}
          <div class="card__line card__line--report" role="status">
            <Ico name="check" size={16} />
            <span>
              {t("settings.import.report", {
                imported: n(importer.report.imported),
                skipped: n(importer.report.skipped),
                folders: n(importer.report.folders),
              })}
              {#if importer.report.errors > 0}{" "}{t("settings.import.reportErrors", { errors: n(importer.report.errors) })}{/if}
            </span>
          </div>
        {:else if importer.runError}
          <div class="card__line card__line--report" role="alert"><Ico name="warning" size={16} /><span>{errorMessage(importer.runError)}</span></div>
        {/if}

        <div class="card__actions">
          {#if ps === "error" || ps === "notFound"}
            <Button variant="secondary" size="sm" icon="refresh" onclick={() => importer.check(true)}>{t("common.retry")}</Button>
          {:else}
            <Button
              variant="primary"
              size="sm"
              icon="download"
              loading={importer.running}
              disabled={ps !== "found"}
              title={unavailable ? t("settings.import.unavailable") : undefined}
              onclick={() => importer.run()}
            >
              {importer.running ? t("settings.import.lt.running") : t("settings.import.lt.run")}
            </Button>
          {/if}
        </div>
      </div>
    {/snippet}
  </Row>
</Group>

<Group title={t("settings.import.backup")} desc={t("settings.import.backup.desc")}>
  <Row label={t("settings.import.export")} icon="upload" note={backupOff ? t("settings.import.unavailable") : null}>
    {#snippet control()}
      <Button
        variant="secondary"
        icon="upload"
        loading={backupBusy === "export"}
        disabled={backupOff || backupBusy !== null}
        title={backupOff ? t("settings.import.unavailable") : undefined}
        onclick={exportBackup}
      >
        {t("settings.import.export")}
      </Button>
    {/snippet}
  </Row>
  <Row label={t("settings.import.restore")} icon="download" note={backupOff ? t("settings.import.unavailable") : null}>
    {#snippet control()}
      <Button
        variant="secondary"
        icon="download"
        loading={backupBusy === "import"}
        disabled={backupOff || backupBusy !== null}
        title={backupOff ? t("settings.import.unavailable") : undefined}
        onclick={restore}
      >
        {t("settings.import.restore")}
      </Button>
    {/snippet}
  </Row>
</Group>

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 14px;
    border-radius: 12px;
    background: var(--s-inset);
  }
  .card__line {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    font-size: var(--text-row);
    line-height: 1.45;
    color: var(--color-ink);
  }
  .card__line :global(.ico) {
    margin-top: 2px;
  }
  .card__line--muted {
    color: var(--color-ink-2);
  }
  .card__line--report {
    font-size: var(--text-secondary);
    font-weight: 600;
  }
  .card__path {
    display: block;
    padding: 6px 10px;
    border-radius: 8px;
    background: var(--color-track);
    font-family: var(--font-mono);
    font-size: var(--text-meta);
    color: var(--color-ink-2);
    overflow-wrap: anywhere;
  }
  .card__actions {
    display: flex;
    gap: 8px;
  }
</style>
