// Settings sections, in sidebar order (SPEC §4). `key` is what `settings:navigate` /
// `settings_open(section)` and `?section=` use.
import type { IconName } from "$lib/icons";

export const SECTIONS = [
  { key: "general", icon: "sliders" },
  { key: "picker", icon: "grid" },
  { key: "hotkeys", icon: "keyboard" },
  { key: "storage", icon: "database" },
  { key: "privacy", icon: "shield" },
  { key: "snippets", icon: "document" },
  { key: "ocr", icon: "ocr" },
  { key: "import", icon: "download" },
  { key: "about", icon: "info" },
] as const satisfies readonly { key: string; icon: IconName }[];

export type SectionKey = (typeof SECTIONS)[number]["key"];

export const SECTION_KEYS: readonly SectionKey[] = SECTIONS.map((s) => s.key);

/** `settings:navigate { section: "onboarding" }` (first launch) shows the onboarding. */
export const ONBOARDING = "onboarding";
