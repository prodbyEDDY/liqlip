// English catalogue — the source of truth for translation keys.
// Every key here MUST exist in ru.ts (enforced by the typed Record in ru.ts).
// Hub file: each agent appends ONLY inside its own prefix block (same order in ru.ts).
export const en = {
  // --- common.* (W0) ---
  "common.ok": "OK",
  "common.cancel": "Cancel",
  "common.delete": "Delete",
  "common.undo": "Undo",
  "common.close": "Close",
  "common.save": "Save",
  "common.retry": "Try again",
  "common.clear": "Clear",
  "common.dismiss": "Dismiss",
  "common.open": "Open",
  "common.copy": "Copy",
  "common.paste": "Paste",
  "common.add": "Add",
  "common.remove": "Remove",
  "common.rename": "Rename",
  "common.back": "Back",
  "common.next": "Next",
  "common.skip": "Skip",
  "common.done": "Done",
  "common.loading": "Loading…",
  "common.on": "On",
  "common.off": "Off",

  "common.error.not_found": "This item no longer exists.",
  "common.error.busy": "The clipboard or the database is busy. Try again in a moment.",
  "common.error.invalid": "This value isn't valid.",
  "common.error.failed": "Something went wrong. Try again.",
  "common.error.unimplemented": "Not available yet.",
  "common.error.unknown": "Something went wrong.",

  "common.hotkey.taken": "This shortcut is used by another app.",
  "common.hotkey.invalid": "This shortcut can't be used.",

  "common.kind.text": "Text",
  "common.kind.link": "Link",
  "common.kind.code": "Code",
  "common.kind.color": "Color",
  "common.kind.contact": "Contact",
  "common.kind.image": "Image",
  "common.kind.file": "File",

  "common.size.bytes": "{n} B",
  "common.size.kb": "{n} KB",
  "common.size.mb": "{n} MB",
  "common.size.gb": "{n} GB",

  // --- picker.* (W1-E) --- (W0 seed for the stub shelf; W1-E owns and may rename)
  "picker.search": "Search",
  "picker.empty.title": "Copy something — it will appear here",
  "picker.empty.desc": "Alt + V opens Liqlip from any window.",
  "picker.error.title": "Couldn't load the history",

  // --- settings.* (W1-F) --- (W0 seed for the stub window; W1-F owns and may rename)
  "settings.nav.label": "Settings sections",
  "settings.section.general": "General",
  "settings.section.picker": "Shelf",
  "settings.section.hotkeys": "Hotkeys",
  "settings.section.storage": "Storage",
  "settings.section.privacy": "Privacy",
  "settings.section.snippets": "Snippets",
  "settings.section.ocr": "Text recognition",
  "settings.section.import": "Import and export",
  "settings.section.about": "About Liqlip",

  // --- onboarding.* (W1-F) ---

  // --- hud.* (W1-D) ---
  // {keys} renders as a key chip (hud.pasteKeys).
  "hud.copied": "Copied — press {keys}",
  "hud.elevated": "Administrator window — press {keys}",
  "hud.stackEmpty": "Paste stack is empty",
  "hud.paused": "Recording paused",
  "hud.resumed": "Recording resumed",
  "hud.skipNext": "The next copy won't be saved",
  "hud.pasteKeys": "Ctrl+V",
} as const;

export type TranslationKey = keyof typeof en;
