<script lang="ts">
  // Screenshots (SPEC §9.7, ARCHITECTURE §18.10 `settings.screenshot`): Capture (the region
  // shortcut, quick shot, magnifier, dimensions, shutter sound + the Snipping Tool warning),
  // After capture (clipboard, folder, file name with a live preview, PNG/JPG + quality, what to
  // show, thumbnail corner and timeout) and Editor (reset tool defaults, key cheat sheet).
  // The shortcuts themselves live in Hotkeys → Screenshots.
  import Button from "$lib/components/Button.svelte";
  import Dropdown from "$lib/components/Dropdown.svelte";
  import Kbd from "$lib/components/Kbd.svelte";
  import SegmentedControl from "$lib/components/SegmentedControl.svelte";
  import Toggle from "$lib/components/Toggle.svelte";
  import { untrack } from "svelte";
  import { t, type TranslationKey } from "$lib/i18n/index.svelte";
  import type { ShotAfterCapture, ShotCorner, ShotFormat } from "$lib/ipc/types";
  import {
    comboKeys,
    JPG_QUALITY_PRESETS,
    jpgQualityValid,
    resetEditorTools,
    SHOT_PLACEHOLDERS,
    SHOT_TIMEOUT_PRESETS,
    shotFileName,
    shotNameIssue,
    snippingConflict,
    withCurrent,
  } from "../logic";
  import type { SectionKey } from "../sections";
  import { settings } from "../settings.svelte";
  import { shotStatusStore } from "../shotStatus.svelte";
  import { toasts } from "../ui.svelte";
  import Group from "../ui/Group.svelte";
  import Row from "../ui/Row.svelte";
  import SnippingWarning from "../ui/SnippingWarning.svelte";
  import TextInput from "../ui/TextInput.svelte";

  let { goTo }: { goTo: (section: SectionKey, hotkey?: string, record?: boolean) => void } = $props();

  const s = $derived(settings.value!);
  const sh = $derived(s.screenshot);
  const thumbs = $derived(sh.afterCapture === "overlay");

  $effect(() => shotStatusStore.watch());
  const snipping = $derived(snippingConflict(shotStatusStore.snippingKey, s.hotkeys));

  const captureToggles = [
    ["quick", "settings.shot.quick"],
    ["magnifier", "settings.shot.magnifier"],
    ["dimensions", "settings.shot.dimensions"],
    ["sound", "settings.shot.sound"],
  ] as const;

  const regionKeys = $derived(comboKeys(s.hotkeys.shotRegion).map((k) => (k === "Super" ? "Win" : k)));

  // ── choices ──
  const formats = $derived<{ value: ShotFormat; label: string }[]>([
    { value: "png", label: "PNG" },
    { value: "jpg", label: "JPG" },
  ]);
  const afterOptions = $derived(
    (["overlay", "editor", "none"] as ShotAfterCapture[]).map((v) => ({ value: v, label: t(`settings.shot.afterCapture.${v}`) })),
  );
  const corners = $derived((["left", "right"] as ShotCorner[]).map((v) => ({ value: v, label: t(`settings.shot.corner.${v}`) })));
  const qualityOptions = $derived(
    withCurrent(
      JPG_QUALITY_PRESETS.map((q) => ({ value: String(q), label: t("settings.shot.quality.value", { n: q }) })),
      String(sh.jpgQuality),
      (v) => t("settings.shot.quality.custom", { n: v }),
    ),
  );
  const timeoutLabel = (sec: number) => (sec === 0 ? t("settings.shot.timeout.never") : t("settings.shot.timeout.seconds", { n: sec }));
  const timeoutOptions = $derived(
    withCurrent(
      SHOT_TIMEOUT_PRESETS.map((v) => ({ value: String(v), label: timeoutLabel(v) })),
      String(sh.timeout),
      (v) => timeoutLabel(Number(v)),
    ),
  );

  // ── folder (read-only until the native folder picker lands — see the W6-F report) ──
  const folderPath = $derived(sh.folder || t("settings.shot.folder.default"));

  // ── file name: a local draft, saved once valid (debounced; at once on blur / Enter) ──
  const nameId = $props.id();
  let draft = $state("");
  /** The draft is the user's (typed, or invalid and kept with its message): a confirmed
   *  value arriving meanwhile doesn't overwrite it. */
  let editingName = false;
  let nameTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const saved = sh.fileName;
    untrack(() => {
      if (!editingName) draft = saved;
    });
  });
  const issue = $derived(shotNameIssue(draft));
  const issueText = $derived(issue ? t(`settings.shot.fileName.error.${issue}` as TranslationKey) : null);
  const preview = $derived(`${shotFileName(issue ? sh.fileName : draft, new Date(), t("settings.shot.fileName.sampleApp"), 1)}.${sh.format}`);

  function commitName() {
    clearTimeout(nameTimer);
    const v = draft.trim();
    if (shotNameIssue(v) === null && v !== sh.fileName) settings.set("screenshot.fileName", v);
  }
  function onNameInput(v: string) {
    draft = v;
    editingName = true;
    clearTimeout(nameTimer);
    if (shotNameIssue(v) === null) nameTimer = setTimeout(commitName, 500);
  }
  function onNameBlur() {
    commitName();
    // An invalid draft keeps its message until it is fixed, Esc, or the section is left.
    if (shotNameIssue(draft) === null) {
      editingName = false;
      draft = draft.trim();
    }
  }
  function onNameKey(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      commitName();
    } else if (e.key === "Escape" && draft !== sh.fileName) {
      e.preventDefault();
      e.stopPropagation();
      clearTimeout(nameTimer);
      editingName = false;
      draft = sh.fileName;
    }
  }
  /** A placeholder chip inserts at the caret (or replaces the selection). */
  function insertPlaceholder(ph: string) {
    const el = document.getElementById(`${nameId}-name`) as HTMLInputElement | null;
    const start = el?.selectionStart ?? draft.length;
    const end = el?.selectionEnd ?? draft.length;
    const next = draft.slice(0, start) + ph + draft.slice(end);
    onNameInput(next);
    queueMicrotask(() => {
      el?.focus();
      el?.setSelectionRange(start + ph.length, start + ph.length);
    });
  }

  // ── editor ──
  function resetTools() {
    let storage: Storage | null = null;
    try {
      storage = localStorage;
    } catch {
      storage = null;
    }
    const done = resetEditorTools(storage);
    toasts.show({ message: t(done ? "settings.shot.editor.resetDone" : "settings.shot.editor.resetNothing"), icon: done ? "check" : "info" });
  }

  let showKeys = $state(false);
  // SPEC §9.2–9.3. Keycap names are printed in English in both locales; words go through i18n.
  const TOOL_KEYS: [string[], string][] = [
    [["V"], "select"],
    [["A"], "arrow"],
    [["L"], "line"],
    [["R"], "rect"],
    [["E", "O"], "ellipse"],
    [["P", "D"], "pen"],
    [["H"], "marker"],
    [["T"], "text"],
    [["N"], "counter"],
    [["B"], "redact"],
    [["S"], "spotlight"],
    [["C"], "crop"],
    [["I"], "picker"],
  ];
  const ACTION_KEYS: [string[][], string][] = [
    [[["Enter"]], "done"],
    [[["Esc"]], "escape"],
    [[["Ctrl", "C"]], "copy"],
    [[["Ctrl", "S"]], "save"],
    [[["Ctrl", "Shift", "S"]], "saveAs"],
    [[["Ctrl", "P"]], "pin"],
    [[["Ctrl", "Z"], ["Ctrl", "Shift", "Z"]], "undo"],
    [[["Ctrl"]], "move"],
    [[["1…5"], ["[", "]"]], "size"],
    [[["Shift", "1…8"]], "color"],
    [[["F"]], "fill"],
    [[["Space"]], "pan"],
    [[["?"]], "help"],
  ];
