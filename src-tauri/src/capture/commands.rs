//! `capture` commands (ARCHITECTURE §9.6 / §9.9) — owner: W1-B. JS-visible names, argument
//! names and return types are the W0 contract. Every state change emits `capture:status`
//! and refreshes the tray (inside the module functions).
//! Errors: `Err("<code>: …")` with code `not_found | busy | invalid | failed`.

use crate::model::{CaptureStatus, RunningApp};
use tauri::AppHandle;

/// Recording state.
#[tauri::command]
pub async fn capture_status(app: AppHandle) -> Result<CaptureStatus, String> {
    Ok(crate::capture::status(&app))
}

/// Pause recording for N minutes (`null` = until resumed). `minutes` must be 1..=525600.
#[tauri::command]
pub async fn capture_pause(app: AppHandle, minutes: Option<i64>) -> Result<CaptureStatus, String> {
    crate::capture::pause(&app, minutes)
}

/// Resume recording.
#[tauri::command]
pub async fn capture_resume(app: AppHandle) -> Result<CaptureStatus, String> {
    crate::capture::resume(&app)
}

/// Skip the next copy (one-shot).
#[tauri::command]
pub async fn capture_ignore_next(app: AppHandle) -> Result<CaptureStatus, String> {
    crate::capture::ignore_next(&app)
}

/// "Collect copies into the paste stack" mode.
#[tauri::command]
pub async fn capture_set_collect(app: AppHandle, value: bool) -> Result<CaptureStatus, String> {
    crate::capture::set_collect(&app, value)
}

/// Running applications with windows (ignored-apps picker). Icon extraction blocks, so it
/// runs on the blocking pool (ARCHITECTURE §15 rule 4).
#[tauri::command]
pub async fn apps_running(_app: AppHandle) -> Result<Vec<RunningApp>, String> {
    tauri::async_runtime::spawn_blocking(crate::capture::running_apps)
        .await
        .map_err(|e| format!("failed: apps_running: {e}"))
}
