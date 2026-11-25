//! HUD toast at the cursor (ARCHITECTURE §7 "HUD") — owner: **W1-D**.

use crate::model::HudKind;
use tauri::AppHandle;

/// Position the `hud` window at the cursor (its monitor's work area), show it WITHOUT
/// activation, emit `hud:show { kind, text }`, auto-hide after 1.6 s (a new show extends).
/// Callable from any thread; never blocks.
#[allow(dead_code)] // W1-C (paste flow), W1-D (hotkeys) call it
pub fn show(app: &AppHandle, kind: HudKind, text: Option<String>) {
    let _ = (app, kind, text); // W1-D
}
