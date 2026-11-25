//! Launch-on-login, driven by `Settings.general.autostart` (ported from Liquid Tools
//! `autostart.rs`; the commands became plain functions — the contract has no autostart
//! commands, the toggle goes through `settings_set`).

use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;

/// Whether the app is registered to launch on login.
pub fn is_enabled(app: &AppHandle) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

/// Register / unregister launch on login.
pub fn set(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let manager = app.autolaunch();
    if enabled {
        manager.enable().map_err(|e| e.to_string())
    } else {
        manager.disable().map_err(|e| e.to_string())
    }
}

/// Startup reconciliation: make the OS registration match the setting. Release builds
/// only — a debug build would register the dev binary in the Run key on every launch.
pub fn sync_startup(app: &AppHandle, wanted: bool) {
    if cfg!(debug_assertions) {
        crate::logx::warn("autostart", "debug build: startup sync skipped");
        return;
    }
    match is_enabled(app) {
        Ok(current) if current == wanted => {}
        Ok(_) => {
            if let Err(e) = set(app, wanted) {
                crate::logx::warn("autostart", &format!("sync failed: {e}"));
            }
        }
        Err(e) => crate::logx::warn("autostart", &format!("query failed: {e}")),
    }
}
