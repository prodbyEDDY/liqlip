<script lang="ts" module>
  import type { CaptureSession } from "$lib/hotkeyCapture";
  // One recording at a time across the window.
  let active: { session: CaptureSession; cancel: (refocus?: boolean) => void } | null = null;
</script>

<script lang="ts">
  // A global shortcut: click the keys to record (native `hotkey_record_*` +
  // `hotkeys:captured` in Tauri, window listeners in a browser — `startHotkeyCapture`),
  // reset to the default, or remove. Front-side checks (duplicate within Liqlip, typing key
  // without Ctrl/Alt/Win, modifiers only) run before `settings_set`; conflicts with other
  // apps come back from the backend as `hotkeys.<action>: taken`.
  import { onDestroy } from "svelte";
  import Ico from "$lib/components/Ico.svelte";
  import Kbd from "$lib/components/Kbd.svelte";
  import { startHotkeyCapture } from "$lib/hotkeyCapture";
  import { t, type TranslationKey } from "$lib/i18n/index.svelte";
  import { comboKeys, comboLabel, HOTKEY_DEFAULTS, hotkeyIssue, hotkeySuggestions, type HotkeyAction } from "../logic";
  import { settings } from "../settings.svelte";
  import Row from "./Row.svelte";

  interface Props {
    action: HotkeyAction;
    /** Onboarding: no description (it shows its own copy) and no "remove" (the step is
     *  about choosing a shortcut, not going without one). */
    compact?: boolean;
    /** Called after a chord is committed (onboarding re-checks). */
    onChange?: (combo: string) => void;
  }
  let { action, compact = false, onChange }: Props = $props();

  let recording = $state(false);
  let localError = $state<string | null>(null);
  /** The chord last sent from this row (a rejected attempt reverts to the old value). */
  let attempted = $state<string | null>(null);
  /** Chords the engine refused from this row (not offered again as suggestions). */
  let refused = $state<string[]>([]);
  let button = $state<HTMLButtonElement>();

  const label = $derived(t(`settings.hotkeys.${action}` as TranslationKey));
  const value = $derived(settings.value?.hotkeys[action] ?? "");
  const keys = $derived(comboKeys(value).map(display));
  const def = $derived(HOTKEY_DEFAULTS[action]);
  const taken = $derived(!localError && settings.error(`hotkeys.${action}`)?.code === "taken");
  // A new chord from this row was refused (the old value stays): name it in the message.
  const rejected = $derived(taken && attempted !== null && attempted !== value ? attempted : null);
  const error = $derived(
    localError ?? (rejected ? t("settings.hotkey.takenCombo", { combo: comboLabel(rejected) }) : settings.message(`hotkeys.${action}`)),
  );
  // The current binding itself is taken (not a rejected new attempt): the engine keeps
  // retrying every 30 s, so say that it will start working on its own.
  const retrying = $derived(taken && !rejected);
  // SPEC §4: a conflict gets a clear error AND a suggestion — free chords, one click each.
  const suggestions = $derived(
    taken && settings.value ? hotkeySuggestions(action, settings.value.hotkeys, rejected ? [...refused, rejected] : refused) : [],
  );

  $effect(() => {
    if (rejected && !refused.includes(rejected)) refused = [...refused, rejected];
  });

  /** Windows calls the Super key "Win". */
  function display(k: string): string {
    return k === "Super" ? "Win" : k;
  }

  // Clicking anywhere else cancels a recording (the usual Windows expectation); mouse
  // buttons are never recorded here, so a click can't be a chord.
  function onOutside(e: PointerEvent) {
    if (button && !button.contains(e.target as Node)) cancel(false);
  }

  function stop() {
    if (active?.cancel === cancel) {
      active.session.stop();
      active = null;
    }
    recording = false;
    window.removeEventListener("pointerdown", onOutside, true);
  }

  function cancel(refocus = true) {
    stop();
    if (refocus) button?.focus();
  }

  function start() {
    active?.cancel(false);
    localError = null;
    recording = true;
    const session = startHotkeyCapture({ onCommit: commit, onCancel: cancel });
    active = { session, cancel };
    window.addEventListener("pointerdown", onOutside, true);
  }

  function commit(combo: string) {
    stop();
    button?.focus();
    apply(combo);
  }

  function apply(combo: string) {
    const hk = settings.value?.hotkeys;
    if (!hk) return;
    const issue = hotkeyIssue(combo, action, hk);
    if (issue) {
      settings.clearError(`hotkeys.${action}`);
      localError =
        issue.kind === "duplicate"
          ? t("settings.hotkey.duplicate", { combo: comboLabel(combo), action: t(`settings.hotkeys.${issue.other}` as TranslationKey) })
          : issue.kind === "needsKey"
            ? t("settings.hotkey.needsKey")
            : t("settings.hotkey.needsModifier");
      return;
    }
    localError = null;
    if (combo === hk[action]) {
      settings.clearError(`hotkeys.${action}`);
      onChange?.(combo);
      return;
    }
    attempted = combo;
    settings.set(`hotkeys.${action}`, combo);
    onChange?.(combo);
  }

  onDestroy(stop);
</script>

