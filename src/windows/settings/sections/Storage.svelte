<script lang="ts">
  // Storage: retention, limits, the 7 type toggles, space used (`storage_stats`), clear
  // history (keeps favorites/folders/snippets; Undo) and clear everything (double
  // confirmation), open the data folder (SPEC §4, §3.9).
  import Button from "$lib/components/Button.svelte";
  import Dropdown from "$lib/components/Dropdown.svelte";
  import Ico from "$lib/components/Ico.svelte";
  import SegmentedControl from "$lib/components/SegmentedControl.svelte";
  import Skeleton from "$lib/components/Skeleton.svelte";
  import Toggle from "$lib/components/Toggle.svelte";
  import { clipsClear, clipsUndo, openPath, storageStats } from "$lib/ipc/commands";
  import { errorMessage } from "$lib/ipc/errors";
  import { onClipsChanged } from "$lib/ipc/events";
  import { KINDS, type Kind, type Retention, type StorageStats } from "$lib/ipc/types";
  import type { IconName } from "$lib/icons";
  import { getLocale, t } from "$lib/i18n/index.svelte";
  import { formatBytes, formatCount, withCurrent } from "../logic";
  import { settings } from "../settings.svelte";
  import { dialogs, toasts } from "../ui.svelte";
  import Group from "../ui/Group.svelte";
  import Row from "../ui/Row.svelte";

  const s = $derived(settings.value!);
  const st = $derived(s.storage);
  const locale = $derived(getLocale());

  const KIND_ICON: Record<Kind, IconName> = {
    text: "text",
    link: "link",
    code: "code",
    color: "palette",
    contact: "at",
    image: "image",
    file: "file",
  };

  const size = (bytes: number) => {
    const { n, unit } = formatBytes(bytes, locale);
    return t(`common.size.${unit}`, { n });
  };

  const retentions = $derived(
    (["1d", "1w", "1m", "3m", "6m", "1y", "forever"] as Retention[]).map((v) => ({ value: v, label: t(`settings.storage.retention.${v}`) })),
  );
  const entries = $derived(
    withCurrent(
      [1000, 5000, 20000, 0].map((n) => ({ value: String(n), label: n ? formatCount(n, locale) : t("settings.storage.unlimited") })),
      String(st.maxEntries),
      (v) => t("settings.storage.custom", { value: formatCount(Number(v), locale) }),
    ),
  );
  const sizes = $derived(
    withCurrent(
      [256, 1024, 4096, 0].map((mb) => ({ value: String(mb), label: mb ? size(mb * 1024 * 1024) : t("settings.storage.unlimited") })),
      String(st.maxSizeMb),
      (v) => t("settings.storage.custom", { value: size(Number(v) * 1024 * 1024) }),
    ),
  );

  // ── space used ──
  let stats = $state<StorageStats | null>(null);
  let statsStatus = $state<"loading" | "ready" | "error">("loading");
  async function loadStats(quiet = false) {
    if (!quiet) statsStatus = "loading";
    try {
      stats = await storageStats();
      statsStatus = "ready";
    } catch {
      if (!quiet) statsStatus = "error";
    }
  }
  $effect(() => {
    void loadStats();
    let timer: ReturnType<typeof setTimeout> | undefined;
    const off = onClipsChanged(() => {
      clearTimeout(timer);
      timer = setTimeout(() => void loadStats(true), 600);
    });
    return () => {
      clearTimeout(timer);
      void off.then((f) => f());
    };
  });

  type StatKey = "items" | "db" | "media" | "total";
  const cells = $derived<{ key: StatKey; value: string | null }[]>([
    { key: "items", value: stats ? formatCount(stats.clips, locale) : null },
    { key: "db", value: stats ? size(stats.dbBytes) : null },
    { key: "media", value: stats ? size(stats.mediaBytes) : null },
    { key: "total", value: stats ? size(stats.dbBytes + stats.mediaBytes) : null },
  ]);

  // ── clearing ──
  let clearing = $state<"history" | "all" | null>(null);

  async function clearHistory() {
    const ok = await dialogs.confirm({
      title: t("settings.storage.clearHistory.title"),
      body: t("settings.storage.clearHistory.body"),
      confirm: t("settings.storage.clearHistory"),
      danger: true,
    });
    if (!ok) return;
    clearing = "history";
    try {
      const { undoToken } = await clipsClear("history");
      toasts.show({
        message: t("settings.storage.cleared"),
        icon: "check",
        action: undoToken
          ? {
              label: t("common.undo"),
              run: async () => {
                const restored = await clipsUndo(undoToken).catch(() => false);
                toasts.show({
                  message: t(restored ? "settings.toast.restored" : "settings.toast.undoExpired"),
                  icon: restored ? "check" : "info",
                });
                void loadStats(true);
              },
            }
          : undefined,
      });
      void loadStats(true);
    } catch (e) {
      toasts.show({ message: errorMessage(e), icon: "warning" });
    } finally {
      clearing = null;
    }
  }

  async function clearAll() {
    const first = await dialogs.ask({
      title: t("settings.storage.clearAll.title"),
      body: t("settings.storage.clearAll.body"),
      actions: [{ value: "next", label: t("settings.storage.clearAll.continue"), variant: "danger" }],
    });
    if (!first) return;
    const second = await dialogs.confirm({
      title: t("settings.storage.clearAll.title2"),
      body: t("settings.storage.clearAll.body2", { n: formatCount(stats?.clips ?? 0, locale) }),
      confirm: t("settings.storage.clearAll.confirm"),
      danger: true,
    });
    if (!second) return;
    clearing = "all";
    try {
      await clipsClear("all");
      toasts.show({ message: t("settings.storage.clearedAll"), icon: "check" });
      void loadStats(true);
    } catch (e) {
      toasts.show({ message: errorMessage(e), icon: "warning" });
    } finally {
      clearing = null;
    }
  }

  async function openData() {
    try {
      await openPath("data");
    } catch (e) {
      toasts.show({ message: errorMessage(e), icon: "warning" });
    }
  }
