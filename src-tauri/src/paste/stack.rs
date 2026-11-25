//! Paste stack (SPEC §3.11, ARCHITECTURE §6 "Очередь вставки") — owner: **W1-C**.
//! Every mutation emits `stack:changed` (`events::stack_changed`) and calls
//! `crate::tray::refresh(app)` (the tray shows "Paste stack: N").

use crate::model::{PasteOutcome, Stack};
use tauri::AppHandle;

/// Append a record to the stack — capture's "collect copies" mode calls it after a
/// successful insert.
#[allow(dead_code)] // W1-B calls it
pub fn push(app: &AppHandle, clip_id: i64) -> Result<(), String> {
    let _ = (app, clip_id); // W1-C
    Err("unimplemented: paste::stack::push".into())
}

/// Current stack (empty until W1-C).
pub fn snapshot(app: &AppHandle) -> Stack {
    let _ = app; // W1-C
    Stack::default()
}

/// Number of items (tray label).
pub fn len(app: &AppHandle) -> usize {
    snapshot(app).items.len()
}

/// Paste the top item into the CURRENT foreground window (no shelf) and pop it; empty
/// stack → `hud::show(stack_empty)`. Blocking — the hotkey engine (W1-D) calls it on a
/// worker thread, `stack_paste_next` wraps it.
#[allow(dead_code)] // W1-D calls it
pub fn paste_next(app: &AppHandle) -> Result<PasteOutcome, String> {
    let _ = app; // W1-C
    Err("unimplemented: paste::stack::paste_next".into())
}
