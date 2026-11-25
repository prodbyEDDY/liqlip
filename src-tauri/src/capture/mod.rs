//! Capture — clipboard listener, one-OpenClipboard snapshot, privacy markers, owner +
//! app icons, classification, secrets, pause / skip / collect, worker pipeline
//! (ARCHITECTURE §5). Owner: **W1-B**. W0 declares the inter-module API (§15).
//!
//! State changes (pause, skip, collect) must emit `capture:status` (`events::capture_status`)
//! and call `crate::tray::refresh(app)` so the tray icon/menu follow.

pub mod classify;
pub mod commands;

use crate::model::CaptureStatus;
use tauri::AppHandle;

/// `setup()` step 5: start the listener thread + worker (needs `Store` and settings
/// managed). Must return quickly.
pub fn start(app: &AppHandle) {
    let _ = app; // W1-B
}

/// Self-copy guard (§5): the paste writer calls this with `GetClipboardSequenceNumber()`
/// taken right after its own `CloseClipboard`; the listener ignores that update.
/// Callable from any thread without an `AppHandle`.
#[allow(dead_code)] // W1-C calls it
pub fn mark_own_write(seq: u32) {
    let _ = seq; // W1-B
}

/// Current recording state. Safe default until W1-B: recording, nothing pending.
pub fn status(app: &AppHandle) -> CaptureStatus {
    let _ = app; // W1-B
    CaptureStatus::default()
}

/// Pause for `minutes` (`None` = until resumed). Tray, hotkey and `capture_pause` use it.
pub fn pause(app: &AppHandle, minutes: Option<i64>) -> Result<CaptureStatus, String> {
    let _ = (app, minutes); // W1-B
    Err("unimplemented: capture::pause".into())
}

/// Resume recording.
pub fn resume(app: &AppHandle) -> Result<CaptureStatus, String> {
    let _ = app; // W1-B
    Err("unimplemented: capture::resume".into())
}

/// Pause until resumed if recording, resume if paused (hotkey `PauseToggle`).
#[allow(dead_code)] // W1-D calls it from the hotkey engine
pub fn toggle_pause(app: &AppHandle) -> Result<CaptureStatus, String> {
    let _ = app; // W1-B
    Err("unimplemented: capture::toggle_pause".into())
}

/// Skip the next clipboard change (one-shot). Tray, hotkey and `capture_ignore_next`.
pub fn ignore_next(app: &AppHandle) -> Result<CaptureStatus, String> {
    let _ = app; // W1-B
    Err("unimplemented: capture::ignore_next".into())
}

/// `data:image/png;base64,…` icon of an executable from the capture icon cache
/// (extracted once per exe). Used by paste (`TargetInfo.iconUrl`) and `apps_running`.
/// `None` when unavailable.
#[allow(dead_code)] // W1-C calls it
pub fn app_icon_data_url(exe: &str) -> Option<String> {
    let _ = exe; // W1-B
    None
}
