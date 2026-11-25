//! `store` commands (ARCHITECTURE §9.6) — owner: W1-A.
//! W0 stubs: every command is registered in `lib.rs` with its exact JS-visible
//! signature (argument names are the camelCase keys the frontend sends). Keep the
//! names, argument names/types and return types; add `State`/`AppHandle` params freely.
//! Errors: `Err("<code>: …")` with code `not_found | busy | invalid | failed`.

use crate::model::{AppInfo, ClearResult, ClearScope, Clip, ClipDetail, Counts, FeedQuery, Folder, Page, Snippet, SnippetInput, StorageStats, UndoResult};
use tauri::AppHandle;

/// One page of the feed (filters, free text, opaque cursor).
#[allow(unused_variables)] // W1-A: stub
#[tauri::command]
pub async fn clips_list(_app: AppHandle, q: FeedQuery) -> Result<Page, String> {
    Err("unimplemented: clips_list".into())
}

/// Pill counters.
#[tauri::command]
pub async fn clips_counts(_app: AppHandle) -> Result<Counts, String> {
    Err("unimplemented: clips_counts".into())
}

/// Full record for Quick Look.
#[allow(unused_variables)] // W1-A: stub
#[tauri::command]
pub async fn clip_detail(_app: AppHandle, id: i64) -> Result<ClipDetail, String> {
    Err("unimplemented: clip_detail".into())
}

/// Mark / unmark favorites (new favorites go to the end of the manual order).
#[allow(unused_variables)] // W1-A: stub
#[tauri::command]
pub async fn clip_set_favorite(_app: AppHandle, ids: Vec<i64>, value: bool) -> Result<(), String> {
    Err("unimplemented: clip_set_favorite".into())
}

/// Manual order inside the Favorites pill; `beforeId: null` = move to the end.
#[allow(unused_variables)] // W1-A: stub
#[tauri::command]
pub async fn clip_move_favorite(_app: AppHandle, id: i64, before_id: Option<i64>) -> Result<(), String> {
    Err("unimplemented: clip_move_favorite".into())
}

/// Move to a folder (`null` = remove from folder).
#[allow(unused_variables)] // W1-A: stub
#[tauri::command]
pub async fn clip_set_folder(_app: AppHandle, ids: Vec<i64>, folder_id: Option<i64>) -> Result<UndoResult, String> {
    Err("unimplemented: clip_set_folder".into())
}

/// User title (`null` = automatic).
#[allow(unused_variables)] // W1-A: stub
#[tauri::command]
pub async fn clip_rename(_app: AppHandle, id: i64, title: Option<String>) -> Result<Clip, String> {
    Err("unimplemented: clip_rename".into())
}

/// Edit the text in place (re-hash; drops html/rtf).
#[allow(unused_variables)] // W1-A: stub
#[tauri::command]
pub async fn clip_update_text(_app: AppHandle, id: i64, text: String) -> Result<Clip, String> {
    Err("unimplemented: clip_update_text".into())
}

/// New record from text (merge, transform → save); not routed through capture.
#[allow(unused_variables)] // W1-A: stub
#[tauri::command]
pub async fn clip_create_text(_app: AppHandle, text: String, folder_id: Option<i64>) -> Result<Clip, String> {
    Err("unimplemented: clip_create_text".into())
}

/// Soft delete with an undo token (2 min).
#[allow(unused_variables)] // W1-A: stub
#[tauri::command]
pub async fn clips_delete(_app: AppHandle, ids: Vec<i64>) -> Result<UndoResult, String> {
    Err("unimplemented: clips_delete".into())
}

/// Undo a delete / move / clear; `false` if the token expired.
#[allow(unused_variables)] // W1-A: stub
#[tauri::command]
pub async fn clips_undo(_app: AppHandle, token: String) -> Result<bool, String> {
    Err("unimplemented: clips_undo".into())
}

/// Hard-delete everything copied in the last N minutes (no undo); returns the count.
#[allow(unused_variables)] // W1-A: stub
#[tauri::command]
pub async fn clips_delete_recent(_app: AppHandle, minutes: i64) -> Result<i64, String> {
    Err("unimplemented: clips_delete_recent".into())
}

/// 'history' keeps favorites/folders/snippets (undoable); 'all' removes everything but snippets.
#[allow(unused_variables)] // W1-A: stub
#[tauri::command]
pub async fn clips_clear(_app: AppHandle, scope: ClearScope) -> Result<ClearResult, String> {
    Err("unimplemented: clips_clear".into())
}

/// Folders in manual order, with counts.
#[tauri::command]
pub async fn folders_list(_app: AppHandle) -> Result<Vec<Folder>, String> {
    Err("unimplemented: folders_list".into())
}

/// Create a folder (`invalid: exists` on a case-insensitive name clash).
#[allow(unused_variables)] // W1-A: stub
#[tauri::command]
pub async fn folder_create(_app: AppHandle, name: String) -> Result<Folder, String> {
    Err("unimplemented: folder_create".into())
}

/// Rename a folder.
#[allow(unused_variables)] // W1-A: stub
#[tauri::command]
pub async fn folder_rename(_app: AppHandle, id: i64, name: String) -> Result<Folder, String> {
    Err("unimplemented: folder_rename".into())
}

/// Reorder folders; `beforeId: null` = move to the end.
#[allow(unused_variables)] // W1-A: stub
#[tauri::command]
pub async fn folder_move(_app: AppHandle, id: i64, before_id: Option<i64>) -> Result<(), String> {
    Err("unimplemented: folder_move".into())
}

/// Delete a folder; its records stay in history unless `withClips`.
#[allow(unused_variables)] // W1-A: stub
#[tauri::command]
pub async fn folder_delete(_app: AppHandle, id: i64, with_clips: bool) -> Result<UndoResult, String> {
    Err("unimplemented: folder_delete".into())
}

/// Snippets matching `text` ("" = all), manual order.
#[allow(unused_variables)] // W1-A: stub
#[tauri::command]
pub async fn snippets_list(_app: AppHandle, text: String) -> Result<Vec<Snippet>, String> {
    Err("unimplemented: snippets_list".into())
}

/// Create (no id) or update a snippet.
#[allow(unused_variables)] // W1-A: stub
#[tauri::command]
pub async fn snippet_save(_app: AppHandle, input: SnippetInput) -> Result<Snippet, String> {
    Err("unimplemented: snippet_save".into())
}

/// Delete a snippet.
#[allow(unused_variables)] // W1-A: stub
#[tauri::command]
pub async fn snippet_delete(_app: AppHandle, id: i64) -> Result<(), String> {
    Err("unimplemented: snippet_delete".into())
}

/// Bump a snippet's use counter.
#[allow(unused_variables)] // W1-A: stub
#[tauri::command]
pub async fn snippet_mark_used(_app: AppHandle, id: i64) -> Result<(), String> {
    Err("unimplemented: snippet_mark_used".into())
}

/// Source applications with record counts.
#[tauri::command]
pub async fn apps_list(_app: AppHandle) -> Result<Vec<AppInfo>, String> {
    Err("unimplemented: apps_list".into())
}

/// Database / media sizes.
#[tauri::command]
pub async fn storage_stats(_app: AppHandle) -> Result<StorageStats, String> {
    Err("unimplemented: storage_stats".into())
}
