// Mock infrastructure shared by every mock module (coordinator file, W0): the in-page
// event bus, URL flags, simulated latency, deterministic randomness and string errors
// shaped like the Rust ones ("<code>: <detail>").
//
// URL flags (browser only):
//   ?mock-empty=1        empty database (first-run states)
//   ?mock-error=1        listing commands reject with "busy: …" (error states)
//   ?mock-live=0         no simulated captures (default: a new capture every ~9 s)
//   ?mock-size=5000      number of seeded records (default 300)
//   ?mock-target=chrome|none|elevated   the paste target the shelf "sees" (default chrome)
//   ?mock-unimpl=ocr,import             make those (W2) command groups reject "unimplemented"
//   ?mock-position=center|left|right|top|bottom   shelf position the picker mock lays out (W1-E, system.ts)
//   ?mock-row=1          centre position as a single row (W1-E, system.ts)

export type Unlisten = () => void;

type Handler = (payload: unknown) => void;
const handlers = new Map<string, Set<Handler>>();

/** Subscribe to a mock event; returns a disposer (same shape as Tauri's `listen`). */
export function mockListen<T>(event: string, cb: (payload: T) => void): Unlisten {
  let set = handlers.get(event);
  if (!set) {
    set = new Set();
    handlers.set(event, set);
  }
  const h = cb as Handler;
  set.add(h);
  return () => {
    set?.delete(h);
  };
}

/** Emit a mock event asynchronously (like IPC: never re-entrant into the caller). */
export function mockEmit<T>(event: string, payload: T): void {
  const set = handlers.get(event);
  if (!set || set.size === 0) return;
  const snapshot = structuredClone(payload);
  setTimeout(() => {
    for (const h of [...set]) h(snapshot);
  }, 0);
}

function param(name: string): string | null {
  if (typeof location === "undefined") return null;
  return new URLSearchParams(location.search).get(name);
}

const inBrowser = typeof window !== "undefined" && typeof location !== "undefined";

export const flags = {
  empty: param("mock-empty") === "1",
  error: param("mock-error") === "1",
  /** Off outside a browser (node tests must not keep a timer alive). */
  live: inBrowser && param("mock-live") !== "0",
  size: Math.max(0, Math.min(100_000, Number(param("mock-size") ?? 300) || 0)),
  target: (param("mock-target") ?? "chrome") as "chrome" | "none" | "elevated",
  unimpl: new Set((param("mock-unimpl") ?? "").split(",").filter(Boolean)),
  /** No artificial latency in node tests. */
  latency: inBrowser,
};

/** Simulated IPC latency, 30–80 ms. */
export function delay(): Promise<void> {
  if (!flags.latency) return Promise.resolve();
  return new Promise((r) => setTimeout(r, 30 + Math.random() * 50));
}

/** Reject like a Rust command: `throw fail("not_found", "clip 3")`. */
export function fail(code: "not_found" | "busy" | "invalid" | "failed" | "unimplemented", detail: string): string {
  return `${code}: ${detail}`;
}

/** Deterministic PRNG (mulberry32) so every page load sees the same seed. */
export function rng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

export function now(): number {
  return Date.now();
}
