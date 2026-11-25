//! Paste — target/focus, UIPI, clipboard writers, SendInput, paste stack, open/reveal,
//! drag-out (ARCHITECTURE §6). Owner: **W1-C**.
//!
//! Layout: `focus` (target), `uipi` (integrity levels), `writer` (clipboard transaction +
//! byte formats), `input` (SendInput), `compose` (records → clipboard content), `flow`
//! (ordered paste steps), `stack` (paste stack), `open` (open / open with / reveal),
//! `dragout` (OLE drag), `commands` (§9.6).

pub mod commands;
mod compose;
pub mod dragout;
mod flow;
pub mod focus;
mod input;
mod open;
pub mod stack;
mod uipi;
mod writer;

use crate::model::{ChangeOrigin, ClipsChange, ClipsChanged, PasteChord, PasteMode, PasteOutcome, PasteReason, PasteResult};
use crate::store::{PastePayload, Store};
use flow::{AfterPaste, Job, Origin, RunResult};
use tauri::{AppHandle, Manager};
use writer::{Content, WriteError};

/// `setup()` step 5: start the hidden clipboard-owner window (its own message thread).
pub fn init(app: &AppHandle) {
    let _ = app;
    writer::start_owner();
}

/// Records for a paste/copy, in request order. Missing / deleted ids are skipped
/// (`not_found`); any other store error aborts.
pub(crate) fn load_payloads(app: &AppHandle, ids: &[i64]) -> Result<Vec<PastePayload>, String> {
    let store = app.state::<Store>();
    let mut out = Vec::with_capacity(ids.len());
    for &id in ids {
        match store.get_for_paste(id) {
            Ok(p) => out.push(p),
            Err(e) if e.starts_with("not_found") => {}
            Err(e) => return Err(e),
        }
    }
    Ok(out)
}

/// `used_at`/`use_count` (+ `copied_at` with `picker.bumpOnPaste`) and `clips:changed {origin:'user'}`.
fn mark_used(app: &AppHandle, ids: &[i64]) {
    if ids.is_empty() {
        return;
    }
    let bump = crate::settings::get(app).picker.bump_on_paste;
    match app.state::<Store>().mark_used(ids, bump) {
        Ok(()) => crate::events::clips_changed(
            app,
            &ClipsChanged { origin: ChangeOrigin::User, change: ClipsChange::Updated, ids: Some(ids.to_vec()) },
        ),
        Err(e) => crate::logx::warn("paste", &format!("mark_used: {e}")),
    }
}

fn failed(reason: PasteReason) -> PasteOutcome {
    PasteOutcome { result: PasteResult::Failed, reason: Some(reason) }
}

/// Resolve a stored image path against the data directory; `true` if the PNG vanished from
/// `media\` (→ `not_found` instead of an OS error).
fn resolve_image(app: &AppHandle, content: &mut Content) -> bool {
    if let Content::Image { png_path } = content {
        *png_path = open::resolve_media(app, png_path);
        return !png_path.exists();
    }
    false
}

/// The separator between joined records: the request's (front sends `"\n"` etc.), else
/// `picker.mergeSeparator`.
fn separator(app: &AppHandle, requested: Option<&str>) -> String {
    match requested {
        Some(s) => compose::normalize_separator(s),
        None => compose::merge_separator(crate::settings::get(app).picker.merge_separator).to_string(),
    }
}

/// How a set of records is delivered.
pub(crate) struct Delivery<'a> {
    pub mode: PasteMode,
    pub separator: Option<&'a str>,
    pub origin: Origin,
    pub target: Option<focus::Target>,
    pub after: AfterPaste,
    pub wait_keys: Vec<u16>,
}

