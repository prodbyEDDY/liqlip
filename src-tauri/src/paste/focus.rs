//! Paste target (ARCHITECTURE §6 "Цель") — owner: **W1-C**.

use crate::model::TargetInfo;
use tauri::AppHandle;

/// Called by the shelf (`picker::summon`, W1-D) BEFORE it shows itself: remember the
/// foreground window as the paste target and describe it. Own windows, the desktop and
/// the shell tray are rejected → `None`, and the previously remembered target is NOT
/// overwritten with nothing.
#[allow(dead_code)] // W1-D calls it
pub fn capture_target(app: &AppHandle) -> Option<TargetInfo> {
    let _ = app; // W1-C
    None
}

/// The remembered target, re-validated (window still alive). `picker_target` and the
/// footer hint use it.
pub fn current_target(app: &AppHandle) -> Option<TargetInfo> {
    let _ = app; // W1-C
    None
}
