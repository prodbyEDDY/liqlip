//! Shelf window — state machine (summon / hover / exits), docking, `settings_open`,
//! monitors (ARCHITECTURE §7) and the HUD (`hud.rs`). Owner: **W1-D**. W0 declares the
//! inter-module API (§15).
//!
//! Windows are pre-created hidden in `lib.rs::setup` (labels `events::PICKER`,
//! `events::HUD`, `events::SETTINGS`); this module only shows / hides / moves them.
//! Note for W1-D: tao rewrites `GWL_EXSTYLE` from its own flags on every
//! show/hide/focusable change (tao `window_state.rs::apply_diff`), so a manually added
//! `WS_EX_TOOLWINDOW` (keep the shelf out of Alt+Tab) must be re-applied after them.

pub mod commands;
pub mod hud;

use tauri::AppHandle;

/// `setup()` after step 6 (windows exist): subscribe to `settings.picker.*`
/// (`settings::subscribe`), start the hover poller, emit the first `picker:layout`.
pub fn init(app: &AppHandle) {
    let _ = app; // W1-D
}

/// Open the shelf from the hotkey, the tray or a second instance (toggle: closes it when
/// open). Calls `paste::focus::capture_target` before showing. Must not block.
pub fn summon(app: &AppHandle) {
    let _ = app; // W1-D
    crate::logx::warn("picker", "summon: unimplemented (W1-D)");
}

/// Hide immediately, no exit animation, WITHOUT restoring focus (the paste flow restores
/// focus to the target itself). Blocking until the window is hidden.
#[allow(dead_code)] // W1-C calls it
pub fn hide_now(app: &AppHandle) {
    let _ = app; // W1-D
}

/// Show + activate the settings window, optionally on a section (`settings:navigate`).
/// Used by `settings_open`, the tray and the first-run onboarding (`setup()` step 10).
pub fn show_settings(app: &AppHandle, section: Option<&str>) {
    let _ = (app, section); // W1-D
    crate::logx::warn("picker", "show_settings: unimplemented (W1-D)");
}
