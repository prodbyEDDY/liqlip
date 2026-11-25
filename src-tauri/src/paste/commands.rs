//! `paste` commands (ARCHITECTURE §9.6) — owner: W1-C.
//! Names, JS argument names and return types are the W0 contract; every body runs its
//! blocking work (store reads, clipboard retries, focus waits, SendInput) on
//! `spawn_blocking`, never on the UI thread.
//! Errors: `Err("<code>: …")` with code `not_found | busy | invalid | failed`.

use super::{dragout, open, stack};
use crate::model::{CopyRequest, DragOutcome, PasteOutcome, PasteRequest, Stack, TextPasteRequest};
use tauri::AppHandle;

/// Run blocking work off the async runtime's core threads.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| format!("failed: {e}"))?
}

/// Paste records into the target window (or copy when there is none).
#[tauri::command]
pub async fn clip_paste(app: AppHandle, req: PasteRequest) -> Result<PasteOutcome, String> {
    blocking(move || super::paste_clips(&app, &req.ids, req.mode, req.separator.as_deref(), req.keep_open.unwrap_or(false))).await
}

/// Put records on the clipboard only.
#[tauri::command]
pub async fn clip_copy(app: AppHandle, req: CopyRequest) -> Result<PasteOutcome, String> {
    blocking(move || super::copy_clips(&app, &req.ids, req.mode, req.separator.as_deref())).await
}

/// Paste arbitrary text (snippets, transforms); `caretBack` = ← presses after.
#[tauri::command]
pub async fn text_paste(app: AppHandle, req: TextPasteRequest) -> Result<PasteOutcome, String> {
    blocking(move || super::paste_text(&app, &req.text, req.caret_back.unwrap_or(0), req.keep_open.unwrap_or(false))).await
}

/// Put arbitrary text on the clipboard.
#[tauri::command]
pub async fn text_copy(_app: AppHandle, text: String) -> Result<(), String> {
    blocking(move || super::copy_text(&text)).await
}

/// The clipboard's current `CF_UNICODETEXT` (`{clipboard}` in snippets), `None` when there is
/// no text (FIX-09). One short, retried `OpenClipboard` with a deadline (`capture::clipboard_text`);
/// text with privacy markers is returned too — pasting the snippet is an explicit user action.
/// `Err("busy: …")` → the shelf falls back to the newest history record.
#[tauri::command]
pub async fn clipboard_text(_app: AppHandle) -> Result<Option<String>, String> {
    blocking(crate::capture::clipboard_text).await
}

fn payload(app: &AppHandle, id: i64) -> Result<crate::store::PastePayload, String> {
    use tauri::Manager;
    app.state::<crate::store::Store>().get_for_paste(id)
}

/// Open a link in the browser / a file with its default app.
#[tauri::command]
pub async fn clip_open(app: AppHandle, id: i64) -> Result<(), String> {
    blocking(move || open::open(&app, &payload(&app, id)?)).await
}

/// "Open with…" dialog for a file record.
#[tauri::command]
pub async fn clip_open_with(app: AppHandle, id: i64) -> Result<(), String> {
    blocking(move || open::open_with(&app, &payload(&app, id)?)).await
}

/// Show the file (or the stored image) in Explorer.
#[tauri::command]
pub async fn clip_reveal(app: AppHandle, id: i64) -> Result<(), String> {
    blocking(move || open::reveal(&app, &payload(&app, id)?)).await
}

/// Native OLE drag of records out of the shelf.
#[tauri::command]
pub async fn clip_drag_out(app: AppHandle, ids: Vec<i64>) -> Result<DragOutcome, String> {
    if ids.is_empty() {
        return Err("invalid: no records".into());
    }
    let content = {
        let app = app.clone();
        blocking(move || {
            let payloads = super::load_payloads(&app, &ids)?;
            if payloads.is_empty() {
                return Err("not_found: records".to_string());
            }
            dragout::drag_content(&payloads, |p| open::resolve_media(&app, p), "\r\n")
        })
        .await?
    };
    dragout::drag_out(app, content).await
}

/// Current paste stack (records deleted from history are dropped first).
#[tauri::command]
pub async fn stack_get(app: AppHandle) -> Result<Stack, String> {
    blocking(move || Ok(stack::get(&app))).await
}

/// Append records to the paste stack.
#[tauri::command]
pub async fn stack_add(app: AppHandle, ids: Vec<i64>) -> Result<Stack, String> {
    blocking(move || stack::add(&app, &ids)).await
}

/// Remove one stack item.
#[tauri::command]
pub async fn stack_remove(app: AppHandle, key: String) -> Result<Stack, String> {
    blocking(move || stack::mutate(&app, |s| s.remove(&key).map(|_| ()))).await
}

/// Reorder the stack; `beforeKey: null` = move to the end.
#[tauri::command]
pub async fn stack_move(app: AppHandle, key: String, before_key: Option<String>) -> Result<Stack, String> {
    blocking(move || stack::mutate(&app, |s| s.move_before(&key, before_key.as_deref()))).await
}

/// Reverse the stack order.
#[tauri::command]
pub async fn stack_reverse(app: AppHandle) -> Result<Stack, String> {
    blocking(move || {
        stack::mutate(&app, |s| {
            s.reverse();
            Ok(())
        })
    })
    .await
}

/// Empty the stack.
#[tauri::command]
pub async fn stack_clear(app: AppHandle) -> Result<Stack, String> {
    blocking(move || {
        stack::mutate(&app, |s| {
            s.clear();
            Ok(())
        })
    })
    .await
}

/// Paste the whole stack joined by `separator`.
#[tauri::command]
pub async fn stack_paste_all(app: AppHandle, separator: String) -> Result<PasteOutcome, String> {
    blocking(move || stack::paste_all(&app, &separator)).await
}

/// Paste the top item into the active window and pop it.
#[tauri::command]
pub async fn stack_paste_next(app: AppHandle) -> Result<PasteOutcome, String> {
    blocking(move || stack::paste_next_with(&app, false)).await
}
