//! `hotkeys` commands (ARCHITECTURE §9.6) — owner: W1-D.
//! Errors: `Err("<code>: …")` with code `not_found | busy | invalid | failed`.

use tauri::AppHandle;

/// Start native chord recording: global hotkeys are suspended until the recorder commits,
/// is cancelled (Esc → `combo: ""`), stopped, or times out (30 s → `combo: ""`). The result
/// arrives as `hotkeys:captured { combo }` in the settings window. `Err("failed: hook: …")`
/// when the keyboard hook cannot be installed — the webview recorder still records ordinary
/// chords (the hotkeys stay suspended until `hotkey_record_stop`).
#[tauri::command]
pub async fn hotkey_record_start(_app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(super::record_start)
        .await
        .map_err(|e| format!("failed: {e}"))?
}

/// Stop native chord recording (idempotent) and restore the global hotkeys.
#[tauri::command]
pub async fn hotkey_record_stop(_app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(super::record_stop)
        .await
        .map_err(|e| format!("failed: {e}"))
}
