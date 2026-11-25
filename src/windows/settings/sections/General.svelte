<script lang="ts">
  // General: autostart, language, theme, tray icon (SPEC §4).
  import Badge from "$lib/components/Badge.svelte";
  import Dropdown from "$lib/components/Dropdown.svelte";
  import SegmentedControl from "$lib/components/SegmentedControl.svelte";
  import Toggle from "$lib/components/Toggle.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { settings } from "../settings.svelte";
  import Group from "../ui/Group.svelte";
  import Row from "../ui/Row.svelte";

  const s = $derived(settings.value!);
  const hotkey = $derived(s.hotkeys.summon || "Alt + V");

  // Language names are written in their own language (never translated).
  const languages = $derived([
    { value: "system", label: t("settings.general.language.system") },
    { value: "en", label: "English" },
    { value: "ru", label: "Русский" },
  ]);
  const themes = $derived([
    { value: "system", label: t("settings.general.theme.system") },
    { value: "light", label: t("settings.general.theme.light") },
    { value: "dark", label: t("settings.general.theme.dark") },
  ]);
</script>

<Group title={t("settings.general.startup")}>
  <Row
    label={t("settings.general.autostart")}
    desc={t("settings.general.autostart.desc", { hotkey })}
    icon="power"
    error={settings.message("general.autostart")}
  >
    {#snippet control()}
      <Toggle
        checked={s.general.autostart}
        onChange={(v) => settings.set("general.autostart", v)}
        ariaLabel={t("settings.general.autostart")}
      />
    {/snippet}
  </Row>
</Group>

<Group title={t("settings.general.appearance")}>
  <Row
    label={t("settings.general.language")}
    desc={t("settings.general.language.desc")}
    icon="languages"
    error={settings.message("general.language")}
  >
    {#snippet control()}
      <div class="w-dd">
        <Dropdown
          options={languages}
          value={s.general.language}
          onChange={(v) => settings.set("general.language", v)}
          ariaLabel={t("settings.general.language")}
        />
      </div>
    {/snippet}
  </Row>
  <Row label={t("settings.general.theme")} desc={t("settings.general.theme.desc")} icon="eye" error={settings.message("general.theme")}>
    {#snippet control()}
      <SegmentedControl
        options={themes}
        value={s.general.theme}
        onChange={(v) => settings.set("general.theme", v)}
        ariaLabel={t("settings.general.theme")}
        full={false}
      />
    {/snippet}
  </Row>
  <Row label={t("settings.general.tray")} desc={t("settings.general.tray.desc")} icon="appWindow">
    {#snippet control()}
      <Badge icon="check">{t("settings.general.tray.always")}</Badge>
    {/snippet}
  </Row>
</Group>

<style>
  .w-dd {
    width: 220px;
  }
</style>
