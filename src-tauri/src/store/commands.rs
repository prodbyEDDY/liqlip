//! `store` commands (ARCHITECTURE §9.6) — owner: W1-A.
//! JS-visible signatures are the W0 contract (argument names are the camelCase keys the
//! frontend sends; §9.9). Every command runs its SQLite work on the blocking pool
//! (`tauri::async_runtime::spawn_blocking`) and, for a mutation, emits
//! `clips:changed { origin: 'user' }` through `events.rs` after it succeeded.
//! Errors: `Err("<code>: …")` with code `not_found | busy | invalid | failed`.
//!
//! Folder and snippet edits also emit `clips:changed { origin: 'user', change: 'updated' }`
//! without `ids`: it is the only cross-window signal, and the pills (folder names, counts,
//! the Snippets counter) must follow an edit made in the other window.

use super::lifecycle::Undone;
use super::Store;
use crate::events;
use crate::model::{
    AppInfo, ChangeOrigin, ClearResult, ClearScope, Clip, ClipDetail, ClipsChange, ClipsChanged, Counts, FeedQuery,
    Folder, Page, Snippet, SnippetInput, StorageStats, UndoResult,
};
use tauri::{AppHandle, Manager};

/// Event payloads carry at most this many ids (the frontend reloads without ids).
const EVENT_IDS_MAX: usize = 1000;

/// Run `f` against the managed `Store` on the blocking pool.
async fn blocking<T, F>(app: &AppHandle, f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&Store) -> Result<T, String> + Send + 'static,
{
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let store = app.try_state::<Store>().ok_or_else(|| "failed: store is not ready".to_string())?;
        f(&store)
    })
    .await
    .map_err(|e| format!("failed: {e}"))?
}

fn changed(app: &AppHandle, change: ClipsChange, ids: Option<Vec<i64>>) {
    let ids = ids.filter(|v| v.len() <= EVENT_IDS_MAX);
    events::clips_changed(app, &ClipsChanged { origin: ChangeOrigin::User, change, ids });
}

/// After a hard delete (delete recent, clear all): drop paste-stack items of records that are
/// gone right away, so their previews do not linger in memory, in the stack popover or in
/// the tray count until the stack is next touched (W3). Blocking store reads → its own task.
fn prune_stack(app: &AppHandle) {
    let app = app.clone();
    drop(tauri::async_runtime::spawn_blocking(move || crate::paste::stack::prune(&app)));
}

/// One page of the feed (filters, free text, opaque cursor).
#[tauri::command]
pub async fn clips_list(app: AppHandle, q: FeedQuery) -> Result<Page, String> {
    let favorites_first = crate::settings::get(&app).picker.favorites_first;
    blocking(&app, move |s| s.clips_list(&q, favorites_first)).await
}

/// Pill counters.
#[tauri::command]
pub async fn clips_counts(app: AppHandle) -> Result<Counts, String> {
    blocking(&app, |s| s.counts()).await
}

/// Full record for Quick Look.
#[tauri::command]
pub async fn clip_detail(app: AppHandle, id: i64) -> Result<ClipDetail, String> {
    blocking(&app, move |s| s.clip_detail(id)).await
}

/// Mark / unmark favorites (new favorites go to the end of the manual order).
#[tauri::command]
pub async fn clip_set_favorite(app: AppHandle, ids: Vec<i64>, value: bool) -> Result<(), String> {
    let ids = blocking(&app, move |s| s.set_favorite(&ids, value)).await?;
    if !ids.is_empty() {
        changed(&app, ClipsChange::Updated, Some(ids));
    }
    Ok(())
}

/// Manual order inside the Favorites pill; `beforeId: null` = move to the end.
#[tauri::command]
pub async fn clip_move_favorite(app: AppHandle, id: i64, before_id: Option<i64>) -> Result<(), String> {
    blocking(&app, move |s| s.move_favorite(id, before_id)).await?;
    changed(&app, ClipsChange::Updated, Some(vec![id]));
    Ok(())
}

/// Move to a folder (`null` = remove from folder).
#[tauri::command]
pub async fn clip_set_folder(app: AppHandle, ids: Vec<i64>, folder_id: Option<i64>) -> Result<UndoResult, String> {
    let (ids, undo_token) = blocking(&app, move |s| s.set_folder(&ids, folder_id)).await?;
    if !ids.is_empty() {
        changed(&app, ClipsChange::Updated, Some(ids));
    }
    Ok(UndoResult { undo_token })
}

/// User title (`null` = automatic).
#[tauri::command]
pub async fn clip_rename(app: AppHandle, id: i64, title: Option<String>) -> Result<Clip, String> {
    let clip = blocking(&app, move |s| s.rename(id, title.as_deref())).await?;
    changed(&app, ClipsChange::Updated, Some(vec![id]));
    Ok(clip)
}

/// Edit the text in place (re-hash; drops html/rtf).
#[tauri::command]
pub async fn clip_update_text(app: AppHandle, id: i64, text: String) -> Result<Clip, String> {
    let clip = blocking(&app, move |s| s.update_text(id, &text)).await?;
    changed(&app, ClipsChange::Updated, Some(vec![id]));
    Ok(clip)
}

/// New record from text (merge, transform → save); not routed through capture.
#[tauri::command]
pub async fn clip_create_text(app: AppHandle, text: String, folder_id: Option<i64>) -> Result<Clip, String> {
    let (clip, created) = blocking(&app, move |s| s.create_text(&text, folder_id)).await?;
    let change = if created { ClipsChange::Added } else { ClipsChange::Updated };
    changed(&app, change, Some(vec![clip.id]));
    Ok(clip)
}

