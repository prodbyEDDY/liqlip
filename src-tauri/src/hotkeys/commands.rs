//! `hotkeys` commands (ARCHITECTURE §9.6) — owner: W1-D.
//! W0 stubs: every command is registered in `lib.rs` with its exact JS-visible
//! signature (argument names are the camelCase keys the frontend sends). Keep the
//! names, argument names/types and return types; add `State`/`AppHandle` params freely.
//! Errors: `Err("<code>: …")` with code `not_found | busy | invalid | failed`.

use tauri::AppHandle;

/// Start native chord recording (result via `hotkeys:captured`).
#[tauri::command]
pub async fn hotkey_record_start(_app: AppHandle) -> Result<(), String> {
    Err("unimplemented: hotkey_record_start".into())
}

/// Stop native chord recording.
#[tauri::command]
pub async fn hotkey_record_stop(_app: AppHandle) -> Result<(), String> {
    Err("unimplemented: hotkey_record_stop".into())
}
