<script lang="ts">
  // "PrtSc opens the Snipping Tool" (SPEC §9.1, §9.7): what happens and the two ways out —
  // open the Windows Settings page with the switch (`open_windows_settings('keyboard')`,
  // ms-settings:easeofaccess-keyboard) or give Liqlip another shortcut. Liqlip never edits the
  // registry for the user. Shown by the Screenshots section and the onboarding step, only while
  // a screenshot slot is bare PrtSc (`logic.snippingConflict`).
  import Button from "$lib/components/Button.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { openWindowsSettings } from "$lib/ipc/commands";
  import { errorMessage } from "$lib/ipc/errors";
  import { toasts } from "../ui.svelte";
  import Row from "./Row.svelte";

  interface Props {
    /** "Assign another shortcut": go to (or start recording) the region shortcut. */
    onAssign: () => void;
  }
  let { onAssign }: Props = $props();

  let opening = $state(false);
  async function openSettings() {
    opening = true;
    try {
      await openWindowsSettings("keyboard");
    } catch (e) {
      toasts.show({ message: t("settings.shot.snipping.openFailed", { reason: errorMessage(e) }), icon: "warning" });
    } finally {
      opening = false;
    }
  }
</script>

<Row icon="warning" label={t("settings.shot.snipping.title")} desc={t("settings.shot.snipping.desc")} stack>
  {#snippet control()}
    <div class="ways">
      <Button variant="secondary" size="sm" icon="external" loading={opening} onclick={() => void openSettings()}>{t("settings.shot.snipping.open")}</Button>
      <Button variant="ghost" size="sm" icon="keyboard" onclick={onAssign}>{t("settings.shot.snipping.assign")}</Button>
    </div>
  {/snippet}
</Row>

<style>
  .ways {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
</style>
