//! Quick-access overlay — the thumbnail stack after a shot, window `shots` (owner: W6-B;
//! ARCHITECTURE §18.2, §18.8). W5-A stub.
//!
//! To do (W6-B): build `shots` on the first shot (transparent, frameless, always-on-top,
//! skip-taskbar, `focusable(false)` = `WS_EX_NOACTIVATE`, `disable_drag_drop_handler`,
//! `WDA_EXCLUDEFROMCAPTURE`), then keep it hidden; place it in the configured corner of the
//! shot monitor's work area; show without activation; hide while a session selects; return
//! after the commit / cancel when the stack is not empty.

use tauri::AppHandle;

/// What the QAO window side remembers (`ScreenshotState.qao`): built yet, monitor, height.
#[derive(Default)]
pub(crate) struct QaoState {}

/// `shots_hud_layout { height }` — the stack's height in logical px (the window follows).
pub(crate) fn layout(_app: &AppHandle, _height: f64) -> Result<(), String> {
    Err("unimplemented: shots_hud_layout".into())
}

/// `shots_hud_hide` — the stack is empty (or a session hides it); the window keeps living.
pub(crate) fn hide(_app: &AppHandle) -> Result<(), String> {
    Err("unimplemented: shots_hud_hide".into())
}
