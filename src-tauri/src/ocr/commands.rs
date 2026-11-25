//! `ocr` commands (ARCHITECTURE §9.6, §5a) — owner: W2-A.
//! JS-visible signatures are the W0 contract. Every command runs on the blocking pool
//! (WinRT and SQLite block); the logic lives in `ocr/mod.rs`.
//! Errors: `Err("<code>: …")` with code `not_found | busy | invalid | failed`.

use crate::model::OcrStatus;
use tauri::AppHandle;

async fn blocking<T, F>(app: &AppHandle, f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&AppHandle) -> Result<T, String> + Send + 'static,
{
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || f(&app)).await.map_err(|e| format!("failed: {e}"))?
}

/// OCR availability, installed languages (BCP-47 tags, fresh each call), queue length.
/// `available: false` + `languages: []` = no OCR language pack; `Err("failed: …")` = WinRT
/// itself failed.
#[tauri::command]
pub async fn ocr_status(app: AppHandle) -> Result<OcrStatus, String> {
    blocking(&app, super::status).await
}

/// Re-run OCR on every image in the background; progress `ocr:progress { done, total }`
/// (first event `{0, total}` right away, last one `done == total`; `{0, 0}` when there are no
/// images). A second call during a reindex does not restart it. `invalid:` when OCR is off,
/// `failed:` when no OCR language is installed.
#[tauri::command]
pub async fn ocr_reindex(app: AppHandle) -> Result<(), String> {
    blocking(&app, super::reindex).await
}

/// Recognised text of one image record (`""` = the image has no text). `done` → from the
/// database at once; otherwise recognised now, outside the queue (≤ ~10 s, then
/// `failed: … timed out` while the result is still stored when it arrives). `not_found:`
/// (missing/deleted), `invalid:` (not an image / OCR turned off), `failed:` (no language,
/// engine error), `busy:` (too many direct recognitions at once).
#[tauri::command]
pub async fn clip_ocr(app: AppHandle, id: i64) -> Result<String, String> {
    blocking(&app, move |app| super::clip_text(app, id)).await
}
