// Ported from Liquid Tools `src/lib/motion/index.ts` (dictation-cap duration and the
// launcher-only `panelSlide` dropped). Mirrors the --ease-* / --dur-* tokens in app.css.
import { cubicOut } from "svelte/easing";
import type { TransitionConfig } from "svelte/transition";

/** CSS easing strings (BUILD-PROMPT §motion). Mirrored in app.css as --ease-*. */
export const easing = {
  /** Liquid slide with a pronounced overshoot — the signature panel motion. */
  liquid: "cubic-bezier(0.34, 1.24, 0.44, 1)",
  /** Sliding pills — a gentler overshoot (used many times/session, so restrained). */
  pill: "cubic-bezier(0.34, 1.12, 0.42, 1)",
  /** Soft decelerate for fades / content swaps. */
  outSoft: "cubic-bezier(0.22, 1, 0.36, 1)",
  /** Strong decelerate, no overshoot — emphasis without bounce. */
  emphasized: "cubic-bezier(0.2, 0, 0, 1)",
  /** Symmetric ease-in-out. */
  inOut: "cubic-bezier(0.65, 0, 0.35, 1)",
} as const;

/** State-motion durations. Keep transitions in the 150–250 ms band. */
export const duration = {
  fast: 150,
  base: 200,
  slow: 250,
  /** Sliding-pill travel. */
  pill: 240,
} as const;

/** True when the user asked the OS to reduce motion. */
export function prefersReducedMotion(): boolean {
  return (
    typeof matchMedia !== "undefined" &&
    matchMedia("(prefers-reduced-motion: reduce)").matches
  );
}

/** A duration that collapses to 0 when reduced motion is requested. */
export function motionDuration(ms: number): number {
  return prefersReducedMotion() ? 0 : ms;
}

/**
 * Directional crossfade for a panel that swaps content (e.g. Quick Look moving to
 * the next record). Fade + a small vertical translate + a hair of blur so the swap
 * reads as one liquid motion, never a hard cut. Use with stacked items so in/out
 * genuinely overlap:
 *   in:panelContent={{ dir: 1 }}  out:panelContent={{ dir: -1 }}
 * `dir` is the travel direction (enter rises into place, leave continues upward).
 * `u` is 1−t, so the effect is full at the far end of the transition and 0 at rest.
 */
export function panelContent(
  node: Element,
  {
    duration: d = duration.base,
    y = 8,
    dir = 1,
    blur = 1.4,
  }: { duration?: number; y?: number; dir?: number; blur?: number } = {},
): TransitionConfig {
  void node;
  if (prefersReducedMotion()) return { duration: 0 };
  return {
    duration: d,
    easing: cubicOut,
    css: (t, u) =>
      `opacity:${t};` +
      `transform:translateY(${(u * y * dir).toFixed(2)}px);` +
      `filter:blur(${(u * blur).toFixed(2)}px);`,
  };
}