/// Compose → encode → run the paste flow → mark used. Blocking.
pub(crate) fn deliver(app: &AppHandle, payloads: &[PastePayload], d: Delivery<'_>) -> Result<PasteOutcome, String> {
    let settings = crate::settings::get(app);
    let sep = separator(app, d.separator);
    let mut composed = compose::compose(payloads, d.mode, &sep, settings.picker.always_plain).map_err(|e| e.to_command_error())?;
    if resolve_image(app, &mut composed.content) {
        return Ok(failed(PasteReason::NotFound));
    }
    let entries = writer::encode(&composed.content).map_err(|e| e.to_command_error())?;
    let chord = d
        .target
        .as_ref()
        .map(|t| input::resolve_chord(&t.info.exe, &settings.picker.paste_keys))
        .unwrap_or(PasteChord::CtrlV);
    let job = Job {
        origin: d.origin,
        target: d.target,
        entries: &entries,
        chord,
        after: d.after,
        wait_keys: d.wait_keys,
        close_on_copy: settings.picker.close_on_copy,
    };
    match flow::run(app, job) {
        RunResult::WriteFailed(WriteError::Busy) => Ok(failed(PasteReason::ClipboardBusy)),
        RunResult::WriteFailed(e) => Err(e.to_command_error()),
        RunResult::Done(mut out) => {
            mark_used(app, &composed.used);
            if composed.partial && out.reason.is_none() {
                out.reason = Some(PasteReason::Partial);
            }
            Ok(out)
        }
    }
}

/// `clip_paste`: records into the target remembered at summon (or copy when there is none).
pub(crate) fn paste_clips(
    app: &AppHandle,
    ids: &[i64],
    mode: PasteMode,
    sep: Option<&str>,
    keep_open: bool,
) -> Result<PasteOutcome, String> {
    if ids.is_empty() {
        return Err("invalid: no records".into());
    }
    let payloads = load_payloads(app, ids)?;
    if payloads.is_empty() {
        return Ok(failed(PasteReason::NotFound));
    }
    deliver(
        app,
        &payloads,
        Delivery {
            mode,
            separator: sep,
            origin: Origin::Shelf { keep_open },
            target: focus::current(),
            after: AfterPaste::Nothing,
            wait_keys: Vec::new(),
        },
    )
}

/// `clip_copy`: records onto the clipboard only.
pub(crate) fn copy_clips(app: &AppHandle, ids: &[i64], mode: PasteMode, sep: Option<&str>) -> Result<PasteOutcome, String> {
    if ids.is_empty() {
        return Err("invalid: no records".into());
    }
    let payloads = load_payloads(app, ids)?;
    if payloads.is_empty() {
        return Ok(failed(PasteReason::NotFound));
    }
    let sep = separator(app, sep);
    let always_plain = crate::settings::get(app).picker.always_plain;
    let mut composed = compose::compose(&payloads, mode, &sep, always_plain).map_err(|e| e.to_command_error())?;
    if resolve_image(app, &mut composed.content) {
        return Ok(failed(PasteReason::NotFound));
    }
    let entries = writer::encode(&composed.content).map_err(|e| e.to_command_error())?;
    match writer::write(&entries) {
        Ok(()) => {
            mark_used(app, &composed.used);
            Ok(PasteOutcome { result: PasteResult::Copied, reason: composed.partial.then_some(PasteReason::Partial) })
        }
        Err(WriteError::Busy) => Ok(failed(PasteReason::ClipboardBusy)),
        Err(e) => Err(e.to_command_error()),
    }
}

/// `text_paste`: arbitrary text (snippets, transforms) into the remembered target;
/// `caret_back` ← presses afterwards for `{cursor}`.
pub(crate) fn paste_text(app: &AppHandle, text: &str, caret_back: u32, keep_open: bool) -> Result<PasteOutcome, String> {
    let entries = writer::encode_text(text, None, None);
    let settings = crate::settings::get(app);
    let target = focus::current();
    let chord = target
        .as_ref()
        .map(|t| input::resolve_chord(&t.info.exe, &settings.picker.paste_keys))
        .unwrap_or(PasteChord::CtrlV);
    let job = Job {
        origin: Origin::Shelf { keep_open },
        target,
        entries: &entries,
        chord,
        after: if caret_back > 0 { AfterPaste::CaretBack(caret_back) } else { AfterPaste::Nothing },
        wait_keys: Vec::new(),
        close_on_copy: settings.picker.close_on_copy,
    };
    match flow::run(app, job) {
        RunResult::Done(out) => Ok(out),
        RunResult::WriteFailed(WriteError::Busy) => Ok(failed(PasteReason::ClipboardBusy)),
        RunResult::WriteFailed(e) => Err(e.to_command_error()),
    }
}

/// `text_copy`: arbitrary text onto the clipboard (`CF_UNICODETEXT` only).
pub(crate) fn copy_text(text: &str) -> Result<(), String> {
    writer::write(&writer::encode_text(text, None, None)).map_err(|e| e.to_command_error())
}
