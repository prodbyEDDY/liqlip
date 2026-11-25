<script lang="ts">
  // First-run onboarding (SPEC §4) in the settings window, the step list in place of the
  // sidebar: Welcome → Shortcut (conflict check through settings_set, change by recording,
  // free alternatives) → Privacy (what is never saved + ignored apps) → Import from Liquid
  // Tools (only when `import_lt_probe.found`) → Ready ("press Alt + V") →
  // `general.onboardingDone = true`. "Skip setup" finishes at once.
  import Button from "$lib/components/Button.svelte";
  import Ico from "$lib/components/Ico.svelte";
  import Kbd from "$lib/components/Kbd.svelte";
  import { getLocale, t, type TranslationKey } from "$lib/i18n/index.svelte";
  import type { IconName } from "$lib/icons";
  import { importer } from "../importer.svelte";
  import { comboKeys, formatCount, HOTKEY_ACTIONS, sameExe } from "../logic";
  import { settings } from "../settings.svelte";
  import { toasts } from "../ui.svelte";
  import ExeList from "../ui/ExeList.svelte";
  import HotkeyRow from "../ui/HotkeyRow.svelte";
  import ImportProgressBar from "../ui/ImportProgressBar.svelte";

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
  const summonKeys = $derived(comboKeys(summon || "Alt + V").map((k) => (k === "Super" ? "Win" : k)));

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
    if (current === "hotkey" && !checked) void check();
  });
  const summonError = $derived(settings.error("hotkeys.summon"));
  const SUGGESTIONS = ["Ctrl + Alt + V", "Alt + Shift + C", "Ctrl + Shift + Space"];
  const suggestions = $derived(SUGGESTIONS.filter((c) => !HOTKEY_ACTIONS.some((a) => s.hotkeys[a] === c)).slice(0, 2));

  let finishing = $state(false);
  async function finish() {
    finishing = true;
    settings.set("general.onboardingDone", true);
    await settings.idle();
    finishing = false;
    toasts.show({ message: t("onboarding.done.toast", { hotkey: summon || "Alt + V" }), icon: "check" });
    onFinish();
  }

  const titleId = "onboarding-title";
  const n = (v: number) => formatCount(v, getLocale());
</script>

