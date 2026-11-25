// Browser mock router (owner: W1-E; W0 baseline): `commands.ts` calls `mockInvoke` when
// not running inside Tauri. Every §9.6 command has a handler (checked by
// tests/contract.test.mjs). Handlers throw "<code>: <detail>" strings like Rust commands.
//
// Files: bus.ts (events, flags, latency — coordinator) · seed.ts (data) · clips.ts (store,
// paste, stack, capture simulator) · settings.ts (W1-F) · system.ts (capture state, window,
// about, OCR/import).
import { clipsHandlers, startLive, type Args } from "./clips";
import { settingsHandlers } from "./settings";
import { systemHandlers } from "./system";
import { delay, fail } from "./bus";

const handlers: Record<string, (args: Args) => unknown> = {
  ...clipsHandlers,
  ...settingsHandlers,
  ...systemHandlers,
};

/** Command names the mock implements. */
export const MOCK_COMMANDS: readonly string[] = Object.keys(handlers);

export async function mockInvoke<T>(cmd: string, args: Args): Promise<T> {
  startLive();
  await delay();
  const handler = handlers[cmd];
  if (!handler) throw fail("unimplemented", cmd);
  // Structured clone mimics the JSON boundary: callers never share mock state.
  const result = await handler(structuredClone(args));
  return (result === undefined ? null : structuredClone(result)) as T;
}
