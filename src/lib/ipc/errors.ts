// Command errors are strings "<code>: <detail>" (ARCHITECTURE §9.6). The detail is for
// logs only; the UI shows a localized message chosen by the code.
import { t, type TranslationKey } from "$lib/i18n/index.svelte";

export type ErrorCode = "not_found" | "busy" | "invalid" | "failed" | "unimplemented" | "unknown";

const CODES: readonly ErrorCode[] = ["not_found", "busy", "invalid", "failed", "unimplemented"];

/** Machine code of a rejected command (`"unknown"` for anything unexpected). */
export function errorCode(e: unknown): ErrorCode {
  const raw = typeof e === "string" ? e : e instanceof Error ? e.message : String(e ?? "");
  const code = raw.split(":", 1)[0]?.trim() as ErrorCode;
  return CODES.includes(code) ? code : "unknown";
}

/** Localized, user-facing message for a rejected command. */
export function errorMessage(e: unknown): string {
  return t(`common.error.${errorCode(e)}` as TranslationKey);
}
