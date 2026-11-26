//! Screenshots (v1.1, ARCHITECTURE §18). Hub (W5-A, then the coordinator): the `#[tauri::command]`
//! layer of §18.10, the managed [`ScreenshotState`], the `liq` routes and the `capture`
//! window's close hook. Every command only parses its arguments and delegates to the module
//! that owns the work — agents fill those modules, not this file (§18.1):
//!
//! | Module | Owner | What |
//! |---|---|---|
//! | `grab` · `targets` · `session` · `frame` · `commit` · `files` · `sound` | W6-A (`sound`: W8-A) | freeze, window tree, sessions, frames over `liq`, commit, files, shutter click |
//! | `windows` · `qao` · `editor` · `pin` · `status` | W6-B | on-demand windows, QAO, editor and pin windows, PrtSc / hotkey status |
//! | `body` | hub | raw-body framing of the two commits (§18.4) |
//!
//! All commands are `async`: they never run on the main thread (a sync command does — Tauri
//! `ipc` docs), so a command that builds a window (`shot_edit`, `shot_pin`) or waits for a lock
//! held by a thread that waits for the main thread cannot deadlock the event loop.

mod body;
mod commit;
mod editor;
mod files;
mod frame;
mod grab;
mod pin;
mod qao;
mod session;
mod sound;
mod status;
mod targets;
pub(crate) mod windows;

use crate::model::{
    Clip, PinPayload, ShotAdded, ShotCommit, ShotCommitResult, ShotEditorCommit, ShotEditorPayload, ShotMode,
    ShotSession, ShotStage, ShotStatus, WindowsSettingsPage,
};
use parking_lot::Mutex;
use tauri::{AppHandle, WebviewWindow};

/// Managed state (`app.manage` in `setup()` step 5, BEFORE any window exists — a webview may
/// invoke a command as soon as it loads). One lock per zone; each zone defines its own type.
#[derive(Default)]
pub struct ScreenshotState {
    /// Session, per-monitor frames, last region (W6-A, `session.rs`).
    pub(crate) capture: Mutex<session::SessionState>,
    /// The QAO window side (W6-B, `qao.rs`).
    pub(crate) qao: Mutex<qao::QaoState>,
    /// The editor window side (W6-B, `editor.rs`).
    pub(crate) editor: Mutex<editor::EditorState>,
    /// Open pin windows (W6-B, `pin.rs`).
    pub(crate) pins: Mutex<pin::PinState>,
}

/// `setup()` step 5, after `app.manage(ScreenshotState::default())`: remember the main thread,
/// sweep the temp shots of the last run, follow the language in the editor title.
pub fn init(app: &AppHandle) {
    windows::init(app);
}

/// The screenshot folder: `settings.screenshot.folder`, else `<Pictures>\Liqlip`
/// (`open_path('shots')`, `pick_folder`'s start — the folder the commit writes into).
pub fn shot_folder(s: &crate::model::ScreenshotSettings) -> Result<std::path::PathBuf, String> {
    files::folder(s)
}

/// A second launch's arguments (single-instance callback in `lib.rs`): `liqlip --capture [mode]`
/// starts a capture and returns `true`; anything else is the caller's (it summons the shelf).
pub fn handle_args(app: &AppHandle, args: &[String]) -> bool {
    session::handle_args(app, args)
}

/// The pre-created `capture` window got `CloseRequested` (Alt+F4): cancel the open session.
/// `lib.rs` prevents the close and hides the window.
pub fn on_capture_close(app: &AppHandle) {
    session::on_capture_close(app);
}

/// `liq` paths served by this module (§18.3): `/shot/<sessionId>/<monitorIndex>.bmp` (frames),
/// `/base/<clipId>` (the editor's picture without annotations) and `/temp/<name>` (a QAO card
/// of a shot kept out of history, INT-5). Everything else is the store's (§9.8).
pub fn owns_uri(path: &str) -> bool {
    path.starts_with("/shot/") || path.starts_with("/base/") || path.starts_with("/temp/")
}

/// `(mime, bytes)` of an [`owns_uri`] path; `None` → 404. `lib.rs` builds the response with
/// `store::cache_control` (`no-store` for these paths) and ACAO `*`.
pub fn serve_uri(app: &AppHandle, path: &str) -> Option<(String, Vec<u8>)> {
    if let Some(name) = path.strip_prefix("/temp/") {
        return files::serve_temp(name).map(|png| ("image/png".to_string(), png));
    }
    frame::serve(app, path)
}

// ── Commands (§18.10; JS keys camelCase) ────────────────────────────────────

/// Start a capture: `region` / `text` open the overlay, `screen` / `window` / `repeat` commit
/// at once; `delay` (s) counts down first.
#[tauri::command]
pub async fn shot_capture(app: AppHandle, mode: ShotMode, delay: Option<u32>) -> Result<(), String> {
    session::capture(&app, mode, delay)
}

/// The open session (the overlay asks on load, it may have missed `shot:session`).
#[tauri::command]
pub async fn shot_session(app: AppHandle) -> Result<Option<ShotSession>, String> {
    session::current(&app)
}

#[tauri::command]
pub async fn shot_stage(app: AppHandle, session_id: i64, stage: ShotStage) -> Result<(), String> {
    session::stage(&app, session_id, stage)
}

