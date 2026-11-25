<script lang="ts">
  // Privacy (SPEC §2.4, §4): password-manager markers, ignored apps (running apps / Browse…,
  // never free text), secret patterns, pause with the live state (`capture:status`), skip
  // the next copy, delete recent (period + confirmation).
  import Button from "$lib/components/Button.svelte";
  import Ico from "$lib/components/Ico.svelte";
  import SegmentedControl from "$lib/components/SegmentedControl.svelte";
  import Skeleton from "$lib/components/Skeleton.svelte";
  import Toggle from "$lib/components/Toggle.svelte";
  import { captureIgnoreNext, capturePause, captureResume, captureStatus, clipsDeleteRecent } from "$lib/ipc/commands";
  import { errorMessage } from "$lib/ipc/errors";
  import { onCaptureStatus } from "$lib/ipc/events";
  import type { CaptureStatus } from "$lib/ipc/types";
  import { getLocale, t } from "$lib/i18n/index.svelte";
  import { pauseState, sameExe } from "../logic";
  import { settings } from "../settings.svelte";
  import { dialogs, toasts } from "../ui.svelte";
  import ExeList from "../ui/ExeList.svelte";
  import Group from "../ui/Group.svelte";
  import Row from "../ui/Row.svelte";

  const s = $derived(settings.value!);
  const pv = $derived(s.privacy);

  // ── capture state ──
  let cap = $state<CaptureStatus | null>(null);
  let capStatus = $state<"loading" | "ready" | "error">("loading");
  let now = $state(Date.now());
  let acting = $state<string | null>(null);

  async function loadCapture() {
    capStatus = "loading";
    try {
      cap = await captureStatus();
      capStatus = "ready";
    } catch {
      capStatus = "error";
    }
  }

  $effect(() => {
    void loadCapture();
    const off = onCaptureStatus((c) => {
      cap = c;
      capStatus = "ready";
    });
    const tick = setInterval(() => (now = Date.now()), 15_000);
    return () => {
      clearInterval(tick);
      void off.then((f) => f());
    };
  });

  const pause = $derived(pauseState(cap, now));
  const paused = $derived(pause.kind !== "recording");
  const timeFmt = $derived(new Intl.DateTimeFormat(getLocale(), { hour: "2-digit", minute: "2-digit" }));
  const statusText = $derived(
    pause.kind === "until"
      ? t("settings.privacy.status.until", { time: timeFmt.format(pause.at), n: pause.minutesLeft })
      : pause.kind === "indefinite"
        ? t("settings.privacy.status.indefinite")
        : t("settings.privacy.status.recording"),
  );

  async function act(key: string, fn: () => Promise<CaptureStatus>) {
    acting = key;
    try {
      cap = await fn();
      now = Date.now();
      capStatus = "ready";
    } catch (e) {
      toasts.show({ message: errorMessage(e), icon: "warning" });
    } finally {
      acting = null;
    }
  }

  const durations = [
    { key: "5", minutes: 5 },
    { key: "30", minutes: 30 },
    { key: "60", minutes: 60 },
    { key: "forever", minutes: null },
  ] as const;

  // ── delete recent ──
  let period = $state("15");
  let deleting = $state(false);
  const periods = $derived(
    (["5", "15", "30", "60", "1440"] as const).map((v) => ({ value: v, label: t(`settings.privacy.recent.${v}`) })),
  );

  async function deleteRecent() {
    const label = periods.find((p) => p.value === period)?.label ?? period;
    const ok = await dialogs.confirm({
      title: t("settings.privacy.recent.title", { period: label }),
      body: t("settings.privacy.recent.body"),
      confirm: t("common.delete"),
      danger: true,
    });
    if (!ok) return;
    deleting = true;
    try {
      const n = await clipsDeleteRecent(Number(period));
      toasts.show({ message: n > 0 ? t("settings.privacy.recent.done", { n }) : t("settings.privacy.recent.none"), icon: n > 0 ? "check" : "info" });
    } catch (e) {
      toasts.show({ message: errorMessage(e), icon: "warning" });
    } finally {
      deleting = false;
    }
  }
</script>

