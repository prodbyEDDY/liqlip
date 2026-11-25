<script lang="ts">
  // Text recognition (SPEC §3.12, §4) — skeleton with real controls; the engine is W2-A.
  // The on/off and language are ordinary settings (saved now). Status and "Recognize again"
  // come from `ocr_status` / `ocr_reindex` + `ocr:progress`; while those commands answer
  // `unimplemented`, the section shows a calm "arrives in an update" state, not an error.
  import Button from "$lib/components/Button.svelte";
  import Dropdown from "$lib/components/Dropdown.svelte";
  import Ico from "$lib/components/Ico.svelte";
  import Skeleton from "$lib/components/Skeleton.svelte";
  import Toggle from "$lib/components/Toggle.svelte";
  import { ocrReindex, ocrStatus } from "$lib/ipc/commands";
  import { errorCode, errorMessage } from "$lib/ipc/errors";
  import { onOcrProgress } from "$lib/ipc/events";
  import type { OcrProgress, OcrStatus } from "$lib/ipc/types";
  import { getLocale, t } from "$lib/i18n/index.svelte";
  import { formatCount } from "../logic";
  import { settings } from "../settings.svelte";
  import { toasts } from "../ui.svelte";
  import Group from "../ui/Group.svelte";
  import Row from "../ui/Row.svelte";

  const s = $derived(settings.value!);
  let status = $state<OcrStatus | null>(null);
  let phase = $state<"loading" | "ready" | "unavailable" | "error">("loading");
  let progress = $state<OcrProgress | null>(null);
  let starting = $state(false);
  /** ocr_reindex accepted; waiting for the first ocr:progress (or a short timeout). */
  let awaiting = $state(false);
  let awaitTimer: ReturnType<typeof setTimeout> | undefined;

  async function load() {
    phase = "loading";
    try {
      status = await ocrStatus();
      phase = "ready";
    } catch (e) {
      phase = errorCode(e) === "unimplemented" ? "unavailable" : "error";
    }
  }

  $effect(() => {
    void load();
    const off = onOcrProgress((p) => {
      awaiting = false;
      clearTimeout(awaitTimer);
      progress = p;
      if (p.done >= p.total) {
        setTimeout(() => {
          if (progress === p) progress = null;
        }, 2500);
        void load();
      }
    });
    return () => {
      clearTimeout(awaitTimer);
      void off.then((f) => f());
    };
  });

  const running = $derived(awaiting || (progress !== null && progress.done < progress.total));

  // Display names of BCP-47 tags in the UI language ("ru-RU" → "русский (Россия)").
  const names = $derived.by(() => {
    try {
      return new Intl.DisplayNames([getLocale()], { type: "language" });
    } catch {
      return null;
    }
  });
  const langName = (tag: string) => names?.of(tag) ?? tag;

  const languages = $derived.by(() => {
    const tags = status?.languages ?? [];
    const opts = [{ value: "auto", label: t("settings.ocr.language.auto") }, ...tags.map((tag) => ({ value: tag, label: langName(tag) }))];
    if (!opts.some((o) => o.value === s.ocr.language)) opts.push({ value: s.ocr.language, label: langName(s.ocr.language) });
    return opts;
  });

  async function reindex() {
    starting = true;
    try {
      await ocrReindex();
      awaiting = true;
      clearTimeout(awaitTimer);
      awaitTimer = setTimeout(() => {
        awaiting = false;
        void load();
      }, 5000);
    } catch (e) {
      if (errorCode(e) === "unimplemented") phase = "unavailable";
      else toasts.show({ message: errorMessage(e), icon: "warning" });
    } finally {
      starting = false;
    }
  }

  const pct = $derived(progress ? Math.round((progress.done / Math.max(1, progress.total)) * 100) : 0);
</script>

