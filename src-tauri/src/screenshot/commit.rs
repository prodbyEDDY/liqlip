//! Commit of a shot and of an editor edit (owner: W6-A; ARCHITECTURE §18.4, §18.9). W5-A stub.
//!
//! `screenshot/mod.rs` has already split the raw body (`body.rs`): the JSON header arrives
//! parsed, the pixels as a slice borrowed from the IPC body — `None` when the webview sent
//! none (no annotations → crop the frame here, the fast path). The slice lives as long as the
//! command call: copy it (`to_vec`) to hand it to another thread.
//!
//! To do (W6-A): clamp `rect` to the session frame (`clamp_crop` + tests), hide `capture` and
//! give the focus back first, encode on a worker, clipboard FIRST (`paste::copy_png`), then
//! `store::shots::insert_screenshot` (+ base picture and `shot_docs` when annotated), the file
//! (`files.rs`), `emit shot:added`; `action: 'text'` → OCR → `paste::copy_text`, HUD
//! `text_copied` / `text_empty`, no record.

use crate::model::{Clip, ShotCommit, ShotCommitResult, ShotEditorCommit};
use tauri::AppHandle;

/// `shot_commit` (raw body §18.4).
pub(crate) fn commit(_app: &AppHandle, _req: ShotCommit, _rgba: Option<&[u8]>) -> Result<ShotCommitResult, String> {
    Err("unimplemented: shot_commit".into())
}

/// `shot_editor_commit` (raw body, same framing): `done` → `store::shots::apply_edit` +
/// clipboard + rewrite `saved_path`; `copy` / `save_as` / `pin` leave the record as it is.
pub(crate) fn editor_commit(_app: &AppHandle, _req: ShotEditorCommit, _rgba: Option<&[u8]>) -> Result<Clip, String> {
    Err("unimplemented: shot_editor_commit".into())
}