<Group title={t("settings.privacy.markers")}>
  <Row
    label={t("settings.privacy.respectMarkers")}
    desc={t("settings.privacy.respectMarkers.desc")}
    icon="lock"
    note={pv.respectMarkers ? null : t("settings.privacy.respectMarkers.off")}
    error={settings.message("privacy.respectMarkers")}
  >
    {#snippet control(ids)}
      <Toggle describedby={ids.describedby}
        checked={pv.respectMarkers}
        onChange={(v) => settings.set("privacy.respectMarkers", v)}
        ariaLabel={t("settings.privacy.respectMarkers")}
      />
    {/snippet}
  </Row>
</Group>

<Group title={t("settings.privacy.ignored")}>
  <Row label={t("settings.privacy.ignored")} desc={t("settings.privacy.ignored.desc")} icon="ban" error={settings.message("privacy.ignoredApps")} stack>
    {#snippet control()}
      <ExeList
        items={pv.ignoredApps}
        label={t("settings.privacy.ignored")}
        empty={t("settings.privacy.ignored.empty")}
        onAdd={(exe) => settings.set("privacy.ignoredApps", [...pv.ignoredApps, exe])}
        onRemove={(exe) => settings.set("privacy.ignoredApps", pv.ignoredApps.filter((x) => !sameExe(x, exe)))}
      />
    {/snippet}
  </Row>
</Group>

<Group title={t("settings.privacy.secrets")}>
  <Row
    label={t("settings.privacy.secretPatterns")}
    desc={t("settings.privacy.secretPatterns.desc")}
    icon="shield"
    error={settings.message("privacy.secretPatterns")}
  >
    {#snippet control(ids)}
      <Toggle describedby={ids.describedby}
        checked={pv.secretPatterns}
        onChange={(v) => settings.set("privacy.secretPatterns", v)}
        ariaLabel={t("settings.privacy.secretPatterns")}
      />
    {/snippet}
  </Row>
</Group>

<Group title={t("settings.privacy.pause")} desc={t("settings.privacy.pause.desc")}>
  <div class="pause">
    <div class="state squircle" class:state--paused={paused} aria-live="polite">
      {#if capStatus === "loading"}
        <Skeleton width="18px" height="18px" circle />
        <Skeleton width="220px" height="12px" />
      {:else if capStatus === "error"}
        <Ico name="warning" size={18} />
        <span class="state__text">{t("settings.privacy.status.error")}</span>
        <Button variant="secondary" size="sm" icon="refresh" onclick={loadCapture}>{t("common.retry")}</Button>
      {:else}
        <Ico name={paused ? "pause" : "play"} size={18} />
        <span class="state__text">
          {statusText}
          {#if cap?.ignoreNext && !paused}<span class="state__sub">{t("settings.privacy.status.ignoreNext")}</span>{/if}
        </span>
        {#if paused}
          <Button variant="primary" size="sm" icon="play" loading={acting === "resume"} onclick={() => act("resume", captureResume)}>
            {t("settings.privacy.resume")}
          </Button>
        {/if}
      {/if}
    </div>
    <div class="pause__buttons">
      {#each durations as d (d.key)}
        <Button
          variant="secondary"
          size="sm"
          icon={d.minutes === null ? "pause" : "clock"}
          disabled={capStatus !== "ready" || acting !== null}
          loading={acting === d.key}
          onclick={() => act(d.key, () => capturePause(d.minutes))}
        >
          {t(`settings.privacy.pause.${d.key}`)}
        </Button>
      {/each}
      <Button
        variant="ghost"
        size="sm"
        icon="skip"
        disabled={capStatus !== "ready" || acting !== null || paused || !!cap?.ignoreNext}
        loading={acting === "ignore"}
        onclick={() => act("ignore", captureIgnoreNext)}
      >
        {t("settings.privacy.ignoreNext")}
      </Button>
    </div>
  </div>
</Group>

<Group title={t("settings.privacy.recent")}>
  <Row label={t("settings.privacy.recent")} desc={t("settings.privacy.recent.desc")} icon="clock" stack>
    {#snippet control(ids)}
      <div class="recent">
        <SegmentedControl describedby={ids.describedby} options={periods} value={period} onChange={(v) => (period = v)} ariaLabel={t("settings.privacy.recent.period")} full={false} />
        <Button variant="danger" icon="trash" loading={deleting} onclick={deleteRecent}>
          {t("settings.privacy.recent.button")}
        </Button>
      </div>
    {/snippet}
  </Row>
</Group>

<style>
  .pause {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 12px;
  }
  /* Live status: r24 (scale) with a 32 px capsule button inset 8 px → concentric. Paused
     inverts to ink — the state reads at a glance without colour. */
  .state {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 48px;
    padding: 8px 8px 8px 16px;
    border-radius: var(--radius-2xl);
    background: var(--color-track);
    color: var(--color-ink-2);
    transition:
      background-color var(--dur-base) var(--ease-out-soft),
      color var(--dur-base) var(--ease-out-soft);
  }
  .state--paused {
    background: var(--color-ink);
    color: var(--color-paper);
  }
  .state__text {
    display: flex;
    flex: 1 1 auto;
    flex-direction: column;
    min-width: 0;
    font-size: var(--text-row);
    font-weight: 600;
  }
  .state__sub {
    font-size: var(--text-secondary);
    font-weight: 500;
    opacity: 0.8;
  }
  .state--paused :global(.btn--primary) {
    background: var(--color-paper);
    color: var(--color-ink);
  }
  .pause__buttons,
  .recent {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
</style>
