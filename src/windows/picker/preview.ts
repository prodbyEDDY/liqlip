// Card badge + file family icon + automatic title — port of Liquid Tools
// `shotlibrary/preview.ts` onto the Liqlip `Clip` (kind/meta from the store instead of
// re-deriving from strings). Pure — node-tested (tests/pickerFormat).
import type { Clip } from "$lib/ipc/types";
import type { IconName } from "$lib/icons";
import { hostPath } from "./format";

type BadgeClip = Pick<Clip, "kind" | "preview" | "meta" | "formats">;

/** Format badge of the meta row: `TXT · URL · CODE · HEX · RGB · PNG · PDF · ZIP · EMAIL · TEL …`. */
export function entryFormat(c: BadgeClip): string {
  switch (c.kind) {
    case "file": {
      const ext = c.meta.ext ?? c.preview.split("\n")[0].match(/\.([a-z0-9]{1,8})$/i)?.[1];
      return ext ? ext.toUpperCase() : "FILE";
    }
    case "image":
      return c.formats.includes("png") || c.formats.length === 0 ? "PNG" : c.formats[0].toUpperCase();
    case "contact":
      return (c.meta.contact ?? (c.preview.includes("@") ? "email" : "phone")) === "email" ? "EMAIL" : "TEL";
    case "color": {
      const v = c.preview.trim();
      return v.startsWith("#") ? "HEX" : (v.split("(")[0].replace(/a$/i, "").toUpperCase() || "HEX");
    }
    case "link":
      return "URL";
    case "code":
      return "CODE";
    default:
      return "TXT";
  }
}

/** Icon of a file's format family (archive, table, document, video, audio, image, code). */
export function fileIcon(format: string): IconName {
  if (/^(ZIP|RAR|7Z|TAR|GZ|BZ2|XZ)$/.test(format)) return "archive";
  if (/^(XLSX?|CSV|ODS|TSV)$/.test(format)) return "table";
  if (/^(PDF|DOCX?|ODT|RTF|TXT|MD|PPTX?|ODP)$/.test(format)) return "document";
  if (/^(MP4|MOV|AVI|WEBM|MKV)$/.test(format)) return "video";
  if (/^(MP3|WAV|FLAC|M4A|OGG)$/.test(format)) return "music";
  if (/^(PNG|JPE?G|GIF|SVG|WEBP|AVIF|BMP|HEIC|ICO|TIFF?)$/.test(format)) return "image";
  if (/^(JSON|JS|TS|PY|RS|HTML|CSS|XML|YAML|YML|SQL|SH|PS1|C|CPP|H|CS|GO|JAVA)$/.test(format)) return "code";
  if (format === "FILE") return "folder";
  return "file";
}

/** Base name of a Windows/POSIX path. */
export function baseName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}

/**
 * Title of the meta row when the user hasn't renamed the record: the source app for
 * text-like records (the preview already shows the content — Liquid Tools did the same for
 * text/code), host+path for links, dimensions for images. `null` → the caller shows the kind
 * label.
 */
export function autoTitle(c: Pick<Clip, "kind" | "preview" | "meta" | "width" | "height">, appName: string | null): string | null {
  switch (c.kind) {
    case "link":
      return hostPath(c.meta.url ?? c.preview) || null;
    case "image":
      return c.width && c.height ? `${c.width} × ${c.height}` : appName;
    default:
      return appName;
  }
}
