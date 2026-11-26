//! Capture sessions (owner: W6-A; ARCHITECTURE §18.3). W5-A stub: the signatures below are
//! what `screenshot/mod.rs` delegates to; every action answers `unimplemented: <command>`.
//!
//! To do (W6-A): `trigger(mode)` on a short thread (hide the shelf + `DwmFlush`, hide the
//! QAO, `paste::focus::capture_target`), freeze every monitor (`grab.rs`), window tree of the
//! cursor monitor (`targets.rs`), place + show the pre-created `capture` window
//! (position → size → position), `emit shot:session`; follow the cursor while the stage is
//! `select`; the delay countdown (`hud:show countdown`); `screen` / `window` / `repeat` straight
//! to `commit.rs`; ignore a trigger while a session is open; cancel → hide `capture`, give the
//! focus back, `shot:end`, free the frames.

use crate::model::{ShotMode, ShotSession, ShotStage};
use tauri::AppHandle;

/// Everything a session holds (`ScreenshotState.capture`): the session, its per-monitor BMP
/// frames (served by `frame.rs`), the last region `{ monitorId, rect }`, the focus target.
/// W6-A adds the fields.
#[derive(Default)]
pub(crate) struct SessionState {}

/// `setup()` step 5 (via `screenshot::init`): start-up work of the capture side — e.g. clear
/// `%TEMP%\Liqlip\shots\` (§18.4), subscribe to `settings.screenshot`. No-op until W6-A.
pub(crate) fn init(_app: &AppHandle) {}

/// `shot_capture { mode, delay? }` (also the hotkeys, the tray, `liqlip --capture`).
pub(crate) fn capture(_app: &AppHandle, _mode: ShotMode, _delay: Option<u32>) -> Result<(), String> {
    Err("unimplemented: shot_capture".into())
}

/// `shot_session` — the open session, `None` when there is none.
pub(crate) fn current(_app: &AppHandle) -> Result<Option<ShotSession>, String> {
    Err("unimplemented: shot_session".into())
}

/// `shot_stage { sessionId, stage }` — the overlay's stage (cursor following runs in `select`).
pub(crate) fn stage(_app: &AppHandle, _session_id: i64, _stage: ShotStage) -> Result<(), String> {
    Err("unimplemented: shot_stage".into())
}

/// `shot_cancel { sessionId }`.
pub(crate) fn cancel(_app: &AppHandle, _session_id: i64) -> Result<(), String> {
    Err("unimplemented: shot_cancel".into())
}

/// The pre-created `capture` window got `CloseRequested` (Alt+F4): cancel the open session.
/// `lib.rs` prevents the close and hides the window itself. No-op until W6-A.
pub(crate) fn on_capture_close(_app: &AppHandle) {}