</script>

<Group title={t("settings.storage.history")}>
  <Row label={t("settings.storage.retention")} desc={t("settings.storage.retention.desc")} error={settings.message("storage.retention")}>
    {#snippet control(ids)}
      <div class="w-dd">
        <Dropdown describedby={ids.describedby}
          options={retentions}
          value={st.retention}
          onChange={(v) => settings.set("storage.retention", v)}
          ariaLabel={t("settings.storage.retention")}
        />
      </div>
    {/snippet}
  </Row>
  <Row label={t("settings.storage.maxEntries")} desc={t("settings.storage.maxEntries.desc")} error={settings.message("storage.maxEntries")}>
    {#snippet control(ids)}
      <SegmentedControl describedby={ids.describedby}
        options={entries}
        value={String(st.maxEntries)}
        onChange={(v) => settings.set("storage.maxEntries", Number(v))}
        ariaLabel={t("settings.storage.maxEntries")}
        full={false}
      />
    {/snippet}
  </Row>
  <Row label={t("settings.storage.maxSize")} desc={t("settings.storage.maxSize.desc")} error={settings.message("storage.maxSizeMb")}>
    {#snippet control(ids)}
      <SegmentedControl describedby={ids.describedby}
        options={sizes}
        value={String(st.maxSizeMb)}
        onChange={(v) => settings.set("storage.maxSizeMb", Number(v))}
        ariaLabel={t("settings.storage.maxSize")}
        full={false}
      />
    {/snippet}
  </Row>
  <p class="hint"><Ico name="info" size={14} />{t("settings.storage.limits.note")}</p>
</Group>

<Group title={t("settings.storage.types")} desc={t("settings.storage.types.desc")}>
  {#each KINDS as kind (kind)}
    <Row
      label={t(`settings.storage.type.${kind}`)}
      desc={t(`settings.storage.type.${kind}.desc`)}
      icon={KIND_ICON[kind]}
      error={settings.message(`storage.types.${kind}`)}
    >
      {#snippet control(ids)}
        <Toggle describedby={ids.describedby}
          checked={st.types[kind]}
          onChange={(v) => settings.set(`storage.types.${kind}`, v)}
          ariaLabel={t(`settings.storage.type.${kind}`)}
        />
      {/snippet}
    </Row>
  {/each}
</Group>

<Group title={t("settings.storage.usage")}>
  <div class="usage">
    {#if statsStatus === "error"}
      <div class="usage__error">
        <Ico name="warning" size={16} />
        <span>{t("settings.storage.usage.error")}</span>
        <Button variant="secondary" size="sm" icon="refresh" onclick={() => loadStats()}>{t("common.retry")}</Button>
      </div>
    {:else}
      <dl class="stats">
        {#each cells as c (c.key)}
          <div class="stat squircle" class:stat--total={c.key === "total"}>
            <dt>{t(`settings.storage.usage.${c.key}`)}</dt>
            <dd>
              {#if statsStatus === "loading" || c.value === null}
                <Skeleton width="64px" height="18px" />
              {:else}
                {c.value}
              {/if}
            </dd>
          </div>
        {/each}
      </dl>
    {/if}
    <div class="usage__actions">
      <Button variant="secondary" size="sm" icon="folderOpen" onclick={openData}>{t("settings.storage.openFolder")}</Button>
    </div>
  </div>
</Group>

<Group title={t("settings.storage.clear")}>
  <Row label={t("settings.storage.clearHistory")} desc={t("settings.storage.clearHistory.desc")}>
    {#snippet control()}
      <Button variant="secondary" icon="trash" loading={clearing === "history"} disabled={clearing !== null} onclick={clearHistory}>
        {t("settings.storage.clearHistory")}
      </Button>
    {/snippet}
  </Row>
  <Row label={t("settings.storage.clearAll")} desc={t("settings.storage.clearAll.desc")}>
    {#snippet control()}
      <Button variant="danger" icon="trash" loading={clearing === "all"} disabled={clearing !== null} onclick={clearAll}>
        {t("settings.storage.clearAll")}
      </Button>
    {/snippet}
  </Row>
</Group>

<style>
  .w-dd {
    width: 200px;
  }
  .hint {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    margin: 0;
    padding: 2px 12px 12px;
    color: var(--color-ink-2);
    font-size: var(--text-secondary);
    line-height: var(--text-secondary--line-height);
  }
  .hint :global(.ico) {
    margin-top: 2px;
  }
  .usage {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 12px;
  }
  .stats {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 8px;
    margin: 0;
  }
  /* Stat cells sit 16 px from the tile corner (≥ its radius) → radius from the scale. */
  .stat {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
    padding: 12px 14px;
    border-radius: var(--radius-md);
    background: var(--color-track);
  }
  .stat dt {
    overflow: hidden;
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .stat dd {
    min-height: 20px;
    margin: 0;
    color: var(--color-ink);
    font-family: var(--font-mono);
    font-size: var(--text-panel);
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .stat--total {
    background: var(--color-ink);
  }
  .stat--total dt {
    color: color-mix(in srgb, var(--color-paper) 70%, transparent);
  }
  .stat--total dd {
    color: var(--color-paper);
  }
  .usage__error {
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--color-ink-2);
    font-size: var(--text-secondary);
  }
  .usage__error span {
    flex: 1 1 auto;
  }
  .usage__actions {
    display: flex;
  }
  @media (max-width: 859px) {
    .stats {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
</style>
