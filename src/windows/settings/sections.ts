// Settings sections, in sidebar order (SPEC §4). `key` is what `settings:navigate` /
// `settings_open(section)` and `?section=` use.
import type { IconName } from "$lib/icons";
import type { TranslationKey } from "$lib/i18n/en";

export const SECTIONS = [
  { key: "general", icon: "sliders" },
  { key: "picker", icon: "grid" },
  { key: "hotkeys", icon: "keyboard" },
  { key: "storage", icon: "database" },
  { key: "privacy", icon: "shield" },
  { key: "snippets", icon: "document" },
  // SPEC §4 / §9.7 (v1.1, W6-F): between Snippets and Text recognition.
  { key: "screenshot", icon: "camera" },
  { key: "ocr", icon: "ocr" },
  { key: "import", icon: "download" },
  { key: "about", icon: "info" },
] as const satisfies readonly { key: string; icon: IconName }[];

export type SectionKey = (typeof SECTIONS)[number]["key"];

export const SECTION_KEYS: readonly SectionKey[] = SECTIONS.map((s) => s.key);

/** `settings:navigate { section: "onboarding" }` (first launch) shows the onboarding. */
export const ONBOARDING = "onboarding";

/** i18n key of a section's title (`settings.section.<key>`; the Screenshots section lives in
 *  the screenshot block, `settings.shot.section`). */
export function sectionTitleKey(key: SectionKey): TranslationKey {
  return key === "screenshot" ? "settings.shot.section" : (`settings.section.${key}` as TranslationKey);
}

/** i18n key of a section's one-line description under its title. */
export function sectionDescKey(key: SectionKey): TranslationKey {
  return `${sectionTitleKey(key)}.desc` as TranslationKey;
}
