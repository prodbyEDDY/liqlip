// Pure part of the i18n runtime (no runes) — covered by tests/i18n.test.mjs.

export type Vars = Record<string, string | number>;

/** Fill `{name}` slots in one pass. Values are inserted verbatim: a value is never searched
 *  for further slots (a folder called "{n}" stays "{n}") and `$&`, `$1`… in a value are not
 *  replacement patterns (they were with `String.replaceAll(slot, value)`). Slots without a
 *  value stay as written (`{keys}` is rendered by the HUD itself). */
export function interpolate(template: string, vars?: Vars): string {
  if (!vars) return template;
  return template.replace(/\{(\w+)\}/g, (slot, name: string) =>
    Object.prototype.hasOwnProperty.call(vars, name) ? String(vars[name]) : slot,
  );
}
