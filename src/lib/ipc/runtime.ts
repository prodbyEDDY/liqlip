/** True inside the Tauri webview; false in a plain browser (`pnpm dev`), where the IPC
 *  layer falls back to the in-memory mock (`./mock`). */
export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}
