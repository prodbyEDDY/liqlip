//! Paste — target/focus, UIPI, clipboard writers, SendInput, paste stack, open/reveal,
//! drag-out (ARCHITECTURE §6). Owner: **W1-C**. W0 declares the inter-module API (§15);
//! W1-C adds `uipi.rs`, `writer.rs`, `input.rs`, `dragout.rs`, `open.rs` here.

pub mod commands;
pub mod focus;
pub mod stack;

use tauri::AppHandle;

/// `setup()` step 5: create the hidden clipboard-owner window / worker if needed.
pub fn init(app: &AppHandle) {
    let _ = app; // W1-C
}
