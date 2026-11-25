//! `ocr` commands (ARCHITECTURE §9.6) — owner: W2-A.
//! W0 stubs: every command is registered in `lib.rs` with its exact JS-visible
//! signature (argument names are the camelCase keys the frontend sends). Keep the
//! names, argument names/types and return types; add `State`/`AppHandle` params freely.
//! Errors: `Err("<code>: …")` with code `not_found | busy | invalid | failed`.

use crate::model::{OcrStatus};
use tauri::AppHandle;

/// OCR availability, installed languages, queue length.
#[tauri::command]
pub async fn ocr_status(_app: AppHandle) -> Result<OcrStatus, String> {
    Err("unimplemented: ocr_status".into())
}

/// Re-run OCR on every image in the background (`ocr:progress`).
#[tauri::command]
pub async fn ocr_reindex(_app: AppHandle) -> Result<(), String> {
    Err("unimplemented: ocr_reindex".into())
}

/// Recognized text of one image record.
#[allow(unused_variables)] // W2-A: stub
#[tauri::command]
pub async fn clip_ocr(_app: AppHandle, id: i64) -> Result<String, String> {
    Err("unimplemented: clip_ocr".into())
}
