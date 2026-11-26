// Tool defaults (SPEC §9.3 rule 4): every drawing tool remembers its own colour, size and
// style between launches (`localStorage["liqlip.editor.tools"]`), plus the three most recent
// custom colours. Rule 4 writes here after a property change (the just-created object or the
// selection also changes — see interaction.ts). Pure; storage is injectable for node tests.
import type { Kind, Size } from "./doc";

export type ArrowStyle = "straight" | "curved" | "double";
export type TextStyle = "plain" | "badge";
export type RedactMode = "solid" | "pixelate" | "blur";
export type SpotShape = "rect" | "ellipse";

/** Every style knob a tool may use (each tool reads the ones it has). */
export interface ToolStyle {
  color: string;
  size: Size;
  fill: boolean;
  round: boolean;
  arrowStyle: ArrowStyle;
  textStyle: TextStyle;
  redactMode: RedactMode;
  spotShape: SpotShape;
}

export type StyleTool = "arrow" | "line" | "rect" | "ellipse" | "pen" | "marker" | "text" | "counter" | "redact" | "spotlight";
export const STYLE_TOOLS: readonly StyleTool[] = ["arrow", "line", "rect", "ellipse", "pen", "marker", "text", "counter", "redact", "spotlight"];

export interface Defaults {
  tools: Record<StyleTool, ToolStyle>;
  /** up to 3 custom colours, newest first */
  recent: string[];
}

/** The 8 presets (SPEC §9.3): red · orange · yellow · green · blue · purple · black · white. */
export const PRESETS = ["#ff3b30", "#ff9500", "#ffcc00", "#34c759", "#007aff", "#af52de", "#000000", "#ffffff"] as const;
export const PRESET_KEYS = ["red", "orange", "yellow", "green", "blue", "purple", "black", "white"] as const;

export const STORAGE_KEY = "liqlip.editor.tools";

const BASE: ToolStyle = {
  color: PRESETS[0],
  size: 3,
  fill: false,
  round: true,
  arrowStyle: "straight",
  textStyle: "plain",
  redactMode: "solid",
  spotShape: "rect",
};

export function factoryDefaults(): Defaults {
  const tools = {} as Record<StyleTool, ToolStyle>;
  for (const t of STYLE_TOOLS) tools[t] = { ...BASE };
  tools.marker.color = PRESETS[2];
  tools.redact.color = PRESETS[6];
  return { tools, recent: [] };
}

/** The style tool that owns objects of `kind` (`image` has none). */
export function toolOfKind(kind: Kind): StyleTool | null {
  return kind === "image" ? null : kind;
}

const HEX = /^#[0-9a-f]{6}$/i;

function readStyle(raw: unknown, base: ToolStyle): ToolStyle {
  const r = (raw && typeof raw === "object" ? raw : {}) as Partial<Record<keyof ToolStyle, unknown>>;
  const pick = <T>(v: unknown, ok: readonly T[], fb: T): T => (ok.includes(v as T) ? (v as T) : fb);
  return {
    color: typeof r.color === "string" && HEX.test(r.color) ? r.color.toLowerCase() : base.color,
    size: pick<Size>(r.size, [1, 2, 3, 4, 5], base.size),
    fill: typeof r.fill === "boolean" ? r.fill : base.fill,
    round: typeof r.round === "boolean" ? r.round : base.round,
    arrowStyle: pick<ArrowStyle>(r.arrowStyle, ["straight", "curved", "double"], base.arrowStyle),
    textStyle: pick<TextStyle>(r.textStyle, ["plain", "badge"], base.textStyle),
    redactMode: pick<RedactMode>(r.redactMode, ["solid", "pixelate", "blur"], base.redactMode),
    spotShape: pick<SpotShape>(r.spotShape, ["rect", "ellipse"], base.spotShape),
  };
}

type Store = Pick<Storage, "getItem" | "setItem" | "removeItem">;
function store(s?: Store | null): Store | null {
  if (s !== undefined) return s;
  try {
    return typeof localStorage === "undefined" ? null : localStorage;
  } catch {
    return null;
  }
}

/** Saved defaults over the factory ones (a broken or foreign value falls back field by field). */
export function loadDefaults(storage?: Store | null): Defaults {
  const out = factoryDefaults();
  const s = store(storage);
  if (!s) return out;
  try {
    const raw = JSON.parse(s.getItem(STORAGE_KEY) ?? "null") as { tools?: Record<string, unknown>; recent?: unknown } | null;
    if (!raw || typeof raw !== "object") return out;
    for (const t of STYLE_TOOLS) out.tools[t] = readStyle(raw.tools?.[t], out.tools[t]);
    if (Array.isArray(raw.recent)) out.recent = raw.recent.filter((c): c is string => typeof c === "string" && HEX.test(c)).slice(0, 3);
  } catch {
    /* unreadable → factory defaults */
  }
  return out;
}

export function saveDefaults(d: Defaults, storage?: Store | null): void {
  const s = store(storage);
  try {
    s?.setItem(STORAGE_KEY, JSON.stringify(d));
  } catch {
    /* quota / private mode — defaults just won't persist */
  }
}

/** "Reset tool defaults" (Settings → Screenshots → Editor, W6-F). */
export function resetDefaults(storage?: Store | null): void {
  try {
    store(storage)?.removeItem(STORAGE_KEY);
  } catch {
    /* ignore */
  }
}

/** A custom colour joins the recent list (presets don't; newest first; 3 at most). */
export function pushRecent(recent: readonly string[], color: string): string[] {
  const c = color.toLowerCase();
  if ((PRESETS as readonly string[]).includes(c)) return recent.slice();
  return [c, ...recent.filter((x) => x !== c)].slice(0, 3);
}
