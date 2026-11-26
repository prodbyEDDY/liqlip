//! The editor window `editor` (owner: W6-B; ARCHITECTURE §18.2, §18.9). W5-A stub.
//!
//! To do (W6-B): one window, built on demand (an ordinary window: system title bar, taskbar,
//! min 640×420, fit the picture into 80 % of the work area, title "Name — Liqlip" from
//! `i18n_native`), destroyed on close; a second `shot_edit` while it is open switches the
//! record (`shot:editor`; the window asks first when it has unsaved changes →
//! `shot_editor_switch`). The commit itself is `commit::editor_commit` (W6-A).

use crate::model::{ShotEditorAction, ShotEditorPayload};
use tauri::AppHandle;

/// What the editor window side remembers (`ScreenshotState.editor`): the record shown, a
/// pending switch.
#[derive(Default)]
pub(crate) struct EditorState {}

/// `shot_edit { id }` — open the editor on a record (screenshot or image). Called from an
/// `async` command, so building the window here is the documented safe path.
pub(crate) fn open(_app: &AppHandle, _id: i64) -> Result<(), String> {
    Err("unimplemented: shot_edit".into())
}

/// `shot_editor_get` — what the editor window shows, `None` when nothing is open.
pub(crate) fn payload(_app: &AppHandle) -> Result<Option<ShotEditorPayload>, String> {
    Err("unimplemented: shot_editor_get".into())
}

/// `shot_editor_switch { accept }` — the window's answer to a pending switch.
pub(crate) fn switch(_app: &AppHandle, _accept: bool) -> Result<(), String> {
    Err("unimplemented: shot_editor_switch".into())
}

/// After a successful `shot_editor_commit`: `done` closes (destroys) the window; `pin` /
/// `copy` / `save_as` keep it. No-op until W6-B.
pub(crate) fn after_commit(_app: &AppHandle, _action: ShotEditorAction) {}