</script>

<Group title={t("settings.shot.capture")}>
  {#if snipping}
    <SnippingWarning onAssign={() => goTo("hotkeys", "shotRegion", true)} />
  {/if}
  <Row label={t("settings.shot.shortcut")} desc={t("settings.shot.shortcut.desc")}>
    {#snippet control()}
      <div class="shortcut">
        {#if regionKeys.length}
          <span class="shortcut__keys" aria-label={s.hotkeys.shotRegion}>
            {#each regionKeys as k, i (k + i)}
              {#if i > 0}<span class="shortcut__plus">+</span>{/if}<Kbd>{k}</Kbd>
            {/each}
          </span>
        {:else}
          <span class="shortcut__none">{t("settings.hotkey.none")}</span>
        {/if}
        <Button variant="secondary" size="sm" icon="keyboard" onclick={() => goTo("hotkeys", "shotRegion")}>{t("settings.shot.shortcut.change")}</Button>
      </div>
    {/snippet}
  </Row>
  {#each captureToggles as [key, label] (key)}
    <Row label={t(label)} desc={t(`${label}.desc`)} error={settings.message(`screenshot.${key}`)}>
      {#snippet control(ids)}
        <Toggle describedby={ids.describedby} checked={sh[key]} onChange={(v) => settings.set(`screenshot.${key}`, v)} ariaLabel={t(label)} />
      {/snippet}
    </Row>
  {/each}
</Group>

<Group title={t("settings.shot.after")}>
  <Row label={t("settings.shot.copy")} desc={t("settings.shot.copy.desc")} error={settings.message("screenshot.copy")}>
    {#snippet control(ids)}
      <Toggle describedby={ids.describedby} checked={sh.copy} onChange={(v) => settings.set("screenshot.copy", v)} ariaLabel={t("settings.shot.copy")} />
    {/snippet}
  </Row>
  <Row label={t("settings.shot.save")} desc={t("settings.shot.save.desc")} error={settings.message("screenshot.save")}>
    {#snippet control(ids)}
      <Toggle describedby={ids.describedby} checked={sh.save} onChange={(v) => settings.set("screenshot.save", v)} ariaLabel={t("settings.shot.save")} />
    {/snippet}
  </Row>
  <Row
    label={t("settings.shot.folder")}
    desc={t("settings.shot.folder.desc")}
    error={settings.message("screenshot.folder")}
    note={t("settings.shot.folder.soon")}
    stack
  >
    {#snippet control()}
      <div class="folder">
        <span class="folder__path" class:folder__path--default={!sh.folder} title={folderPath}>{folderPath}</span>
        <Button variant="secondary" size="sm" icon="folder" disabled title={t("settings.shot.folder.soon")}>{t("settings.shot.folder.change")}</Button>
        <Button variant="ghost" size="sm" icon="folderOpen" disabled title={t("settings.shot.folder.soon")}>{t("settings.shot.folder.open")}</Button>
      </div>
    {/snippet}
  </Row>
  <Row label={t("settings.shot.fileName")} desc={t("settings.shot.fileName.desc")} error={issueText ?? settings.message("screenshot.fileName")} stack>
    {#snippet control(ids)}
      <div class="name">
        <TextInput
          id="{nameId}-name"
          value={draft}
          ariaLabel={t("settings.shot.fileName")}
          describedby={ids.describedby}
          invalid={!!issue}
          oninput={onNameInput}
          onblur={onNameBlur}
          onkeydown={onNameKey}
        />
        <div class="name__chips" role="group" aria-label={t("settings.shot.fileName.insert")}>
          {#each SHOT_PLACEHOLDERS as ph (ph)}
            <button
              type="button"
              class="chip"
              title={t(`settings.shot.fileName.ph.${ph.slice(1, -1)}` as TranslationKey)}
              aria-label={t("settings.shot.fileName.insertOne", { ph, what: t(`settings.shot.fileName.ph.${ph.slice(1, -1)}` as TranslationKey) })}
              onmousedown={(e) => e.preventDefault()}
              onclick={() => insertPlaceholder(ph)}>{ph}</button
            >
          {/each}
        </div>
        <p class="name__preview" aria-live="polite">
          <span class="name__label">{t("settings.shot.fileName.preview")}</span>
          <span class="name__file" title={preview}>{preview}</span>
        </p>
      </div>
    {/snippet}
  </Row>
  <Row label={t("settings.shot.format")} desc={t("settings.shot.format.desc")} error={settings.message("screenshot.format")}>
    {#snippet control(ids)}
      <SegmentedControl
        describedby={ids.describedby}
        options={formats}
        value={sh.format}
        onChange={(v) => settings.set("screenshot.format", v)}
        ariaLabel={t("settings.shot.format")}
        full={false}
      />
    {/snippet}
  </Row>
  {#if sh.format === "jpg"}
    <Row label={t("settings.shot.quality")} desc={t("settings.shot.quality.desc")} error={settings.message("screenshot.jpgQuality")}>
      {#snippet control(ids)}
        <div class="w-dd">
          <Dropdown
            describedby={ids.describedby}
            options={qualityOptions}
            value={String(sh.jpgQuality)}
            onChange={(v) => {
              const q = Number(v);
              if (jpgQualityValid(q)) settings.set("screenshot.jpgQuality", q);
            }}
            ariaLabel={t("settings.shot.quality")}
          />
        </div>
      {/snippet}
    </Row>
  {/if}
  <Row label={t("settings.shot.afterCapture")} desc={t("settings.shot.afterCapture.desc")} error={settings.message("screenshot.afterCapture")} stack>
    {#snippet control(ids)}
      <SegmentedControl
        describedby={ids.describedby}
        options={afterOptions}
        value={sh.afterCapture}
        onChange={(v) => settings.set("screenshot.afterCapture", v)}
        ariaLabel={t("settings.shot.afterCapture")}
      />
    {/snippet}
  </Row>
  <Row label={t("settings.shot.corner")} desc={t("settings.shot.corner.desc")} error={settings.message("screenshot.corner")} disabled={!thumbs}>
    {#snippet control(ids)}
      <SegmentedControl
        describedby={ids.describedby}
        options={corners}
        value={sh.corner}
        onChange={(v) => settings.set("screenshot.corner", v)}
        ariaLabel={t("settings.shot.corner")}
        disabled={!thumbs}
        full={false}
      />
    {/snippet}
  </Row>
  <Row label={t("settings.shot.timeout")} desc={t("settings.shot.timeout.desc")} error={settings.message("screenshot.timeout")} disabled={!thumbs}>
    {#snippet control(ids)}
      <div class="w-dd">
        <Dropdown
          describedby={ids.describedby}
          options={timeoutOptions}
          value={String(sh.timeout)}
          onChange={(v) => settings.set("screenshot.timeout", Number(v))}
          ariaLabel={t("settings.shot.timeout")}
          disabled={!thumbs}
        />
      </div>
    {/snippet}
  </Row>
</Group>

<Group title={t("settings.shot.editor")}>
  <Row label={t("settings.shot.editor.reset")} desc={t("settings.shot.editor.reset.desc")}>
    {#snippet control()}
      <Button variant="secondary" size="sm" icon="undo" onclick={resetTools}>{t("settings.shot.editor.resetRun")}</Button>
    {/snippet}
  </Row>
  <Row label={t("settings.shot.editor.keys")} desc={t("settings.shot.editor.keys.desc")}>
    {#snippet control()}
      <Button variant="secondary" size="sm" icon={showKeys ? "chevronUp" : "chevronDown"} aria-expanded={showKeys} aria-controls="{nameId}-keys" onclick={() => (showKeys = !showKeys)}>
        {t(showKeys ? "settings.shot.editor.keysHide" : "settings.shot.editor.keysShow")}
      </Button>
    {/snippet}
  </Row>
  {#if showKeys}
    <div class="sheet" id="{nameId}-keys">
      <table class="keys">
        <caption class="keys__cap">{t("settings.shot.keys.tools")}</caption>
        <tbody>
          {#each TOOL_KEYS as [caps, tool] (tool)}
            <tr>
              <td class="keys__combo">
                {#each caps as k, i (k)}
                  {#if i > 0}<span class="keys__or">/</span>{/if}<Kbd size="sm">{k}</Kbd>
                {/each}
              </td>
              <td class="keys__action">{t(`settings.shot.keys.tool.${tool}` as TranslationKey)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
      <table class="keys">
        <caption class="keys__cap">{t("settings.shot.keys.actions")}</caption>
        <tbody>
          {#each ACTION_KEYS as [alts, action] (action)}
            <tr>
              <td class="keys__combo">
                {#each alts as chord, i (i)}
                  {#if i > 0}<span class="keys__or">/</span>{/if}
                  <span class="keys__chord">
                    {#each chord as k, j (j)}
                      {#if j > 0}<span class="keys__plus">+</span>{/if}<Kbd size="sm">{k}</Kbd>
                    {/each}
                  </span>
                {/each}
              </td>
              <td class="keys__action">{t(`settings.shot.keys.action.${action}` as TranslationKey)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</Group>

<style>
  .w-dd {
    width: 240px;
    max-width: 100%;
  }
  .shortcut {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .shortcut__keys {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .shortcut__plus {
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
  }
  .shortcut__none {
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
  }
  /* Folder: the path as a quiet capsule (read-only, mono, ellipsis + the full path on hover),
     its buttons beside it; wraps under on a narrow window. */
  .folder {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .folder__path {
    flex: 1 1 220px;
    min-width: 0;
    height: 36px;
    padding: 0 14px;
    overflow: hidden;
    border-radius: 999px;
    background: var(--color-track);
    color: var(--color-ink);
    font-family: var(--font-mono);
    font-size: var(--text-secondary);
    line-height: 36px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .folder__path--default {
    color: var(--color-ink-2);
  }
  .name {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .name__chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  /* Placeholder chips: 24 px capsules, mono (they are literal text to insert). */
  .chip {
    height: 24px;
    padding: 0 10px;
    border: 0;
    border-radius: 999px;
    background: var(--color-track);
    color: var(--color-ink-2);
    font-family: var(--font-mono);
    font-size: var(--text-meta);
    font-weight: 600;
    cursor: pointer;
    transition:
      background-color var(--dur-fast) var(--ease-out-soft),
      color var(--dur-fast) var(--ease-out-soft),
      transform var(--dur-fast) var(--ease-out-soft);
  }
  .chip:hover {
    background: var(--color-hover);
    color: var(--color-ink);
  }
  .chip:active {
    background: var(--color-selected);
    transform: scale(0.97);
  }
  .name__preview {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
    margin: 0;
    font-size: var(--text-secondary);
  }
  .name__label {
    flex: 0 0 auto;
    color: var(--color-ink-3);
  }
  .name__file {
    min-width: 0;
    overflow: hidden;
    color: var(--color-ink);
    font-family: var(--font-mono);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* Cheat sheet: two quiet tables inside the tile, zebra rows r12 (= 16 − 4) like the
     Hotkeys reference. */
  .sheet {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
    gap: 4px 12px;
    padding: 4px 0 8px;
  }
  .keys {
    width: 100%;
    border-collapse: separate;
    border-spacing: 0;
    align-self: start;
  }
  .keys__cap {
    padding: 10px 12px 6px;
    color: var(--color-ink-3);
    font-size: var(--text-micro);
    font-weight: 600;
    letter-spacing: 0.06em;
    text-align: left;
    text-transform: uppercase;
  }
  .keys td {
    padding: 5px 12px;
    vertical-align: middle;
  }
  .keys tbody tr:nth-child(odd) td {
    background: color-mix(in srgb, var(--color-ink) 3.5%, transparent);
  }
  .keys tbody tr td:first-child {
    border-radius: var(--r-tile-in) 0 0 var(--r-tile-in);
  }
  .keys tbody tr td:last-child {
    border-radius: 0 var(--r-tile-in) var(--r-tile-in) 0;
  }
  .keys__combo {
    width: 46%;
    white-space: nowrap;
  }
  .keys__chord {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .keys__or,
  .keys__plus {
    margin: 0 4px;
    color: var(--color-ink-3);
    font-size: var(--text-secondary);
  }
  .keys__plus {
    margin: 0;
  }
  .keys__action {
    color: var(--color-ink);
    font-size: var(--text-secondary);
    line-height: 1.45;
  }
</style>
