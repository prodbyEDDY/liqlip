import { t } from "$lib/i18n/index.svelte";
import type { SettingsError } from "$lib/ipc/types";

/** Localized message for a hotkey field error from `SettingsResult.errors`
 *  (`code: "taken"` — RegisterHotKey refused it, e.g. Liquid Tools holds Alt + V;
 *  `code: "invalid"` — unparsable or duplicated chord). */
export function hotkeyErrorMessage(code: SettingsError["code"]): string {
  return code === "taken" ? t("common.hotkey.taken") : t("common.hotkey.invalid");
}