/// Raw body (§18.4): `[u32 LE N][JSON ShotCommit][RGBA | nothing]` — `frameBody` in
/// `src/lib/ipc/frame.ts`.
#[tauri::command]
pub async fn shot_commit(app: AppHandle, request: tauri::ipc::Request<'_>) -> Result<ShotCommitResult, String> {
    let bytes = body::bytes(request.body())?;
    let (req, rgba) = body::parse::<ShotCommit>(&bytes)?;
    commit::commit(&app, req, rgba)
}

#[tauri::command]
pub async fn shot_cancel(app: AppHandle, session_id: i64) -> Result<(), String> {
    session::cancel(&app, session_id)
}

/// Open the editor window on a record.
#[tauri::command]
pub async fn shot_edit(app: AppHandle, id: i64) -> Result<(), String> {
    editor::open(&app, id)
}

#[tauri::command]
pub async fn shot_editor_get(app: AppHandle) -> Result<Option<ShotEditorPayload>, String> {
    editor::payload(&app)
}

#[tauri::command]
pub async fn shot_editor_switch(app: AppHandle, accept: bool) -> Result<(), String> {
    editor::switch(&app, accept)
}

/// Raw body, same framing as `shot_commit` with a `ShotEditorCommit` header → the record
/// (`null` — "Save as…" was cancelled; W8-A).
#[tauri::command]
pub async fn shot_editor_commit(app: AppHandle, request: tauri::ipc::Request<'_>) -> Result<Option<Clip>, String> {
    let bytes = body::bytes(request.body())?;
    let (req, rgba) = body::parse::<ShotEditorCommit>(&bytes)?;
    editor::commit(&app, req, rgba).await
}

/// Pin a record on the screen.
#[tauri::command]
pub async fn shot_pin(app: AppHandle, id: i64) -> Result<(), String> {
    pin::open(&app, id)
}

/// The record of the calling `pin-<n>` window.
#[tauri::command]
pub async fn shot_pin_get(app: AppHandle, window: WebviewWindow) -> Result<Option<PinPayload>, String> {
    pin::payload(&app, &window)
}

#[tauri::command]
pub async fn shot_pin_resize(app: AppHandle, window: WebviewWindow, scale: f64) -> Result<(), String> {
    pin::resize(&app, &window, scale)
}

#[tauri::command]
pub async fn shot_pin_close(app: AppHandle, window: WebviewWindow) -> Result<(), String> {
    pin::close(&app, &window)
}

/// Save to the screenshot folder → the path.
#[tauri::command]
pub async fn shot_save(app: AppHandle, id: i64) -> Result<String, String> {
    files::save(&app, id)
}

/// "Save as…" dialog → the path, `null` = cancelled. Called from the shelf, the shelf has hidden
/// itself before the dialog, so its own error toast would never be seen: a failure is shown on
/// the HUD instead (W8-A, `HudKind::SaveFailed`). Other callers (a pin) show their own toast.
#[tauri::command]
pub async fn shot_save_as(app: AppHandle, window: WebviewWindow, id: i64) -> Result<Option<String>, String> {
    let saved = files::save_as(app.clone(), id).await;
    if saved.is_err() && window.label() == crate::events::PICKER {
        crate::picker::hud::show(&app, crate::model::HudKind::SaveFailed, None);
    }
    saved
}

/// Explorer with the saved file selected.
#[tauri::command]
pub async fn shot_reveal(app: AppHandle, id: i64) -> Result<(), String> {
    files::reveal(&app, id)
}

/// A shot kept out of history (QAO card, `liq://temp/<name>`) onto the clipboard.
#[tauri::command]
pub async fn shot_temp_copy(name: String) -> Result<(), String> {
    files::temp_copy(&name)
}

/// "Save as…" for a shot kept out of history → the path, `null` = cancelled.
#[tauri::command]
pub async fn shot_temp_save_as(app: AppHandle, name: String) -> Result<Option<String>, String> {
    files::save_temp_as(app, name).await
}

/// The QAO page subscribed: the `shot:added` payloads that arrived before it did.
#[tauri::command]
pub async fn shots_pending(app: AppHandle) -> Result<Vec<ShotAdded>, String> {
    qao::pending(&app)
}

/// The QAO stack's height in logical px.
#[tauri::command]
pub async fn shots_hud_layout(app: AppHandle, height: f64) -> Result<(), String> {
    qao::layout(&app, height)
}

#[tauri::command]
pub async fn shots_hud_hide(app: AppHandle) -> Result<(), String> {
    qao::hide(&app)
}

#[tauri::command]
pub async fn shot_status(app: AppHandle) -> Result<ShotStatus, String> {
    status::status(&app)
}

/// `ms-settings:` page (`keyboard` = `ms-settings:easeofaccess-keyboard`).
#[tauri::command]
pub async fn open_windows_settings(app: AppHandle, page: WindowsSettingsPage) -> Result<(), String> {
    status::open_windows_settings(&app, page)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn liq_routes_and_their_cache_policy() {
        assert!(owns_uri("/shot/12/0.bmp"));
        assert!(owns_uri("/base/42"));
        assert!(owns_uri("/temp/shot-1-0.png"));
        for store_path in ["/thumb/1", "/media/1", "/icon/3", "/shots/1", "/basement", "/temporary", "/"] {
            assert!(!owns_uri(store_path), "{store_path} stays the store's");
        }
        // Frames and base pictures are screen / clipboard content: never in the WebView2 disk cache.
        assert_eq!(crate::store::cache_control("/shot/12/0.bmp"), "no-store");
        assert_eq!(crate::store::cache_control("/base/42"), "no-store");
        assert_eq!(crate::store::cache_control("/temp/shot-1-0.png"), "no-store");
    }
}
