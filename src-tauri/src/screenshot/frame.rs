//! Frames and base pictures over the `liq` scheme (owner: W6-A; ARCHITECTURE §18.3). W5-A stub.
//!
//! `lib.rs` routes `http://liq.localhost/shot/…` and `/base/…` here (`screenshot::owns_uri`);
//! the response gets `Cache-Control: no-store` (`store::cache_control`) and ACAO `*` (the
//! overlay draws the frame into a canvas: `<img crossorigin="anonymous">`).
//!
//! To do (W6-A): `/shot/<sessionId>/<monitorIndex>.bmp` → the session's BMP of that monitor
//! (`BITMAPFILEHEADER` + top-down `BITMAPINFOHEADER` + BGRA, one buffer per monitor, the
//! crop for the commit comes from the same buffer — research §5.1, `M.rs:725-777`);
//! `/base/<clipId>` → `media/base/<id>.png` if present, else `media/<id>.png`.

use tauri::AppHandle;

/// `(mime, bytes)` of a `/shot/…` or `/base/…` path; `None` → 404 (always, until W6-A).
pub(crate) fn serve(_app: &AppHandle, _path: &str) -> Option<(String, Vec<u8>)> {
    None
}
