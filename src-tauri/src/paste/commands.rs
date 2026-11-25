//! `paste` commands (ARCHITECTURE §9.6) — owner: W1-C.
//! W0 stubs: every command is registered in `lib.rs` with its exact JS-visible
//! signature (argument names are the camelCase keys the frontend sends). Keep the
//! names, argument names/types and return types; add `State`/`AppHandle` params freely.
//! Errors: `Err("<code>: …")` with code `not_found | busy | invalid | failed`.

use crate::model::{CopyRequest, DragOutcome, PasteOutcome, PasteRequest, Stack, TextPasteRequest};
use tauri::AppHandle;

/// Paste records into the target window (or copy when there is none).
#[allow(unused_variables)] // W1-C: stub
#[tauri::command]
pub async fn clip_paste(_app: AppHandle, req: PasteRequest) -> Result<PasteOutcome, String> {
    Err("unimplemented: clip_paste".into())
}

/// Put records on the clipboard only.
#[allow(unused_variables)] // W1-C: stub
#[tauri::command]
pub async fn clip_copy(_app: AppHandle, req: CopyRequest) -> Result<PasteOutcome, String> {
    Err("unimplemented: clip_copy".into())
}

/// Paste arbitrary text (snippets, transforms); `caretBack` = ← presses after.
#[allow(unused_variables)] // W1-C: stub
#[tauri::command]
pub async fn text_paste(_app: AppHandle, req: TextPasteRequest) -> Result<PasteOutcome, String> {
    Err("unimplemented: text_paste".into())
}

/// Put arbitrary text on the clipboard.
#[allow(unused_variables)] // W1-C: stub
#[tauri::command]
pub async fn text_copy(_app: AppHandle, text: String) -> Result<(), String> {
    Err("unimplemented: text_copy".into())
}

/// Open a link in the browser / a file with its default app.
#[allow(unused_variables)] // W1-C: stub
#[tauri::command]
pub async fn clip_open(_app: AppHandle, id: i64) -> Result<(), String> {
    Err("unimplemented: clip_open".into())
}

/// "Open with…" dialog for a file record.
#[allow(unused_variables)] // W1-C: stub
#[tauri::command]
pub async fn clip_open_with(_app: AppHandle, id: i64) -> Result<(), String> {
    Err("unimplemented: clip_open_with".into())
}

/// Show the file (or the stored image) in Explorer.
#[allow(unused_variables)] // W1-C: stub
#[tauri::command]
pub async fn clip_reveal(_app: AppHandle, id: i64) -> Result<(), String> {
    Err("unimplemented: clip_reveal".into())
}

/// Native OLE drag of records out of the shelf.
#[allow(unused_variables)] // W1-C: stub
#[tauri::command]
pub async fn clip_drag_out(_app: AppHandle, ids: Vec<i64>) -> Result<DragOutcome, String> {
    Err("unimplemented: clip_drag_out".into())
}

/// Current paste stack (safe read; empty until W1-C).
#[tauri::command]
pub async fn stack_get(app: AppHandle) -> Result<Stack, String> {
    Ok(crate::paste::stack::snapshot(&app))
}

/// Append records to the paste stack.
#[allow(unused_variables)] // W1-C: stub
#[tauri::command]
pub async fn stack_add(_app: AppHandle, ids: Vec<i64>) -> Result<Stack, String> {
    Err("unimplemented: stack_add".into())
}

/// Remove one stack item.
#[allow(unused_variables)] // W1-C: stub
#[tauri::command]
pub async fn stack_remove(_app: AppHandle, key: String) -> Result<Stack, String> {
    Err("unimplemented: stack_remove".into())
}

/// Reorder the stack; `beforeKey: null` = move to the end.
#[allow(unused_variables)] // W1-C: stub
#[tauri::command]
pub async fn stack_move(_app: AppHandle, key: String, before_key: Option<String>) -> Result<Stack, String> {
    Err("unimplemented: stack_move".into())
}

/// Reverse the stack order.
#[tauri::command]
pub async fn stack_reverse(_app: AppHandle) -> Result<Stack, String> {
    Err("unimplemented: stack_reverse".into())
}

/// Empty the stack.
#[tauri::command]
pub async fn stack_clear(_app: AppHandle) -> Result<Stack, String> {
    Err("unimplemented: stack_clear".into())
}

/// Paste the whole stack joined by `separator`.
#[allow(unused_variables)] // W1-C: stub
#[tauri::command]
pub async fn stack_paste_all(_app: AppHandle, separator: String) -> Result<PasteOutcome, String> {
    Err("unimplemented: stack_paste_all".into())
}

/// Paste the top item into the active window and pop it.
#[tauri::command]
pub async fn stack_paste_next(_app: AppHandle) -> Result<PasteOutcome, String> {
    Err("unimplemented: stack_paste_next".into())
}