<Group title={t("settings.ocr.recognition")}>
  <Row label={t("settings.ocr.enabled")} desc={t("settings.ocr.enabled.desc")} icon="ocr" error={settings.message("ocr.enabled")}>
    {#snippet control()}
      <Toggle checked={s.ocr.enabled} onChange={(v) => settings.set("ocr.enabled", v)} ariaLabel={t("settings.ocr.enabled")} />
    {/snippet}
  </Row>
  <Row
    label={t("settings.ocr.language")}
    desc={t("settings.ocr.language.desc")}
    icon="languages"
    disabled={!s.ocr.enabled}
    error={settings.message("ocr.language")}
  >
    {#snippet control()}
      <div class="w-dd">
        <Dropdown
          options={languages}
          value={s.ocr.language}
          disabled={!s.ocr.enabled}
          onChange={(v) => settings.set("ocr.language", v)}
          ariaLabel={t("settings.ocr.language")}
        />
      </div>
    {/snippet}
  </Row>
  <Row
    label={t("settings.ocr.reindex")}
    desc={t("settings.ocr.reindex.desc")}
    icon="refresh"
    disabled={phase !== "ready" || !s.ocr.enabled}
    note={phase === "unavailable" ? t("settings.import.unavailable") : null}
  >
    {#snippet control()}
      <Button
        variant="secondary"
        icon="refresh"
        loading={starting || running}
        disabled={phase !== "ready" || !s.ocr.enabled}
        onclick={reindex}
      >
        {running ? t("settings.ocr.reindex.running") : t("settings.ocr.reindex.button")}
      </Button>
    {/snippet}
    {#if progress}
      <div class="progress" role="progressbar" aria-valuemin={0} aria-valuemax={progress.total} aria-valuenow={progress.done}>
        <div class="progress__bar"><span style:width="{pct}%"></span></div>
        <span class="progress__text">
          {progress.done >= progress.total
            ? t("settings.ocr.reindex.done")
            : t("settings.ocr.progress", { done: formatCount(progress.done, getLocale()), total: formatCount(progress.total, getLocale()) })}
        </span>
      </div>
    {/if}
  </Row>
</Group>

<Group title={t("settings.ocr.status")}>
  <div class="status">
    {#if phase === "loading"}
      <Skeleton width="18px" height="18px" circle />
      <div class="status__sk"><Skeleton width="55%" height="12px" /><Skeleton width="35%" height="10px" /></div>
    {:else if phase === "unavailable"}
      <Ico name="clock" size={18} />
      <span class="status__text">{t("settings.ocr.unavailable")}</span>
    {:else if phase === "error"}
      <Ico name="warning" size={18} />
      <span class="status__text">{t("settings.ocr.status.error")}</span>
      <Button variant="secondary" size="sm" icon="refresh" onclick={load}>{t("common.retry")}</Button>
    {:else if status && status.available && status.languages.length > 0}
      <Ico name="checkCircle" size={18} />
      <span class="status__text">
        {t("settings.ocr.status.ready", { list: status.languages.map(langName).join(", ") })}
        {#if status.pending > 0}<span class="status__sub">{t("settings.ocr.status.pending", { n: formatCount(status.pending, getLocale()) })}</span>{/if}
      </span>
    {:else}
      <Ico name="info" size={18} />
      <span class="status__text">{t("settings.ocr.status.none")}</span>
    {/if}
  </div>
</Group>

<style>
  .w-dd {
    width: 240px;
    max-width: 100%;
  }
  .progress {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .progress__bar {
    flex: 1 1 auto;
    height: 6px;
    border-radius: 999px;
    background: var(--color-track);
    overflow: hidden;
  }
  .progress__bar span {
    display: block;
    height: 100%;
    border-radius: 999px;
    background: var(--color-ink);
    transition: width var(--dur-slow) var(--ease-out-soft);
  }
  .progress__text {
    flex: 0 0 auto;
    font-family: var(--font-mono);
    font-size: var(--text-meta);
    color: var(--color-ink-2);
  }
  .status {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 14px 16px;
    background: var(--s-tile);
    color: var(--color-ink-2);
  }
  .status :global(.ico) {
    margin-top: 1px;
  }
  .status__text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1 1 auto;
    font-size: var(--text-row);
    line-height: 1.45;
    color: var(--color-ink);
  }
  .status__sub {
    font-size: var(--text-secondary);
    color: var(--color-ink-3);
  }
  .status__sk {
    display: flex;
    flex-direction: column;
    gap: 8px;
    flex: 1 1 auto;
  }
</style>
