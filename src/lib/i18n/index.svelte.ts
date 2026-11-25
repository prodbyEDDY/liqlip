// i18n runtime (ported from Liquid Tools `i18n/index.svelte.ts`). The locale follows
// `Settings.general.language` (backend-authoritative, broadcast via `settings:changed`);
// "system" resolves to the Windows UI language as the webview reports it
// (`navigator.language`). A localStorage mirror paints the first frame correctly.
import { en, type TranslationKey } from "./en";
import { ru } from "./ru";
import type { Language } from "$lib/ipc/types";

export type { TranslationKey };
export type Locale = "en" | "ru";
export const LOCALES: Locale[] = ["en", "ru"];

const dictionaries: Record<Locale, Record<TranslationKey, string>> = { en, ru };
const STORAGE_KEY = "liqlip.locale";

function systemLocale(): Locale {
  if (typeof navigator !== "undefined") {
    const lang = navigator.language?.toLowerCase() ?? "";
    if (lang.startsWith("ru")) return "ru";
  }
  return "en";
}

/** Resolve the setting to a concrete locale. */
export function resolveLanguage(language: Language): Locale {
  return language === "system" ? systemLocale() : language;
}

function detectInitial(): Locale {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved === "en" || saved === "ru") return saved;
  } catch {
    /* localStorage may be unavailable */
  }
  return systemLocale();
}

// Module-level rune: any `t(...)` read in a template re-runs when this changes.
const initialLocale = detectInitial();
let current = $state<Locale>(initialLocale);

if (typeof document !== "undefined") {
  document.documentElement.lang = initialLocale;
}

export function getLocale(): Locale {
  return current;
}

export function setLocale(next: Locale): void {
  current = next;
  try {
    localStorage.setItem(STORAGE_KEY, next);
  } catch {
    /* ignore */
  }
  if (typeof document !== "undefined") {
    document.documentElement.lang = next;
  }
}

/** Apply `Settings.general.language` (called by `initAppearance`). */
export function applyLanguage(language: Language): void {
  const next = resolveLanguage(language);
  if (next !== current) setLocale(next);
}

type Vars = Record<string, string | number>;

/** Translate a key for the current locale, with optional `{name}` interpolation. */
export function t(key: TranslationKey, vars?: Vars): string {
  let str: string = dictionaries[current][key] ?? en[key] ?? key;
  if (vars) {
    for (const [k, v] of Object.entries(vars)) {
      str = str.replaceAll(`{${k}}`, String(v));
    }
  }
  return str;
}

/** Reactive accessor for places that need the raw locale value. */
export const locale = {
  get current(): Locale {
    return current;
  },
};
