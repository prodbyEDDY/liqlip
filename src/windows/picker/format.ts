// Display helpers for cards — port of Liquid Tools `shotlibrary/format.ts` (size readout,
// re-validated colour value) plus what the Liqlip cards need: URL host+path, relative time,
// plural category. No i18n here: callers localize with the returned unit / category.
// Pure — node-tested (tests/pickerFormat).
import type { Clip } from "$lib/ipc/types";

export type SizeUnit = "bytes" | "kb" | "mb" | "gb";

/** Human byte size → unit + number string ("12.4" KB); null/0 → null (no readout). */
export function formatSize(bytes: number | null | undefined): { unit: SizeUnit; n: string } | null {
  if (bytes == null || !(bytes > 0)) return null;
  if (bytes < 1024) return { unit: "bytes", n: String(Math.round(bytes)) };
  const steps: SizeUnit[] = ["kb", "mb", "gb"];
  let v = bytes / 1024;
  let i = 0;
  while (v >= 1024 && i < steps.length - 1) {
    v /= 1024;
    i++;
  }
  return { unit: steps[i], n: v.toFixed(v < 10 ? 1 : 0) };
}

/**
 * The record's CSS colour for swatches — re-validated here (the classifier already matched
 * it, but an old row or a hand-edited database must never inject arbitrary CSS), or null
 * when the record isn't a renderable colour.
 */
export function colorValue(clip: Pick<Clip, "kind" | "preview" | "meta">): string | null {
  if (clip.kind !== "color") return null;
  for (const v of [clip.preview.trim(), clip.meta.color?.hex ?? ""]) {
    if (/^#(?:[0-9a-f]{3}|[0-9a-f]{4}|[0-9a-f]{6}|[0-9a-f]{8})$/i.test(v)) return v;
    if (/^(?:rgb|rgba|hsl|hsla)\(\s*[\d.,%\s/-]+\)$/i.test(v)) return v;
  }
  return null;
}

/** "github.com/tauri-apps/tauri" — a URL without scheme, `www.` and trailing slash. */
export function hostPath(url: string): string {
  return url
    .trim()
    .replace(/^[a-z][a-z0-9+.-]*:\/\//i, "")
    .replace(/^www\./i, "")
    .replace(/\/$/, "");
}

/** Host of a link record (meta.host, else parsed from the URL/preview). */
export function linkHost(clip: Pick<Clip, "preview" | "meta">): string {
  if (clip.meta.host) return clip.meta.host;
  const raw = clip.meta.url ?? clip.preview;
  try {
    return new URL(/^[a-z][a-z0-9+.-]*:/i.test(raw) ? raw : `https://${raw}`).host || raw;
  } catch {
    return raw;
  }
}

/** Plural category for counted strings: `one | few | many` (English uses one/many). */
export function pluralCategory(n: number, locale: string): "one" | "few" | "many" {
  const c = new Intl.PluralRules(locale).select(n);
  return c === "one" ? "one" : c === "few" ? "few" : "many";
}

/** "5 minutes ago" / "5 минут назад" (Intl, locale-aware), for card tooltips. */
export function relativeTime(at: number, now: number, locale: string): string {
  const rtf = new Intl.RelativeTimeFormat(locale, { numeric: "auto" });
  const s = Math.round((at - now) / 1000);
  const abs = Math.abs(s);
  if (abs < 45) return rtf.format(0, "second");
  if (abs < 45 * 60) return rtf.format(Math.round(s / 60), "minute");
  if (abs < 22 * 3600) return rtf.format(Math.round(s / 3600), "hour");
  if (abs < 6 * 86400) return rtf.format(Math.round(s / 86400), "day");
  return new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeStyle: "short" }).format(at);
}

/** Clock time ("14:30") for "paused until …". */
export function clockTime(at: number, locale: string): string {
  return new Intl.DateTimeFormat(locale, { hour: "2-digit", minute: "2-digit" }).format(at);
}