<Row {label} desc={compact ? null : t(`settings.hotkeys.${action}.desc` as TranslationKey)} {error} note={retrying ? t("settings.hotkey.retryNote") : null} stack={compact}>
  {#snippet control()}
    <button
      type="button"
      class="cap"
      class:cap--rec={recording}
      bind:this={button}
      aria-label={t("settings.hotkey.change", { action: label })}
      aria-pressed={recording}
      onclick={() => (recording ? cancel() : start())}
    >
      {#if recording}
        <span class="cap__rec">{t("settings.hotkey.recording")}</span>
        <span class="cap__hint">{t("settings.hotkey.recordingHint")}</span>
      {:else if keys.length > 0}
        <span class="cap__keys">
          {#each keys as k, i (k + i)}
            {#if i > 0}<span class="cap__plus">+</span>{/if}
            <Kbd>{k}</Kbd>
          {/each}
        </span>
      {:else}
        <span class="cap__none">{t("settings.hotkey.none")}</span>
      {/if}
    </button>
    <div class="tools">
      {#if def && value !== def}
        <button
          type="button"
          class="tool"
          aria-label={t("settings.hotkey.reset", { combo: comboLabel(def) })}
          title={t("settings.hotkey.reset", { combo: comboLabel(def) })}
          onclick={() => apply(def)}
        >
          <Ico name="undo" size={14} />
        </button>
      {/if}
      {#if value && !compact}
        <button
          type="button"
          class="tool"
          aria-label={t("settings.hotkey.clear")}
          title={t("settings.hotkey.clear")}
          onclick={() => apply("")}
        >
          <Ico name="x" size={14} />
        </button>
      {/if}
    </div>
  {/snippet}
  {#snippet footer()}
    {#if suggestions.length > 0}
      <div class="suggest">
        <span class="suggest__label">{t("settings.hotkey.suggest")}</span>
        {#each suggestions as combo (combo)}
          <button
            type="button"
            class="chip"
            aria-label={t("settings.hotkey.useSuggestion", { combo: comboLabel(combo) })}
            onclick={() => apply(combo)}
          >
            {#each comboKeys(combo).map(display) as k, i (k + i)}
              {#if i > 0}<span class="chip__plus">+</span>{/if}<Kbd size="sm">{k}</Kbd>
            {/each}
          </button>
        {/each}
      </div>
    {/if}
  {/snippet}
</Row>

<style>
  /* The chord: a 36 px capsule of keycaps (24 px capsules inset 6 px — concentric). While
     recording it inverts to ink so the state is unmistakable without a ring. */
  .cap {
    display: inline-flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    min-width: 168px;
    height: var(--h-lg);
    padding: 0 6px;
    border: 0;
    border-radius: 999px;
    background: var(--color-track);
    color: var(--color-ink);
    font: inherit;
    transition: background-color var(--dur-fast) var(--ease-out-soft);
  }
  .cap:hover {
    background: var(--color-hover);
  }
  .cap:active {
    background: var(--color-selected);
  }
  .cap--rec,
  .cap--rec:hover {
    padding: 0 16px;
    background: var(--color-ink);
    color: var(--color-paper);
  }
  .cap__keys {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .cap__keys :global(.kbd) {
    background: var(--color-raised);
    color: var(--color-ink);
    font-weight: 600;
    box-shadow: var(--shadow-on-lift);
  }
  .cap__plus {
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
    font-weight: 600;
  }
  .cap__none {
    padding: 0 10px;
    color: var(--color-ink-3);
    font-size: var(--text-row);
  }
  /* ink-3 on the hover wash dips under 4.5:1; the placeholder darkens with its capsule. */
  .cap:hover .cap__none {
    color: var(--color-ink-2);
  }
  /* Suggested chords: 32 px capsules of 20 px keycaps inset 6 px (concentric), the control
     wash of the tile like any other button. */
  .suggest {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .suggest__label {
    color: var(--color-ink-2);
    font-size: var(--text-secondary);
    font-weight: 600;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: var(--h-md);
    padding: 0 6px;
    border: 0;
    border-radius: 999px;
    background: var(--color-track);
    transition: background-color var(--dur-fast) var(--ease-out-soft);
  }
  .chip:hover {
    background: var(--color-hover);
  }
  .chip:active {
    background: var(--color-selected);
  }
  .chip :global(.kbd) {
    background: var(--color-raised);
    color: var(--color-ink);
    font-weight: 600;
    box-shadow: var(--shadow-on-lift);
  }
  .chip__plus {
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
    font-weight: 600;
  }
  .cap__rec {
    font-size: var(--text-secondary);
    font-weight: 600;
    line-height: 1.2;
    animation: rec-pulse 1.4s var(--ease-in-out) infinite;
  }
  .cap__hint {
    color: color-mix(in srgb, var(--color-paper) 70%, transparent);
    font-size: var(--text-meta);
    line-height: 1.2;
  }
  @keyframes rec-pulse {
    50% {
      opacity: 0.6;
    }
  }
  .tools {
    display: inline-flex;
    gap: 2px;
    width: 66px;
  }
  .tool {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--h-md);
    height: var(--h-md);
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: var(--color-ink-3);
    transition:
      background-color var(--dur-fast) var(--ease-out-soft),
      color var(--dur-fast) var(--ease-out-soft);
  }
  .tool:hover {
    background: var(--color-track);
    color: var(--color-ink);
  }
  .tool:active {
    background: var(--color-hover);
  }
</style>
