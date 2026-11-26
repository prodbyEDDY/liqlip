// Labels, glyphs and keys of the toolbar and the properties bar (one table, read by Toolbar,
// PropsBar and Cheatsheet, so a tooltip, the cheatsheet and the dispatcher never disagree).
import type { TranslationKey } from "$lib/i18n/index.svelte";
import type { ToolId } from "./doc";
import type { ArrowStyle, RedactMode, SpotShape, TextStyle } from "./defaults";
import type { GlyphName } from "./glyphs/glyphs";

export interface ToolMeta {
  id: ToolId;
  glyph: GlyphName;
  /** the key shown in the tooltip (physical key; RU layout uses the same key) */
  key: string;
  label: TranslationKey;
  /** a group boundary follows this tool (a little more space) */
  gap?: boolean;
}

export const TOOL_META: readonly ToolMeta[] = [
  { id: "select", glyph: "select", key: "V", label: "editor.tool.select", gap: true },
  { id: "arrow", glyph: "arrow", key: "A", label: "editor.tool.arrow" },
  { id: "line", glyph: "line", key: "L", label: "editor.tool.line" },
  { id: "rect", glyph: "rect", key: "R", label: "editor.tool.rect" },
  { id: "ellipse", glyph: "ellipse", key: "E", label: "editor.tool.ellipse", gap: true },
  { id: "pen", glyph: "pen", key: "P", label: "editor.tool.pen" },
  { id: "marker", glyph: "marker", key: "H", label: "editor.tool.marker", gap: true },
  { id: "text", glyph: "text", key: "T", label: "editor.tool.text" },
  { id: "counter", glyph: "counter", key: "N", label: "editor.tool.counter", gap: true },
  { id: "redact", glyph: "redact", key: "B", label: "editor.tool.redact" },
  { id: "spotlight", glyph: "spotlight", key: "S", label: "editor.tool.spotlight", gap: true },
  { id: "crop", glyph: "crop", key: "C", label: "editor.tool.crop" },
  { id: "eyedropper", glyph: "eyedropper", key: "I", label: "editor.tool.eyedropper" },
];

export const ARROW_STYLES: readonly { v: ArrowStyle; glyph: GlyphName; label: TranslationKey }[] = [
  { v: "straight", glyph: "arrowStraight", label: "editor.arrow.straight" },
  { v: "curved", glyph: "arrowCurved", label: "editor.arrow.curved" },
  { v: "double", glyph: "arrowDouble", label: "editor.arrow.double" },
];

export const TEXT_STYLES: readonly { v: TextStyle; glyph: GlyphName; label: TranslationKey }[] = [
  { v: "plain", glyph: "textPlain", label: "editor.textStyle.plain" },
  { v: "badge", glyph: "textBadge", label: "editor.textStyle.badge" },
];

export const REDACT_MODES: readonly { v: RedactMode; glyph: GlyphName; label: TranslationKey }[] = [
  { v: "solid", glyph: "redactSolid", label: "editor.redact.solid" },
  { v: "pixelate", glyph: "redactPixelate", label: "editor.redact.pixelate" },
  { v: "blur", glyph: "redactBlur", label: "editor.redact.blur" },
];

export const SPOT_SHAPES: readonly { v: SpotShape; glyph: GlyphName; label: TranslationKey }[] = [
  { v: "rect", glyph: "shapeRect", label: "editor.spot.rect" },
  { v: "ellipse", glyph: "shapeEllipse", label: "editor.spot.ellipse" },
];

/** The cheatsheet (`?`): groups of [keys, label]. `window` rows only show in the editor window. */
export interface KeyRow {
  keys: string[];
  label: TranslationKey;
  window?: boolean;
}

export const KEY_GROUPS: readonly { title: TranslationKey; rows: readonly KeyRow[] }[] = [
  {
    title: "editor.keys.group.draw",
    rows: [
      { keys: ["Shift"], label: "editor.keys.shift" },
      { keys: ["Alt"], label: "editor.keys.alt" },
      { keys: ["Ctrl"], label: "editor.keys.ctrl" },
      { keys: ["Shift", "B"], label: "editor.keys.variant" },
      { keys: ["F"], label: "editor.keys.fill" },
      { keys: ["1…5"], label: "editor.keys.size" },
      { keys: ["[", "]"], label: "editor.keys.sizeStep" },
      { keys: ["Shift", "1…8"], label: "editor.keys.color" },
    ],
  },
  {
    title: "editor.keys.group.edit",
    rows: [
      { keys: ["Ctrl", "Z"], label: "editor.keys.undo" },
      { keys: ["Ctrl", "Shift", "Z"], label: "editor.keys.redo" },
      { keys: ["Ctrl", "D"], label: "editor.keys.duplicate" },
      { keys: ["Alt", "↔"], label: "editor.keys.altDrag" },
      { keys: ["←↑→↓"], label: "editor.keys.nudge" },
      { keys: ["Delete"], label: "editor.keys.delete" },
      { keys: ["Ctrl", "A"], label: "editor.keys.selectAll" },
      { keys: ["Ctrl", "]"], label: "editor.keys.forward" },
      { keys: ["Ctrl", "["], label: "editor.keys.backward" },
      { keys: ["Ctrl", "Enter"], label: "editor.keys.textDone" },
    ],
  },
  {
    title: "editor.keys.group.view",
    rows: [
      { keys: ["Space"], label: "editor.keys.pan", window: true },
      { keys: ["Ctrl", "⊙"], label: "editor.keys.zoom", window: true },
      { keys: ["Ctrl", "0"], label: "editor.keys.fit", window: true },
      { keys: ["Ctrl", "1"], label: "editor.keys.actual", window: true },
      { keys: ["Enter"], label: "editor.keys.done" },
      { keys: ["Ctrl", "C"], label: "editor.keys.copy" },
      { keys: ["Ctrl", "Shift", "S"], label: "editor.keys.saveAs" },
      { keys: ["Ctrl", "P"], label: "editor.keys.pin" },
      { keys: ["Ctrl", "V"], label: "editor.keys.paste" },
      { keys: ["Esc"], label: "editor.keys.esc" },
      { keys: ["?"], label: "editor.keys.help" },
    ],
  },
];
