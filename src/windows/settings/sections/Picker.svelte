<script lang="ts">
  // Shelf: position (5 variants with mini diagrams), display, edge hover (edges only) /
  // single row (center only), behavior toggles, several-items/stack options, folders and
  // the per-app paste shortcut (SPEC §4, §3.1, §3.9, §3.11, §6 п. 4).
  import Dropdown from "$lib/components/Dropdown.svelte";
  import SegmentedControl from "$lib/components/SegmentedControl.svelte";
  import Toggle from "$lib/components/Toggle.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { monitorsList } from "$lib/ipc/commands";
  import type { MonitorInfo, PasteChord, PasteKey, Position } from "$lib/ipc/types";
  import { appDirectory } from "../apps.svelte";
  import { sameExe } from "../logic";
  import { settings } from "../settings.svelte";
  import ExeList from "../ui/ExeList.svelte";
  import Group from "../ui/Group.svelte";
  import PositionPicker from "../ui/PositionPicker.svelte";
  import Row from "../ui/Row.svelte";
  import Folders from "./Folders.svelte";

  const s = $derived(settings.value!);
  const p = $derived(s.picker);
  const edge = $derived(p.position !== "center");

  const positions = $derived(
    (["center", "left", "right", "top", "bottom"] as Position[]).map((v) => ({ value: v, label: t(`settings.picker.position.${v}`) })),
  );

  // ── monitors ──
  let monitors = $state<MonitorInfo[]>([]);
  let monStatus = $state<"loading" | "ready" | "error">("loading");
  async function loadMonitors() {
    monStatus = "loading";
    try {
      monitors = await monitorsList();
      monStatus = "ready";
    } catch {
      monStatus = "error";
    }
  }
  $effect(() => {
    void loadMonitors();
  });

  const monitorOptions = $derived.by(() => {
    const opts = [
      { value: "cursor", label: t("settings.picker.monitor.cursor") },
      { value: "primary", label: t("settings.picker.monitor.primary") },
      ...monitors.map((m) => ({
        value: m.id,
        label: t(m.primary ? "settings.picker.monitor.itemPrimary" : "settings.picker.monitor.item", {
          name: m.name || m.id,
          w: m.width,
          h: m.height,
        }),
      })),
    ];
    if (!opts.some((o) => o.value === p.monitor)) {
      opts.push({ value: p.monitor, label: t("settings.picker.monitor.missing", { name: p.monitor }) });
    }
    return opts;
  });

  const remember = $derived(
    (["never", "60s", "always"] as const).map((v) => ({ value: v, label: t(`settings.picker.rememberSearch.${v}`) })),
  );
  const separators = $derived(
    (["newline", "space", "tab", "none"] as const).map((v) => ({ value: v, label: t(`settings.picker.mergeSeparator.${v}`) })),
  );
  const clickActions = $derived([
    { value: "paste", label: t("settings.picker.clickAction.paste") },
    { value: "copy", label: t("settings.picker.clickAction.copy") },
  ]);
  // Key names are printed on keycaps in English in both locales.
  const chords: { value: PasteChord; label: string }[] = [
    { value: "ctrl+v", label: "Ctrl + V" },
    { value: "shift+insert", label: "Shift + Insert" },
    { value: "ctrl+shift+v", label: "Ctrl + Shift + V" },
  ];

  // ── paste keys ──
  const pasteKeys = $derived(p.pasteKeys);
  function setPasteKeys(next: PasteKey[]) {
    settings.set("picker.pasteKeys", next);
  }
  function chordOf(exe: string): PasteChord {
    return pasteKeys.find((k) => sameExe(k.exe, exe))?.chord ?? "ctrl+v";
  }

  const toggles = [
    ["hints", "settings.picker.hints"],
    ["favoritesFirst", "settings.picker.favoritesFirst"],
  ] as const;
  const toggles2 = [
    ["closeOnCopy", "settings.picker.closeOnCopy"],
    ["alwaysPlain", "settings.picker.alwaysPlain"],
    ["bumpOnPaste", "settings.picker.bumpOnPaste"],
  ] as const;
  const posLabel = $props.id();
</script>