/// Soft delete with an undo token (2 min).
#[tauri::command]
pub async fn clips_delete(app: AppHandle, ids: Vec<i64>) -> Result<UndoResult, String> {
    let (ids, undo_token) = blocking(&app, move |s| s.delete(&ids)).await?;
    if !ids.is_empty() {
        changed(&app, ClipsChange::Deleted, Some(ids));
    }
    Ok(UndoResult { undo_token })
}

/// Undo a delete / move / clear; `false` if the token expired.
#[tauri::command]
pub async fn clips_undo(app: AppHandle, token: String) -> Result<bool, String> {
    match blocking(&app, move |s| s.undo(&token)).await? {
        Some(Undone { change, ids }) => {
            changed(&app, change, ids);
            Ok(true)
        }
        None => Ok(false),
    }
}

/// Hard-delete everything copied in the last N minutes (no undo); returns the count.
#[tauri::command]
pub async fn clips_delete_recent(app: AppHandle, minutes: i64) -> Result<i64, String> {
    let ids = blocking(&app, move |s| s.delete_recent_ids(minutes)).await?;
    let n = ids.len() as i64;
    if n > 0 {
        changed(&app, ClipsChange::Deleted, Some(ids));
        prune_stack(&app);
    }
    Ok(n)
}

/// 'history' keeps favorites/folders/snippets (undoable); 'all' removes everything but snippets.
#[tauri::command]
pub async fn clips_clear(app: AppHandle, scope: ClearScope) -> Result<ClearResult, String> {
    let undo_token = blocking(&app, move |s| s.clear(scope)).await?;
    changed(&app, ClipsChange::Reset, None);
    if scope == ClearScope::All {
        prune_stack(&app);
    }
    Ok(ClearResult { undo_token })
}

/// Folders in manual order, with counts.
#[tauri::command]
pub async fn folders_list(app: AppHandle) -> Result<Vec<Folder>, String> {
    blocking(&app, |s| s.folders_list()).await
}

/// Create a folder (`invalid: exists` on a case-insensitive name clash).
#[tauri::command]
pub async fn folder_create(app: AppHandle, name: String) -> Result<Folder, String> {
    let folder = blocking(&app, move |s| s.folder_create(&name)).await?;
    changed(&app, ClipsChange::Updated, None);
    Ok(folder)
}

/// Rename a folder.
#[tauri::command]
pub async fn folder_rename(app: AppHandle, id: i64, name: String) -> Result<Folder, String> {
    let folder = blocking(&app, move |s| s.folder_rename(id, &name)).await?;
    changed(&app, ClipsChange::Updated, None);
    Ok(folder)
}

/// Reorder folders; `beforeId: null` = move to the end.
#[tauri::command]
pub async fn folder_move(app: AppHandle, id: i64, before_id: Option<i64>) -> Result<(), String> {
    blocking(&app, move |s| s.folder_move(id, before_id)).await?;
    changed(&app, ClipsChange::Updated, None);
    Ok(())
}

/// Delete a folder; its records stay in history unless `withClips`.
#[tauri::command]
pub async fn folder_delete(app: AppHandle, id: i64, with_clips: bool) -> Result<UndoResult, String> {
    let (undo_token, members) = blocking(&app, move |s| s.folder_delete(id, with_clips)).await?;
    let change = if with_clips { ClipsChange::Deleted } else { ClipsChange::Updated };
    changed(&app, change, Some(members));
    Ok(UndoResult { undo_token })
}

/// Snippets matching `text` ("" = all), manual order.
#[tauri::command]
pub async fn snippets_list(app: AppHandle, text: String) -> Result<Vec<Snippet>, String> {
    blocking(&app, move |s| s.snippets_list(&text)).await
}

/// Create (no id) or update a snippet.
#[tauri::command]
pub async fn snippet_save(app: AppHandle, input: SnippetInput) -> Result<Snippet, String> {
    let snippet = blocking(&app, move |s| s.snippet_save(&input)).await?;
    changed(&app, ClipsChange::Updated, None);
    Ok(snippet)
}

/// Delete a snippet.
#[tauri::command]
pub async fn snippet_delete(app: AppHandle, id: i64) -> Result<(), String> {
    blocking(&app, move |s| s.snippet_delete(id)).await?;
    changed(&app, ClipsChange::Updated, None);
    Ok(())
}

/// Manual snippet order (W2-B); `beforeId: null` = move to the end.
#[tauri::command]
pub async fn snippet_move(app: AppHandle, id: i64, before_id: Option<i64>) -> Result<(), String> {
    blocking(&app, move |s| s.snippet_move(id, before_id)).await?;
    changed(&app, ClipsChange::Updated, None);
    Ok(())
}

/// Bump a snippet's use counter.
#[tauri::command]
pub async fn snippet_mark_used(app: AppHandle, id: i64) -> Result<(), String> {
    blocking(&app, move |s| s.snippet_mark_used(id)).await
}

/// Source applications with record counts.
#[tauri::command]
pub async fn apps_list(app: AppHandle) -> Result<Vec<AppInfo>, String> {
    blocking(&app, |s| s.apps_list()).await
}

/// Database / media sizes.
#[tauri::command]
pub async fn storage_stats(app: AppHandle) -> Result<StorageStats, String> {
    blocking(&app, |s| s.storage_stats()).await
}
