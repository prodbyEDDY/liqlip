//! `picker` commands (ARCHITECTURE §9.6) — owner: W1-D.
//! W0 stubs: every command is registered in `lib.rs` with its exact JS-visible
//! signature (argument names are the camelCase keys the frontend sends). Keep the
//! names, argument names/types and return types; add `State`/`AppHandle` params freely.
//! Errors: `Err("<code>: …")` with code `not_found | busy | invalid | failed`.

use crate::model::{MonitorInfo, TargetInfo};
use tauri::AppHandle;

/// Close the shelf (Esc); `restoreFocus` returns focus to the target.
#[allow(unused_variables)] // W1-D: stub
#[tauri::command]
pub async fn picker_hide(_app: AppHandle, restore_focus: bool) -> Result<(), String> {
    Err("unimplemented: picker_hide".into())
}

/// Hover-opened shelf → focusable + active (click into search).
#[tauri::command]
pub async fn picker_make_interactive(_app: AppHandle) -> Result<(), String> {
    Err("unimplemented: picker_make_interactive".into())
}

/// The window the shelf will paste into (safe read; `null` until W1-C).
#[tauri::command]
pub async fn picker_target(app: AppHandle) -> Result<Option<TargetInfo>, String> {
    Ok(crate::paste::focus::current_target(&app))
}

/// Show the settings window (+ `settings:navigate`).
#[allow(unused_variables)] // W1-D: stub
#[tauri::command]
pub async fn settings_open(_app: AppHandle, section: Option<String>) -> Result<(), String> {
    Err("unimplemented: settings_open".into())
}

/// Monitors for the "Monitor" setting.
#[tauri::command]
pub async fn monitors_list(_app: AppHandle) -> Result<Vec<MonitorInfo>, String> {
    Err("unimplemented: monitors_list".into())
}