<div class="ob">
  <aside class="ob__side">
    <div class="ob__label">{t("onboarding.label")}</div>
    <ol class="ob__steps">
      {#each steps as st, i (st)}
        <li class="ob__step" class:ob__step--cur={st === current} class:ob__step--done={i < index} aria-current={st === current ? "step" : undefined}>
          <span class="ob__dot squircle">
            {#if i < index}<Ico name="check" size={13} />{:else}{i + 1}{/if}
          </span>
          <span class="ob__name">{t(`onboarding.step.${st}` as TranslationKey)}</span>
        </li>
      {/each}
    </ol>
  </aside>

  <main class="ob__main" aria-labelledby={titleId}>
    <div class="ob__progress" aria-hidden="true"><span style:width="{((index + 1) / steps.length) * 100}%"></span></div>
    {#key current}
      <section class="ob__body">
        <p class="ob__count">{t("onboarding.progress", { n: index + 1, total: steps.length })}</p>
        <div class="ob__icon squircle"><Ico name={ICONS[current]} size={22} /></div>

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
            <HotkeyRow action="summon" compact />
          </div>
          <p class="ob__status" aria-live="polite">
            {#if checking}
              <Ico name="loaderCircle" size={15} />{t("onboarding.hotkey.checking")}
            {:else if !summonError && summon}
              <Ico name="checkCircle" size={15} />{t("onboarding.hotkey.ok")}
            {/if}
          </p>
          {#if summonError?.code === "taken" && suggestions.length > 0}
            <div class="ob__suggest">
              <span>{t("onboarding.hotkey.suggest")}</span>
              {#each suggestions as combo (combo)}
                <button type="button" class="chip" onclick={() => settings.set("hotkeys.summon", combo)}>
                  {#each comboKeys(combo) as k, i (k + i)}
                    {#if i > 0}<span class="chip__plus">+</span>{/if}<Kbd>{k}</Kbd>
                  {/each}
                </button>
              {/each}
            </div>
          {/if}
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
              {#if importer.runError}<p class="ob__err" role="alert"><Ico name="warning" size={14} />{t("settings.import.lt.error")}</p>{/if}
              <Button variant="primary" icon="download" disabled={importer.probeState !== "found"} onclick={() => importer.run()}>
                {t("onboarding.import.run")}
              </Button>
            {/if}
          </div>
        {:else}
          <h1 class="ob__title" id={titleId}>{t("onboarding.done.title")}</h1>
          <p class="ob__done">
            <span>{t("onboarding.done.before")}</span>
            <span class="ob__keys">
              {#each summonKeys as k, i (k + i)}
                {#if i > 0}<span class="chip__plus">+</span>{/if}<Kbd>{k}</Kbd>
              {/each}
            </span>
            <span>{t("onboarding.done.after")}</span>
          </p>
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
          <Button variant="primary" icon="check" loading={finishing} onclick={finish}>{t("onboarding.finish")}</Button>
        {:else}
          <Button variant="ghost" disabled={finishing} onclick={finish}>{t("onboarding.skip")}</Button>
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
    grid-template-columns: 232px minmax(0, 1fr);
    height: 100%;
    background: var(--s-ground);
  }
  .ob__side {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 22px 14px 18px;
    background: var(--s-side);
  }
  .ob__label {
    padding: 0 8px;
    font-size: var(--text-micro);
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--color-ink-3);
  }
  .ob__steps {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .ob__step {
    display: flex;
    align-items: center;
    gap: 11px;
    height: 40px;
    padding: 0 10px;
    border-radius: 12px;
    color: var(--color-ink-3);
    transition:
      background-color var(--dur-base) var(--ease-out-soft),
      color var(--dur-base) var(--ease-out-soft);
  }
  .ob__step--cur {
    background: var(--color-selected);
    color: var(--color-ink);
  }
  .ob__step--done {
    color: var(--color-ink-2);
  }
  .ob__dot {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: 0 0 auto;
    width: 22px;
    height: 22px;
    border-radius: 999px;
    background: var(--color-track);
    font-size: var(--text-meta);
    font-weight: 700;
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
  .ob__progress {
    height: 3px;
    background: var(--color-track);
  }
  .ob__progress span {
    display: block;
    height: 100%;
    background: var(--color-ink);
    transition: width var(--dur-slow) var(--ease-out-soft);
  }
  .ob__body {
    flex: 1 1 auto;
    overflow-y: auto;
    width: 100%;
    max-width: 600px;
    margin: 0 auto;
    padding: 36px 32px 24px;
    animation: step-in var(--dur-base) var(--ease-out-soft);
  }
  @keyframes step-in {
    from {
      opacity: 0;
      transform: translateX(8px);
    }
  }
  .ob__count {
    margin: 0 0 18px;
    font-size: var(--text-secondary);
    font-weight: 600;
    color: var(--color-ink-3);
  }
  .ob__icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 48px;
    height: 48px;
    margin-bottom: 16px;
    border-radius: 16px;
    background: var(--color-ink);
    color: var(--color-paper);
  }
  .ob__title {
    margin: 0;
    font-size: 22px;
    font-weight: 700;
    line-height: 1.25;
    letter-spacing: -0.01em;
  }
  .ob__desc {
    margin: 8px 0 0;
    font-size: var(--text-panel);
    line-height: 1.5;
    color: var(--color-ink-2);
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
    font-size: var(--text-row);
    font-weight: 600;
  }
  .ob__list :global(.ico) {
    display: inline-flex;
    padding: 7px;
    border-radius: 10px;
    background: var(--s-tile);
    color: var(--color-ink-2);
  }
  .ob__card {
    margin-top: 20px;
    border-radius: 16px;
    overflow: hidden;
  }
  .ob__card--pad {
    padding: 14px;
    background: var(--s-tile);
  }
  .ob__status,
  .ob__found,
  .ob__err {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 22px;
    margin: 12px 0 0;
    font-size: var(--text-secondary);
    font-weight: 600;
    color: var(--color-ink-2);
  }
  .ob__found {
    color: var(--color-ink);
  }
  .ob__err {
    margin: 0 0 10px;
    color: var(--color-ink);
  }
  .ob__card--pad .ob__err:last-child {
    margin: 10px 0 0;
  }
  .ob__suggest {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-top: 10px;
    font-size: var(--text-secondary);
    color: var(--color-ink-2);
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 34px;
    padding: 0 10px;
    border: 0;
    border-radius: 999px;
    background: var(--s-tile);
    cursor: default;
    transition: background-color var(--dur-fast) var(--ease-out-soft);
  }
  .chip:hover {
    background: var(--color-hover);
  }
  .chip__plus {
    margin: 0 2px;
    font-size: var(--text-meta);
    color: var(--color-ink-3);
  }
  .ob__note {
    margin: 12px 0 0;
    font-size: var(--text-secondary);
    color: var(--color-ink-3);
  }
  .ob__done {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    margin: 14px 0 0;
    font-size: var(--text-panel);
    line-height: 1.6;
    color: var(--color-ink-2);
  }
  .ob__keys {
    display: inline-flex;
    align-items: center;
    gap: 2px;
  }
  .ob__keys :global(.kbd) {
    height: 30px;
    min-width: 30px;
    padding: 0 9px;
    border-radius: 10px;
    background: var(--color-ink);
    color: var(--color-paper);
    font-size: var(--text-row);
    font-weight: 600;
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
    padding: 14px 32px 22px;
  }
  @media (max-width: 859px) {
    .ob {
      grid-template-columns: 64px minmax(0, 1fr);
    }
    .ob__label,
    .ob__name {
      display: none;
    }
    .ob__side {
      padding: 22px 10px;
      align-items: center;
    }
    .ob__step {
      justify-content: center;
      padding: 0;
      width: 40px;
    }
  }
</style>