<Group title={t("settings.picker.placement")}>
  <Row label={t("settings.picker.position")} desc={t("settings.picker.position.desc")} error={settings.message("picker.position")} stack>
    {#snippet control()}
      <span hidden id={posLabel}>{t("settings.picker.position")}</span>
      <PositionPicker value={p.position} options={positions} onChange={(v) => settings.set("picker.position", v)} ariaLabelledby={posLabel} />
    {/snippet}
  </Row>
  {#if edge}
    <Row label={t("settings.picker.hoverOpen")} desc={t("settings.picker.hoverOpen.desc")} error={settings.message("picker.hoverOpen")}>
      {#snippet control(ids)}
        <Toggle describedby={ids.describedby} checked={p.hoverOpen} onChange={(v) => settings.set("picker.hoverOpen", v)} ariaLabel={t("settings.picker.hoverOpen")} />
      {/snippet}
    </Row>
  {:else}
    <Row label={t("settings.picker.centerRow")} desc={t("settings.picker.centerRow.desc")} error={settings.message("picker.centerRow")}>
      {#snippet control(ids)}
        <Toggle describedby={ids.describedby} checked={p.centerRow} onChange={(v) => settings.set("picker.centerRow", v)} ariaLabel={t("settings.picker.centerRow")} />
      {/snippet}
    </Row>
  {/if}
  <Row
    label={t("settings.picker.monitor")}
    desc={t("settings.picker.monitor.desc")}
    note={monStatus === "error" ? t("settings.picker.monitor.error") : null}
    error={settings.message("picker.monitor")}
  >
    {#snippet control(ids)}
      <div class="w-dd">
        <Dropdown describedby={ids.describedby}
          options={monitorOptions}
          value={p.monitor}
          onChange={(v) => settings.set("picker.monitor", v)}
          ariaLabel={t("settings.picker.monitor")}
        />
      </div>
    {/snippet}
  </Row>
</Group>

<Group title={t("settings.picker.behavior")}>
  {#each toggles as [key, label] (key)}
    <Row label={t(label)} desc={t(`${label}.desc`)} error={settings.message(`picker.${key}`)}>
      {#snippet control(ids)}
        <Toggle describedby={ids.describedby} checked={p[key]} onChange={(v) => settings.set(`picker.${key}`, v)} ariaLabel={t(label)} />
      {/snippet}
    </Row>
  {/each}
  <Row label={t("settings.picker.clickAction")} desc={t("settings.picker.clickAction.desc")} error={settings.message("picker.clickAction")}>
    {#snippet control(ids)}
      <SegmentedControl describedby={ids.describedby}
        options={clickActions}
        value={p.clickAction}
        onChange={(v) => settings.set("picker.clickAction", v)}
        ariaLabel={t("settings.picker.clickAction")}
        full={false}
      />
    {/snippet}
  </Row>
  {#each toggles2 as [key, label] (key)}
    <Row label={t(label)} desc={t(`${label}.desc`)} error={settings.message(`picker.${key}`)}>
      {#snippet control(ids)}
        <Toggle describedby={ids.describedby} checked={p[key]} onChange={(v) => settings.set(`picker.${key}`, v)} ariaLabel={t(label)} />
      {/snippet}
    </Row>
  {/each}
  <Row label={t("settings.picker.rememberSearch")} desc={t("settings.picker.rememberSearch.desc")} error={settings.message("picker.rememberSearch")}>
    {#snippet control(ids)}
      <SegmentedControl describedby={ids.describedby}
        options={remember}
        value={p.rememberSearch}
        onChange={(v) => settings.set("picker.rememberSearch", v)}
        ariaLabel={t("settings.picker.rememberSearch")}
        full={false}
      />
    {/snippet}
  </Row>
</Group>

<Group title={t("settings.picker.stack")}>
  <Row label={t("settings.picker.mergeSeparator")} desc={t("settings.picker.mergeSeparator.desc")} error={settings.message("picker.mergeSeparator")}>
    {#snippet control(ids)}
      <div class="w-dd">
        <Dropdown describedby={ids.describedby}
          options={separators}
          value={p.mergeSeparator}
          onChange={(v) => settings.set("picker.mergeSeparator", v)}
          ariaLabel={t("settings.picker.mergeSeparator")}
        />
      </div>
    {/snippet}
  </Row>
  <Row label={t("settings.picker.newlineAfterPaste")} desc={t("settings.picker.newlineAfterPaste.desc")} error={settings.message("stack.newlineAfterPaste")}>
    {#snippet control(ids)}
      <Toggle describedby={ids.describedby}
        checked={s.stack.newlineAfterPaste}
        onChange={(v) => settings.set("stack.newlineAfterPaste", v)}
        ariaLabel={t("settings.picker.newlineAfterPaste")}
      />
    {/snippet}
  </Row>
</Group>

<Folders />

<Group title={t("settings.picker.advanced")}>
  <Row label={t("settings.picker.pasteKeys")} desc={t("settings.picker.pasteKeys.desc")} error={settings.message("picker.pasteKeys")} stack>
    {#snippet control(ids)}
      <ExeList
        items={pasteKeys.map((k) => k.exe)}
        label={t("settings.picker.pasteKeys")}
        empty={t("settings.picker.pasteKeys.empty")}
        onAdd={(exe) => setPasteKeys([...pasteKeys, { exe, chord: "shift+insert" }])}
        onRemove={(exe) => setPasteKeys(pasteKeys.filter((k) => !sameExe(k.exe, exe)))}
      >
        {#snippet trailing(exe)}
          <Dropdown describedby={ids.describedby}
            options={chords}
            value={chordOf(exe)}
            onChange={(v) => setPasteKeys(pasteKeys.map((k) => (sameExe(k.exe, exe) ? { ...k, chord: v as PasteChord } : k)))}
            ariaLabel={t("settings.picker.pasteKeys.chord", { app: appDirectory.look(exe).name })}
          />
        {/snippet}
      </ExeList>
    {/snippet}
  </Row>
</Group>

<style>
  .w-dd {
    width: 240px;
    max-width: 100%;
  }
</style>
