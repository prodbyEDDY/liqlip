<script lang="ts" module>
  import type { CaptureSession } from "$lib/hotkeyCapture";
  // One recording at a time across the window.
  let active: { session: CaptureSession; cancel: () => void } | null = null;
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
  import { comboKeys, HOTKEY_DEFAULTS, hotkeyIssue, type HotkeyAction } from "../logic";
  import { settings } from "../settings.svelte";
  import Row from "./Row.svelte";

  interface Props {
    action: HotkeyAction;
    /** Hide the description (onboarding shows its own copy). */
    compact?: boolean;
    /** Called after a chord is committed (onboarding re-checks). */
    onChange?: (combo: string) => void;
  }
  let { action, compact = false, onChange }: Props = $props();

  let recording = $state(false);
  let localError = $state<string | null>(null);
  /** The chord last sent from this row (a rejected attempt reverts to the old value). */
  let attempted = $state<string | null>(null);
  let button = $state<HTMLButtonElement>();

  const label = $derived(t(`settings.hotkeys.${action}` as TranslationKey));
  const value = $derived(settings.value?.hotkeys[action] ?? "");
  const keys = $derived(comboKeys(value).map(display));
  const def = $derived(HOTKEY_DEFAULTS[action]);
  const error = $derived(localError ?? settings.message(`hotkeys.${action}`));
  // The current binding itself is taken (not a rejected new attempt): the engine keeps
  // retrying every 30 s, so say that it will start working on its own.
  const retrying = $derived(
    !localError && settings.error(`hotkeys.${action}`)?.code === "taken" && (attempted === null || attempted === value),
  );

  /** Windows calls the Super key "Win". */
  function display(k: string): string {
    return k === "Super" ? "Win" : k;
  }

  function stop() {
    if (active?.cancel === cancel) {
      active.session.stop();
      active = null;
    }
    recording = false;
  }

  function cancel() {
    stop();
    button?.focus();
  }

  function start() {
    active?.cancel();
    localError = null;
    recording = true;
    const session = startHotkeyCapture({ onCommit: commit, onCancel: cancel });
    active = { session, cancel };
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
          ? t("settings.hotkey.duplicate", { action: t(`settings.hotkeys.${issue.other}` as TranslationKey) })
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
      class="cap squircle"
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
          aria-label={t("settings.hotkey.reset", { combo: def })}
          title={t("settings.hotkey.reset", { combo: def })}
          onclick={() => apply(def)}
        >
          <Ico name="undo" size={15} />
        </button>
      {/if}
      {#if value}
        <button
          type="button"
          class="tool"
          aria-label={t("settings.hotkey.clear")}
          title={t("settings.hotkey.clear")}
          onclick={() => apply("")}
        >
          <Ico name="x" size={15} />
        </button>
      {/if}
    </div>
  {/snippet}
</Row>

<style>
  .cap {
    display: inline-flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    min-width: 168px;
    min-height: 40px;
    padding: 6px 12px;
    border: 0;
    border-radius: 12px;
    background: var(--color-track);
    color: var(--color-ink);
    font: inherit;
    cursor: default;
    transition:
      background-color var(--dur-fast) var(--ease-out-soft),
      box-shadow var(--dur-fast) var(--ease-out-soft);
  }
  .cap:hover {
    background: var(--color-hover);
  }
  .cap:active {
    background: var(--color-selected);
  }
  .cap--rec,
  .cap--rec:hover {
    background: var(--color-paper);
    box-shadow: inset 0 0 0 1.5px var(--color-ink);
  }
  .cap__keys {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .cap__keys :global(.kbd) {
    background: var(--s-tile);
    color: var(--color-ink);
    font-weight: 600;
  }
  .cap__plus {
    font-size: var(--text-secondary);
    font-weight: 600;
    color: var(--color-ink-3);
  }
  .cap__none {
    font-size: var(--text-row);
    color: var(--color-ink-3);
  }
  .cap__rec {
    font-size: var(--text-secondary);
    font-weight: 600;
    animation: rec-pulse 1.4s var(--ease-out-soft) infinite;
  }
  .cap__hint {
    font-size: var(--text-meta);
    color: var(--color-ink-3);
  }
  @keyframes rec-pulse {
    50% {
      opacity: 0.55;
    }
  }
  .tools {
    display: inline-flex;
    gap: 2px;
    width: 64px;
  }
  .tool {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: var(--color-ink-3);
    cursor: default;
    transition:
      background-color var(--dur-fast) var(--ease-out-soft),
      color var(--dur-fast) var(--ease-out-soft);
  }
  .tool:hover {
    background: var(--color-hover);
    color: var(--color-ink);
  }
  .tool:active {
    background: var(--color-track);
  }
</style>
