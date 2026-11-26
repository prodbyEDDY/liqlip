// Browser mock router (owner: W1-E; W0 baseline): `commands.ts` calls `mockInvoke` when
// not running inside Tauri. Every §9.6 command has a handler (checked by
// tests/contract.test.mjs). Handlers throw "<code>: <detail>" strings like Rust commands.
//
// Files: bus.ts (events, flags, latency — coordinator) · seed.ts (data) · clips.ts (store,
// paste, stack, capture simulator) · settings.ts (W1-F) · system.ts (capture state, window,
// about, OCR/import) · shots.ts (screenshots, W5-A).
import { clipsHandlers, startLive, type Args } from "./clips";
import { settingsHandlers } from "./settings";
import { shotsHandlers } from "./shots";
import { systemHandlers } from "./system";
import { delay, fail } from "./bus";

const handlers: Record<string, (args: Args) => unknown> = {
  ...clipsHandlers,
  ...settingsHandlers,
  ...systemHandlers,
  ...shotsHandlers,
};

/** Command names the mock implements. */
export const MOCK_COMMANDS: readonly string[] = Object.keys(handlers);

/** `args` — named arguments, or the raw body of `shot_commit` / `shot_editor_commit` (a
 *  `Uint8Array`, which those handlers parse like Rust does). */
export async function mockInvoke<T>(cmd: string, args: Args | Uint8Array): Promise<T> {
  startLive();
  await delay();
  const handler = handlers[cmd];
  if (!handler) throw fail("unimplemented", cmd);
  // Structured clone mimics the JSON boundary: callers never share mock state.
  const result = await handler(structuredClone(args) as Args);
  return (result === undefined ? null : structuredClone(result)) as T;
}
