//! `capture` commands (ARCHITECTURE §9.6) — owner: W1-B.
//! W0 stubs: every command is registered in `lib.rs` with its exact JS-visible
//! signature (argument names are the camelCase keys the frontend sends). Keep the
//! names, argument names/types and return types; add `State`/`AppHandle` params freely.
//! Errors: `Err("<code>: …")` with code `not_found | busy | invalid | failed`.

use crate::model::{CaptureStatus, RunningApp};
use tauri::AppHandle;

/// Recording state (safe read; "recording" until W1-B).
#[tauri::command]
pub async fn capture_status(app: AppHandle) -> Result<CaptureStatus, String> {
    Ok(crate::capture::status(&app))
}

/// Pause recording for N minutes (`null` = until resumed).
#[allow(unused_variables)] // W1-B: stub
#[tauri::command]
pub async fn capture_pause(_app: AppHandle, minutes: Option<i64>) -> Result<CaptureStatus, String> {
    Err("unimplemented: capture_pause".into())
}

/// Resume recording.
#[tauri::command]
pub async fn capture_resume(_app: AppHandle) -> Result<CaptureStatus, String> {
    Err("unimplemented: capture_resume".into())
}

/// Skip the next copy (one-shot).
#[tauri::command]
pub async fn capture_ignore_next(_app: AppHandle) -> Result<CaptureStatus, String> {
    Err("unimplemented: capture_ignore_next".into())
}

/// "Collect copies into the paste stack" mode.
#[allow(unused_variables)] // W1-B: stub
#[tauri::command]
pub async fn capture_set_collect(_app: AppHandle, value: bool) -> Result<CaptureStatus, String> {
    Err("unimplemented: capture_set_collect".into())
}

/// Running applications with windows (ignored-apps picker).
#[tauri::command]
pub async fn apps_running(_app: AppHandle) -> Result<Vec<RunningApp>, String> {
    Err("unimplemented: apps_running".into())
}
