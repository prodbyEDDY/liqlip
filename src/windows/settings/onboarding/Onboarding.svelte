<script lang="ts">
  // First-run onboarding (SPEC §4) in the settings window, the step list in place of the
  // sidebar: Welcome → Shortcut (conflict check through settings_set, change by recording,
  // free alternatives) → Privacy (what is never saved + ignored apps) → Import from Liquid
  // Tools (only when `import_lt_probe.found`) → Ready ("press Alt + V") →
  // `general.onboardingDone = true`. "Skip setup" finishes at once. "Done" also hides the
  // window (W2-E, the one behaviour change of the design wave), so the very next thing the
  // user does is press the shortcut; the next time the window opens it shows the settings.
  import Button from "$lib/components/Button.svelte";
  import Ico from "$lib/components/Ico.svelte";
  import Kbd from "$lib/components/Kbd.svelte";
  import { getLocale, t, type TranslationKey } from "$lib/i18n/index.svelte";
  import { isTauri } from "$lib/ipc/runtime";
  import type { IconName } from "$lib/icons";
  import { importer } from "../importer.svelte";
  import { comboKeys, comboLabel, formatCount, importErrorKey, sameExe } from "../logic";
  import { settings } from "../settings.svelte";
  import { toasts } from "../ui.svelte";
  import ExeList from "../ui/ExeList.svelte";
  import HotkeyRow from "../ui/HotkeyRow.svelte";
  import ImportProgressBar from "../ui/ImportProgressBar.svelte";
  import BrandMark from "../ui/BrandMark.svelte";

  let { onFinish }: { onFinish: () => void } = $props();

  type Step = "welcome" | "hotkey" | "privacy" | "import" | "done";
  const ICONS: Record<Step, IconName> = { welcome: "sparkles", hotkey: "keyboard", privacy: "shield", import: "download", done: "checkCircle" };

  $effect(() => {
    void importer.check();
  });

  const steps = $derived<Step[]>(
    importer.probeState === "found" ? ["welcome", "hotkey", "privacy", "import", "done"] : ["welcome", "hotkey", "privacy", "done"],
  );
  let current = $state<Step>("welcome");
  const index = $derived(Math.max(0, steps.indexOf(current)));

  const s = $derived(settings.value!);
  const summon = $derived(s.hotkeys.summon);
  const summonKeys = $derived(comboKeys(summon).map((k) => (k === "Super" ? "Win" : k)));

  function go(delta: number) {
    const next = steps[index + delta];
    if (next) current = next;
  }

  // ── hotkey check ──
  let checking = $state(false);
  let checked = $state(false);
  async function check() {
    checking = true;
    try {
      await settings.recheck();
    } catch {
      /* the field shows nothing new; the user can still change the shortcut */
    } finally {
      checking = false;
      checked = true;
    }
  }
  $effect(() => {
    if (current !== "hotkey") return;
    if (!checked) void check();
    return settings.watchConflicts(false);
  });
  const summonError = $derived(settings.error("hotkeys.summon"));

  let finishing = $state(false);
  async function finish(hide = false) {
    finishing = true;
    settings.set("general.onboardingDone", true);
    await settings.idle();
    finishing = false;
    // Not saved → the onboarding simply shows again next time; say why now.
    const failed = settings.message("general.onboardingDone");
    toasts.show(
      failed
        ? { message: failed, icon: "warning" }
        : {
            message: summon ? t("onboarding.done.toast", { hotkey: comboLabel(summon) }) : t("onboarding.done.toastNoHotkey"),
            icon: "check",
          },
    );
    onFinish();
    // Saved → step out of the way: hide the window so the shortcut is the next thing to try.
    if (hide && !failed && isTauri()) {
      try {
        const { getCurrentWindow } = await import("@tauri-apps/api/window");
        await getCurrentWindow().hide();
      } catch {
        /* the window simply stays open */
      }
    }
  }

  const titleId = "onboarding-title";
  const n = (v: number) => formatCount(v, getLocale());
</script>

