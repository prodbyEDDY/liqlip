//! OCR on images — Windows.Media.Ocr, background queue, FTS (SPEC §3.12). Owner: **W2-A**
//! (port of Liquid Tools `ocr.rs`, `git show 0295998:src-tauri/src/ocr.rs`).

pub mod commands;

use tauri::AppHandle;

/// `setup()` step 5: start the background OCR queue (W2-A).
pub fn start(app: &AppHandle) {
    let _ = app; // W2-A
}
