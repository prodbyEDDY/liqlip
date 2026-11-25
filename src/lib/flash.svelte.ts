/** Transient toast state: one message at a time, auto-cleared. Each window
 *  renders its own markup; this only owns the message + timer.
 *  Ported 1:1 from Liquid Tools `src/lib/flash.svelte.ts`. */
export function createFlash(durationMs = 2400) {
  let message = $state<string | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;
  return {
    get message() {
      return message;
    },
    show(msg: string) {
      message = msg;
      clearTimeout(timer);
      timer = setTimeout(() => (message = null), durationMs);
    },
    clear() {
      clearTimeout(timer);
      message = null;
    },
  };
}