<div class="ob">
  <aside class="ob__side">
    <div class="ob__label">{t("onboarding.label")}</div>
    <ol class="ob__steps">
      {#each steps as st, i (st)}
        <li
          class="ob__step"
          class:ob__step--cur={st === current}
          class:ob__step--done={i < index}
          aria-current={st === current ? "step" : undefined}
          title={t(`onboarding.step.${st}` as TranslationKey)}
        >
          <span class="ob__dot">
            {#if i < index}<Ico name="check" size={13} />{:else}{i + 1}{/if}
          </span>
          <span class="ob__name">{t(`onboarding.step.${st}` as TranslationKey)}</span>
        </li>
      {/each}
    </ol>
  </aside>

  <main class="ob__main" aria-labelledby={titleId}>
    {#key current}
      <section class="ob__body">
        <div class="ob__meta">
          <p class="ob__count">{t("onboarding.progress", { n: index + 1, total: steps.length })}</p>
          <span class="ob__bar" aria-hidden="true"><span style:width="{((index + 1) / steps.length) * 100}%"></span></span>
        </div>
        {#if current === "welcome"}
          <div class="ob__icon ob__icon--brand squircle"><BrandMark size={32} /></div>
        {:else}
          <div class="ob__icon squircle"><Ico name={ICONS[current]} size={22} /></div>
        {/if}

        {#if current === "welcome"}
          <h1 class="ob__title" id={titleId}>{t("onboarding.welcome.title")}</h1>
          <p class="ob__desc">{t("onboarding.welcome.desc")}</p>
          <ul class="ob__list">
            <li><Ico name="search" size={16} />{t("onboarding.welcome.f1")}</li>
            <li><Ico name="paste" size={16} />{t("onboarding.welcome.f2")}</li>
            <li><Ico name="layers" size={16} />{t("onboarding.welcome.f3")}</li>
          </ul>
        {:else if current === "hotkey"}
          <h1 class="ob__title" id={titleId}>{t("onboarding.hotkey.title")}</h1>
          <p class="ob__desc">{t("onboarding.hotkey.desc")}</p>
          <div class="ob__card squircle">
            <!-- A conflict shows inside the row: the error, then free alternatives. -->
            <HotkeyRow action="summon" compact />
          </div>
          <!-- The live region stays mounted (announcements); it only takes room with content. -->
          <p class="ob__status" class:ob__status--empty={!checking && (!!summonError || !summon)} aria-live="polite">
            {#if checking}
              <Ico name="loaderCircle" size={15} />{t("onboarding.hotkey.checking")}
            {:else if !summonError && summon}
              <Ico name="checkCircle" size={15} />{t("onboarding.hotkey.ok")}
            {/if}
          </p>
        {:else if current === "privacy"}
          <h1 class="ob__title" id={titleId}>{t("onboarding.privacy.title")}</h1>
          <p class="ob__desc">{t("onboarding.privacy.desc")}</p>
          <ul class="ob__list">
            <li><Ico name="lock" size={16} />{t("onboarding.privacy.p1")}</li>
            <li><Ico name="shield" size={16} />{t("onboarding.privacy.p2")}</li>
            <li><Ico name="ban" size={16} />{t("onboarding.privacy.p3")}</li>
          </ul>
          <div class="ob__card ob__card--pad squircle">
            <ExeList
              items={s.privacy.ignoredApps}
              label={t("settings.privacy.ignored")}
              empty={t("settings.privacy.ignored.empty")}
              onAdd={(exe) => settings.set("privacy.ignoredApps", [...s.privacy.ignoredApps, exe])}
              onRemove={(exe) => settings.set("privacy.ignoredApps", s.privacy.ignoredApps.filter((x) => !sameExe(x, exe)))}
            />
            {#if settings.message("privacy.ignoredApps")}
              <p class="ob__err" role="alert"><Ico name="warning" size={14} />{settings.message("privacy.ignoredApps")}</p>
            {/if}
          </div>
          <p class="ob__note">{t("onboarding.privacy.later")}</p>
        {:else if current === "import"}
          <h1 class="ob__title" id={titleId}>{t("onboarding.import.title")}</h1>
          <p class="ob__desc">{t("onboarding.import.desc")}</p>
          {#if importer.probe}
            <p class="ob__found">
              <Ico name="checkCircle" size={16} />
              {t("settings.import.lt.found", { clips: n(importer.probe.clips), favorites: n(importer.probe.favorites), folders: n(importer.probe.folders) })}
            </p>
          {/if}
          <div class="ob__card ob__card--pad squircle">
            {#if importer.running}
              <ImportProgressBar progress={importer.progress} />
            {:else if importer.report}
              <p class="ob__found" role="status"><Ico name="check" size={16} />{t("onboarding.import.done", { imported: n(importer.report.imported) })}</p>
            {:else}
              {#if importer.runError}
                {@const key = importErrorKey(importer.runError)}
                <p class="ob__err" role="alert"><Ico name="warning" size={14} />{t(key && key !== "cancelled" ? key : "settings.import.lt.error")}</p>
              {/if}
              <Button variant="primary" icon="download" disabled={importer.probeState !== "found"} onclick={() => importer.run()}>
                {t("onboarding.import.run")}
              </Button>
            {/if}
          </div>
        {:else}
          <h1 class="ob__title" id={titleId}>{t("onboarding.done.title")}</h1>
          {#if summon}
            <p class="ob__done">
              <span>{t("onboarding.done.before")}</span>
              <span class="ob__keys">
                {#each summonKeys as k, i (k + i)}
                  {#if i > 0}<span class="ob__plus">+</span>{/if}<Kbd size="lg" strong>{k}</Kbd>
                {/each}
              </span>
              <span>{t("onboarding.done.after")}</span>
            </p>
          {:else}
            <!-- No shortcut (removed in the settings): never tell the user to press one. -->
            <p class="ob__desc">{t("onboarding.done.noHotkey")}</p>
          {/if}
        {/if}
      </section>
    {/key}

    <footer class="ob__foot">
      <div>
        {#if index > 0}
          <Button variant="ghost" icon="arrowLeft" onclick={() => go(-1)}>{t("common.back")}</Button>
        {/if}
      </div>
      <div class="ob__foot-end">
        {#if current === "done"}
          <Button variant="primary" icon="check" loading={finishing} onclick={() => finish(true)}>{t("onboarding.finish")}</Button>
        {:else}
          <Button variant="ghost" disabled={finishing} onclick={() => finish()}>{t("onboarding.skip")}</Button>
          <Button variant="primary" disabled={importer.running} onclick={() => go(1)}>
            {t("common.next")}
          </Button>
        {/if}
      </div>
    </footer>
  </main>
</div>

<style>
  .ob {
    display: grid;
    grid-template-columns: 220px minmax(0, 1fr);
    height: 100%;
    background: var(--color-ground);
    color: var(--color-ink);
  }
  .ob__side {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 20px 12px 16px;
  }
  .ob__label {
    padding: 0 12px;
    color: var(--color-ink-3);
    font-size: var(--text-micro);
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .ob__steps {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  /* Steps read like the settings sidebar: 40 px capsules, the current one raised. The 24 px
     number circle is inset 8 px → concentric with the capsule (12 + 8 = 20). */
  .ob__step {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 40px;
    padding: 0 12px 0 8px;
    border-radius: 999px;
    color: var(--color-ink-2);
    transition:
      background-color var(--dur-base) var(--ease-out-soft),
      color var(--dur-base) var(--ease-out-soft);
  }
  .ob__step--cur {
    background: var(--color-raised);
    box-shadow: var(--shadow-card);
    color: var(--color-ink);
  }
  .ob__step--done {
    color: var(--color-ink);
  }
  .ob__dot {
    display: inline-flex;
    flex: 0 0 auto;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border-radius: 999px;
    background: var(--color-track);
    color: var(--color-ink-2);
    font-size: var(--text-meta);
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }
  .ob__step--cur .ob__dot,
  .ob__step--done .ob__dot {
    background: var(--color-ink);
    color: var(--color-paper);
  }
  .ob__name {
    font-size: var(--text-row);
    font-weight: 600;
  }
  .ob__main {
    position: relative;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  /* Full-width scroller (its bar sits at the window edge, not mid-window) with the step
     centred inside at ≤ 536 px; the bottom fades where content continues under the footer. */
  .ob__body {
    flex: 1 1 auto;
    width: 100%;
    padding: 32px max(32px, calc((100% - 536px) / 2)) 24px;
    overflow-y: auto;
    mask-image: linear-gradient(to bottom, #000 calc(100% - 24px), transparent);
    animation: step-in var(--dur-base) var(--ease-out-soft);
  }
  @keyframes step-in {
    from {
      opacity: 0;
      transform: translateX(8px);
    }
  }
  /* "Step 2 of 5" with a contained capsule meter (never an edge-to-edge strip). */
  .ob__meta {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 20px;
  }
  .ob__count {
    margin: 0;
    color: var(--color-ink-2);
    font-size: var(--text-secondary);
    font-weight: 600;
  }
  .ob__bar {
    display: block;
    width: 96px;
    height: 6px;
    overflow: hidden;
    border-radius: 999px;
    background: var(--color-track);
  }
  .ob__bar span {
    display: block;
    height: 100%;
    border-radius: 999px;
    background: var(--color-ink);
    transition: width var(--dur-slow) var(--ease-out-soft);
  }
  .ob__icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 48px;
    height: 48px;
    margin-bottom: 16px;
    border-radius: var(--radius-lg);
    background: var(--color-ink);
    color: var(--color-paper);
  }
  .ob__icon--brand {
    background: var(--color-raised);
    box-shadow: var(--shadow-card);
  }
  .ob__title {
    margin: 0;
    color: var(--color-ink);
    font-size: var(--text-title);
    font-weight: 700;
    line-height: var(--text-title--line-height);
    letter-spacing: -0.01em;
    text-wrap: balance;
  }
  .ob__desc {
    margin: 8px 0 0;
    color: var(--color-ink-2);
    font-size: var(--text-panel);
    line-height: 1.55;
    text-wrap: pretty;
  }
  .ob__list {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin: 20px 0 0;
    padding: 0;
    list-style: none;
  }
  .ob__list li {
    display: flex;
    align-items: center;
    gap: 12px;
    color: var(--color-ink);
    font-size: var(--text-row);
    font-weight: 600;
    line-height: var(--text-row--line-height);
  }
  .ob__list :global(.ico) {
    display: inline-flex;
    flex: 0 0 auto;
    box-sizing: content-box;
    padding: 8px;
    border-radius: var(--radius-sm);
    corner-shape: squircle;
    background: var(--color-track);
    color: var(--color-ink);
  }
  /* Content tiles: the window's tile surface (r16). The hotkey row inside is inset 4 px
     (r12, concentric); padded tiles keep 16 px (free radius inside). */
  .ob__card {
    margin-top: 20px;
    padding: var(--pad-tile);
    border-radius: var(--r-tile);
    background: var(--color-tile);
  }
  .ob__card--pad {
    padding: 16px;
  }
  .ob__status,
  .ob__found,
  .ob__err {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 22px;
    margin: 12px 0 0;
    color: var(--color-ink-2);
    font-size: var(--text-secondary);
    font-weight: 600;
  }
  /* An empty status line must not leave a gap above the footer (the conflict case). */
  .ob__status--empty {
    min-height: 0;
    margin: 0;
  }
  .ob__found {
    color: var(--color-ink);
  }
  /* Inside a padded card the line is the card's content: no outer margin (centred). */
  .ob__card--pad .ob__found {
    margin: 0;
  }
  .ob__err {
    margin: 0 0 10px;
    color: var(--color-ink);
  }
  .ob__card--pad .ob__err:last-child {
    margin: 10px 0 0;
  }
  .ob__plus {
    margin: 0 2px;
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
  }
  .ob__note {
    margin: 12px 0 0;
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
    line-height: var(--text-secondary--line-height);
  }
  .ob__done {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    margin: 14px 0 0;
    color: var(--color-ink-2);
    font-size: var(--text-panel);
    line-height: 1.6;
  }
  .ob__keys {
    display: inline-flex;
    align-items: center;
    gap: 2px;
  }
  .ob__foot-end {
    display: flex;
    gap: 8px;
  }
  .ob__foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    max-width: 600px;
    margin: 0 auto;
    padding: 12px 32px 24px;
  }
  @media (max-width: 859px) {
    .ob {
      grid-template-columns: 64px minmax(0, 1fr);
    }
    .ob__label {
      display: none;
    }
    /* Icons-only steps keep their names for screen readers (and as a tooltip). */
    .ob__name {
      position: absolute;
      width: 1px;
      height: 1px;
      overflow: hidden;
      clip-path: inset(50%);
      white-space: nowrap;
    }
    .ob__side {
      align-items: center;
      padding: 20px 12px;
    }
    .ob__step {
      justify-content: center;
      width: 40px;
      padding: 0;
    }
    .ob__body {
      padding: 24px 24px 16px;
    }
    .ob__foot {
      padding: 12px 24px 20px;
    }
  }
</style>
