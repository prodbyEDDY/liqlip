//! Files in the screenshot folder (owner: W6-A per ARCHITECTURE §18.1; §18.4 step 5). W5-A stub.
//!
//! To do: the folder (`settings.screenshot.folder`, "" = `FOLDERID_Pictures\Liqlip`, created on
//! demand), the name template `{date} {time} {app} {n}`, `(2)` for a taken name, PNG / JPG
//! (`jpgQuality`), `store::shots::set_saved_path`; Save as… through `tauri-plugin-dialog`
//! (start in the screenshot folder); reveal = Explorer with `saved_path` selected. Commands
//! take a record id, never a path from the webview (research §6.2.9).

use tauri::AppHandle;

/// `shot_save { id }` → the written path.
pub(crate) fn save(_app: &AppHandle, _id: i64) -> Result<String, String> {
    Err("unimplemented: shot_save".into())
}

/// `shot_save_as { id }` → the chosen path, `None` = the dialog was cancelled. `async`: the
/// dialog answers through a callback (as `settings::pick_exe` awaits a oneshot).
pub(crate) async fn save_as(_app: AppHandle, _id: i64) -> Result<Option<String>, String> {
    Err("unimplemented: shot_save_as".into())
}

/// `shot_reveal { id }` — Explorer with the saved file selected.
pub(crate) fn reveal(_app: &AppHandle, _id: i64) -> Result<(), String> {
    Err("unimplemented: shot_reveal".into())
}
