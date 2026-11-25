//! `import` commands (ARCHITECTURE §9.6) — owner: W2-B.
//! W0 stubs: every command is registered in `lib.rs` with its exact JS-visible
//! signature (argument names are the camelCase keys the frontend sends). Keep the
//! names, argument names/types and return types; add `State`/`AppHandle` params freely.
//! Errors: `Err("<code>: …")` with code `not_found | busy | invalid | failed`.

use crate::model::{BackupMode, ImportProbe, ImportReport};
use tauri::AppHandle;

/// Look for a Liquid Tools clipboard database.
#[tauri::command]
pub async fn import_lt_probe(_app: AppHandle) -> Result<ImportProbe, String> {
    Err("unimplemented: import_lt_probe".into())
}

/// Import Liquid Tools history (idempotent; `import:progress`).
#[tauri::command]
pub async fn import_lt_run(_app: AppHandle) -> Result<ImportReport, String> {
    Err("unimplemented: import_lt_run".into())
}

/// Export a `.liqlip` backup; returns the path (`null` = cancelled).
#[tauri::command]
pub async fn backup_export(_app: AppHandle) -> Result<Option<String>, String> {
    Err("unimplemented: backup_export".into())
}

/// Restore a backup (replace or merge).
#[allow(unused_variables)] // W2-B: stub
#[tauri::command]
pub async fn backup_import(_app: AppHandle, mode: BackupMode) -> Result<ImportReport, String> {
    Err("unimplemented: backup_import".into())
}
