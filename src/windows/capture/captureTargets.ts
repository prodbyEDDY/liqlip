// Window picking of the capture overlay (ARCHITECTURE §18.7; W6-D). Ported as is from Liquid
// Tools `src/windows/capture/captureTargets.ts` (research §5.1, §6.3), typed on the contract's
// `WinRect` (`session.windows`: z-order, first = top-most; physical px of the monitor frame).
// Pure — node-tested (tests/captureTargets.test.mjs, the LT tests ported).

/** A window rectangle with its nested surfaces (child HWNDs: a browser page, a panel). */
export interface CaptureTarget {
  x: number;
  y: number;
  w: number;
  h: number;
  children?: CaptureTarget[];
}

/**
 * The window under a frame point. Coordinates stay physical until the last conversion.
 * Descends only inside the first (top-most) window that contains the point — never picks a
 * smaller window behind it. `wholeWindow` (Alt held) keeps the outer window. Right and bottom
 * edges are exclusive.
 */
export function targetAt<T extends CaptureTarget>(targets: readonly T[], x: number, y: number, wholeWindow = false): T | null {
  const root = targets.find((r) => x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h);
  if (!root || wholeWindow) return root ?? null;
  return (targetAt((root.children ?? []) as T[], x, y) as T | null) ?? root;
}
