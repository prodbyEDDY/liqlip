//! Pinned pictures — windows `pin-<n>` (owner: W6-B; ARCHITECTURE §18.2, §18.8). W5-A stub.
//!
//! To do (W6-B): up to 6 windows built on demand (transparent, frameless, always-on-top,
//! skip-taskbar, focusable, `shadow(false)`; the picture in logical px ≤ 80 % of the monitor),
//! the 7th closes the oldest; destroyed on close. The window asks for its record by label
//! (`shot_pin_get`), so the commands below get the calling window.

use crate::model::PinPayload;
use tauri::{AppHandle, WebviewWindow};

/// Open pins (`ScreenshotState.pins`): label → record, creation order, the label counter.
#[derive(Default)]
pub(crate) struct PinState {}

/// `shot_pin { id }` — pin a record on the screen. Called from an `async` command, so building
/// the window here is the documented safe path.
pub(crate) fn open(_app: &AppHandle, _id: i64) -> Result<(), String> {
    Err("unimplemented: shot_pin".into())
}

/// `shot_pin_get` — the record of the calling `pin-<n>` window.
pub(crate) fn payload(_app: &AppHandle, _win: &WebviewWindow) -> Result<Option<PinPayload>, String> {
    Err("unimplemented: shot_pin_get".into())
}

/// `shot_pin_resize { scale }` — resize the calling window around the cursor.
pub(crate) fn resize(_app: &AppHandle, _win: &WebviewWindow, _scale: f64) -> Result<(), String> {
    Err("unimplemented: shot_pin_resize".into())
}

/// `shot_pin_close` — close (destroy) the calling window.
pub(crate) fn close(_app: &AppHandle, _win: &WebviewWindow) -> Result<(), String> {
    Err("unimplemented: shot_pin_close".into())
}
