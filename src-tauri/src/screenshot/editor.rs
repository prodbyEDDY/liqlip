//! The editor window `editor` (owner: W6-B; ARCHITECTURE §18.2, §18.9; SPEC §9.5).
//!
//! - **One window, built on demand** (`windows.rs`): an ordinary window — system title bar,
//!   taskbar button, Alt+Tab — min 640×420, sized to fit the picture into 80 % of the work area
//!   of the cursor's monitor, centred there, focused. Title "Name — Liqlip" (follows the
//!   language). Closing destroys it (the page's `onCloseRequested` asks about unsaved changes
//!   first and then calls `destroy()`; `WindowEvent::Destroyed` clears [`EditorState`]).
//! - **A second `shot_edit`** while it is open keeps the window and switches the record: the
//!   new record becomes current at once and `shot:editor` carries it with `dirtyGuard: true`
//!   (only the event sets it; `shot_editor_get` always says `false`); the page loads it, or —
//!   with unsaved changes — asks and answers `shot_editor_switch` (`false` = stay: the previous
//!   record becomes current again). "Save" in that dialog sends `done` for the PREVIOUS record
//!   first and then `shot_editor_switch(true)`: while a switch is pending, `done` of the record
//!   the switch leaves keeps the window open (the next record loads into it). `shot_editor_get`
//!   drops a switch the page never answered (its `shot:editor` was lost while the page loaded:
//!   the page shows `current` anyway), so a stale switch never keeps "Done" from closing.
//! - **Closing after "Done"** is asynchronous (`destroy()` reaches the main thread later): the
//!   state is marked `closing` at once, a `shot_edit` that arrives meanwhile waits for the old
//!   window to go and builds a fresh one (instead of switching a dying page), and a window's
//!   `Destroyed` only clears the state of its own generation (never a newer window's).
//! - **The commit** (`shot_editor_commit`, body already split by `body.rs`) is [`commit`] (INT-5:
//!   W6-A's steps called directly): `done` with pixels → `commit::apply_edit` (store, thumbnail,
//!   `clips:changed`) + clipboard (if `settings.screenshot.copy`) + rewrite of the saved file,
//!   then the window closes; `done` WITHOUT pixels = nothing changed: the record is not
//!   rewritten, its picture goes to the clipboard (if `copy`) and the window closes; `copy` →
//!   clipboard only; `save_as` → dialog; `pin` with pixels (unsaved changes) → the same as
//!   `done` but the window stays, then the pin; `pin` without pixels → the pin of the record as
//!   it is. A body without pixels always means "the record's current picture".

use super::windows::{self as w, Mon, PxRect};
use crate::i18n_native::{self, Key, Lang};
use crate::model::{Clip, ClipDetail, Kind, ShotEditorAction, ShotEditorCommit, ShotEditorPayload};
use crate::store::Store;
use crate::{events, logx, paste, picker, settings};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};

/// Minimum inner size, logical px (SPEC §9.5).
pub(crate) const MIN_SIZE: (f64, f64) = (640.0, 420.0);
/// Room around the picture inside the window, logical px (W6-C's layout, `EditorWindow.svelte`):
/// the fit margin of the canvas (24 on each side, `view.ts::FIT_MARGIN`) across; the two bars on
/// top (12 + 40 + 8 + 40 + 8) plus that margin down.
const CHROME: (f64, f64) = (48.0, 156.0);
/// Share of the work area the window may take by default.
const FIT: f64 = 0.8;
/// How long a `shot_edit` waits for the window that "Done" is closing (it usually goes within a
/// frame or two); after that it answers `busy`.
const CLOSE_WAIT: Duration = Duration::from_secs(2);
const CLOSE_POLL: Duration = Duration::from_millis(20);

/// `?v=` of the base picture URL: every payload asks for a fresh copy (`no-store` keeps the
/// disk cache out, this keeps the page's memory cache out after an edit).
static VERSION: AtomicU64 = AtomicU64::new(0);

/// What the editor window side remembers (`ScreenshotState.editor`).
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct EditorState {
    /// The record the window shows (or is switching to).
    current: Option<i64>,
    /// The record shown before a switch the page has not answered yet.
    previous: Option<i64>,
    /// The window is being built (a second `shot_edit` only moves `current`).
    building: bool,
    /// "Done" is destroying the window; it is gone once it leaves the window manager.
    closing: bool,
    /// Generation of the window this state belongs to (grows with every build, never resets).
    gen: u64,
}

/// What `shot_edit` does with the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Next {
    /// The window is being built: the page asks `shot_editor_get` once it has loaded.
    Wait,
    /// Wait for the window "Done" is closing, then ask again.
    Closing,
    /// The window already shows this record: bring it forward.
    Focus,
    /// Another record in the open window (`shot:editor`).
    Switch,
    /// Build a window of this generation.
    Build(u64),
}

impl EditorState {
    /// `shot_edit(id)`; `window` = an `editor` window is in the window manager.
    fn open(&mut self, id: i64, window: bool) -> Next {
        if self.building {
            self.current = Some(id);
            return Next::Wait;
        }
        if window && self.closing {
            return Next::Closing;
        }
        if window {
            if self.current == Some(id) && self.previous.is_none() {
                return Next::Focus;
            }
            if self.previous.is_none() {
                self.previous = self.current;
            }
            self.current = Some(id);
            return Next::Switch;
        }
        let gen = self.gen + 1;
        *self = EditorState { current: Some(id), previous: None, building: true, closing: false, gen };
        Next::Build(gen)
    }

    /// The build of generation `gen` finished (`ok`) or failed.
    fn built(&mut self, gen: u64, ok: bool) {
        if self.gen == gen {
            self.building = false;
            if !ok {
                (self.current, self.previous) = (None, None);
            }
        }
    }

    /// `shot_editor_get`: the page loads `current`. A switch it has not answered is moot — its
    /// `shot:editor` was lost while the page loaded (the page shows `current` anyway).
    fn get(&mut self) -> Option<i64> {
        self.previous = None;
        self.current
    }

    /// `shot_editor_switch(accept)`: `false` makes the previous record current again (returned).
    fn answer(&mut self, accept: bool) -> Option<i64> {
        match self.previous.take() {
            Some(prev) if !accept => {
                self.current = Some(prev);
                Some(prev)
            }
            _ => None,
        }
    }

    /// What follows a successful commit of record `id`: `done` closes the window — unless a
    /// switch is pending and `id` is the record it leaves (the page saved it before moving on,
    /// the next record loads into the window); `pin` pins the record as it is in history.
    fn after(&self, action: ShotEditorAction, id: i64) -> After {
        match action {
            ShotEditorAction::Done if self.previous == Some(id) => After::Nothing,
            ShotEditorAction::Done => After::Close,
            ShotEditorAction::Pin => After::Pin,
            _ => After::Nothing,
        }
    }

    /// "Done" destroys the window: no record is shown any more.
    fn close(&mut self) {
        (self.current, self.previous, self.closing) = (None, None, true);
    }

    /// `WindowEvent::Destroyed` of the window of generation `gen` (a newer window's state stays).
    fn destroyed(&mut self, gen: u64) {
        if self.gen == gen {
            *self = EditorState { gen, ..EditorState::default() };
        }
    }
}

fn with<T>(app: &AppHandle, f: impl FnOnce(&mut EditorState) -> T) -> Option<T> {
    let st = app.try_state::<super::ScreenshotState>()?;
    let mut guard = st.editor.lock();
    Some(f(&mut guard))
}

/// `windows::init`: the title follows `general.language` (as the settings window's).
pub(crate) fn init(app: &AppHandle) {
    settings::subscribe(app, {
        let app = app.clone();
        move |old, new| {
            if old.general.language == new.general.language {
                return;
            }
            let Some(id) = with(&app, |st| st.current).flatten() else { return };
            let Some(win) = app.get_webview_window(events::EDITOR) else { return };
            let lang = i18n_native::resolve(new.general.language);
            if let Ok(d) = picture(&app, id) {
                let _ = win.set_title(&title(&name_of(&d, lang)));
            }
        }
    });
}

// ── Commands ────────────────────────────────────────────────────────────────

/// `shot_edit { id }` — open the editor on a record (screenshot or image). Called from an
/// `async` command, so building the window here is the documented safe path; on the main
/// thread (a caller outside the command layer) the work moves to a short thread.
pub(crate) fn open(app: &AppHandle, id: i64) -> Result<(), String> {
    if w::on_main_thread() {
        let app = app.clone();
        std::thread::Builder::new()
            .name("liqlip-editor-open".into())
            .spawn(move || {
                if let Err(e) = open_now(&app, id) {
                    logx::warn("screenshot", &format!("editor open: {e}"));
                }
            })
            .map_err(|e| format!("failed: {e}"))?;
        return Ok(());
    }
    open_now(app, id)
}

fn open_now(app: &AppHandle, id: i64) -> Result<(), String> {
    let detail = picture(app, id)?;
    let deadline = Instant::now() + CLOSE_WAIT;
    let (next, existing) = loop {
        let existing = app.get_webview_window(events::EDITOR);
        let next = with(app, |st| st.open(id, existing.is_some())).ok_or("failed: screenshot state")?;
        if next != Next::Closing {
            break (next, existing);
        }
        // "Done" is destroying the window (off the main thread: this is a command or a spawned
        // thread). Switching it would talk to a dying page; wait for it to go, then build.
        if Instant::now() >= deadline {
            return Err("busy: the editor window is closing".into());
        }
        std::thread::sleep(CLOSE_POLL);
    };
    match next {
        Next::Wait | Next::Closing => Ok(()),
        Next::Focus => {
            focus(app);
            Ok(())
        }
        Next::Switch => {
            events::shot_editor(app, &payload_of(app, &detail, true));
            if let Some(win) = existing {
                let _ = win.set_title(&title(&name_of(&detail, lang(app))));
            }
            focus(app);
            Ok(())
        }
        Next::Build(gen) => {
            let built = build(app, &detail, gen);
            with(app, |st| st.built(gen, built.is_ok()));
            built
        }
    }
}

/// `shot_editor_get` — what the editor window shows, `None` when nothing is open.
pub(crate) fn payload(app: &AppHandle) -> Result<Option<ShotEditorPayload>, String> {
    let Some(id) = with(app, EditorState::get).flatten() else { return Ok(None) };
    let detail = picture(app, id)?;
    Ok(Some(payload_of(app, &detail, false)))
}

/// `shot_editor_switch { accept }` — the page's answer to a pending switch (`shot:editor`).
/// `false` makes the previous record current again. No pending switch → nothing to do.
pub(crate) fn switch(app: &AppHandle, accept: bool) -> Result<(), String> {
    let back = with(app, |st| st.answer(accept)).ok_or("failed: screenshot state")?;
    if let (Some(id), Some(win)) = (back, app.get_webview_window(events::EDITOR)) {
        if let Ok(d) = picture(app, id) {
            let _ = win.set_title(&title(&name_of(&d, lang(app))));
        }
    }
    Ok(())
}

/// `shot_editor_commit` — see the module docs. `rgba` = the flattened picture (the page's
/// export of the current document, `req.width × req.height`); `None` = the record's current
/// picture (for `done`: nothing changed).
pub(crate) async fn commit(app: &AppHandle, req: ShotEditorCommit, rgba: Option<&[u8]>) -> Result<Clip, String> {
    let id = req.clip_id;
    let detail = picture(app, id)?;
    match (req.action, rgba) {
        (ShotEditorAction::Done, Some(px)) => {
            let clip = apply(app, id, px, req.width, req.height, req.doc)?;
            after(app, ShotEditorAction::Done, id);
            Ok(clip)
        }
        (ShotEditorAction::Done, None) => {
            if settings::get(app).screenshot.copy {
                if let Err(e) = record_png(app, id).and_then(|png| paste::copy_png(png, None)) {
                    logx::warn("screenshot", &format!("editor done: clipboard: {e}"));
                }
            }
            after(app, ShotEditorAction::Done, id);
            Ok(detail.clip)
        }
        (ShotEditorAction::Copy, px) => {
            match px {
                Some(px) => paste::copy_png(super::commit::encode_png(req.width, req.height, px)?, Some((req.width, req.height, px)))?,
                None => paste::copy_png(record_png(app, id)?, None)?,
            }
            Ok(detail.clip)
        }
        (ShotEditorAction::SaveAs, px) => {
            let png = match px {
                Some(px) => super::commit::encode_png(req.width, req.height, px)?,
                None => record_png(app, id)?,
            };
            super::files::save_png_as(app.clone(), id, png).await?;
            // The record now carries the chosen file as its saved path (`meta.shot.savedPath`).
            Ok(fresh(app, id).unwrap_or(detail.clip))
        }
        (ShotEditorAction::Pin, Some(px)) => {
            // Unsaved changes: applied to the record first (as Done, the window stays), so the
            // pin shows what the editor shows.
            let clip = apply(app, id, px, req.width, req.height, req.doc)?;
            after(app, ShotEditorAction::Pin, id);
            Ok(clip)
        }
        (ShotEditorAction::Pin, None) => {
            after(app, ShotEditorAction::Pin, id);
            Ok(detail.clip)
        }
    }
}

/// Done's steps on the record (W6-A, §18.5): the edit (PNG, hash, thumbnail, document, base kept,
/// `clips:changed`), then the clipboard (if `settings.screenshot.copy`) and the saved file
/// rewritten in its own format. A failure after the edit never undoes it (logged).
fn apply(app: &AppHandle, id: i64, rgba: &[u8], width: u32, height: u32, doc: serde_json::Value) -> Result<Clip, String> {
    let (clip, png) = super::commit::apply_edit(app, id, rgba, width, height, doc)?;
    if settings::get(app).screenshot.copy {
        if let Err(e) = paste::copy_png(png.clone(), Some((width, height, rgba))) {
            logx::warn("screenshot", &format!("editor done: clipboard: {e}"));
        }
    }
    if let Err(e) = super::files::overwrite_saved(app, id, &png) {
        logx::warn("screenshot", &format!("editor done: saved file: {e}"));
    }
    Ok(clip)
}

/// The record's current picture (`media\<id>.png`).
fn record_png(app: &AppHandle, id: i64) -> Result<Vec<u8>, String> {
    let src = app.try_state::<Store>().ok_or("failed: store")?.shot_source(id)?;
    std::fs::read(&src.png).map_err(|_| format!("not_found: picture of clip {id}"))
}

fn fresh(app: &AppHandle, id: i64) -> Option<Clip> {
    app.try_state::<Store>()?.shot_clip(id).ok()
}

// ── Window ──────────────────────────────────────────────────────────────────

/// What the window does after a successful commit of record `id`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum After {
    Nothing,
    Close,
    Pin,
}

fn after(app: &AppHandle, action: ShotEditorAction, id: i64) {
    // Decided and, for "close", marked under one lock: a `shot_edit` right after this waits for
    // the window to go instead of switching it.
    let effect = with(app, |st| {
        let effect = st.after(action, id);
        if effect == After::Close {
            st.close();
        }
        effect
    })
    .unwrap_or(After::Nothing);
    match effect {
        After::Close => {
            if let Some(win) = app.get_webview_window(events::EDITOR) {
                if let Err(e) = win.destroy() {
                    with(app, |st| st.closing = false);
                    logx::warn("screenshot", &format!("editor close: {e}"));
                }
            }
        }
        After::Pin => {
            if let Err(e) = super::pin::open(app, id) {
                logx::warn("screenshot", &format!("editor pin: {e}"));
            }
        }
        After::Nothing => {}
    }
}

fn build(app: &AppHandle, detail: &ClipDetail, gen: u64) -> Result<(), String> {
    w::may_build()?;
    let mons = w::monitors(app);
    let cursor = w::cursor_pos();
    let mon = w::pick_monitor(&mons, None, cursor).cloned();
    let (pw, ph) = (detail.clip.width.unwrap_or(0), detail.clip.height.unwrap_or(0));
    let rect = mon.as_ref().map(|m| editor_rect(m, pw, ph));
    let scale = mon.as_ref().map_or(1.0, |m| m.scale);
    let (lw, lh) = rect.map_or(MIN_SIZE, |r| (f64::from(r.w) / scale, f64::from(r.h) / scale));
    // The shelf is always-on-top: it would cover the editor (a no-op when it is closed).
    picker::hide_now(app);
    let win = WebviewWindowBuilder::new(app, events::EDITOR, WebviewUrl::App("editor.html".into()))
        .title(title(&name_of(detail, lang(app))))
        .decorations(true)
        .resizable(true)
        .min_inner_size(MIN_SIZE.0, MIN_SIZE.1)
        .inner_size(lw, lh)
        .visible(false)
        .theme(w::native_theme(app))
        .additional_browser_args(w::BROWSER_ARGS)
        .on_navigation(w::navigation_guard(app))
        .build()
        .map_err(|e| format!("failed: {e}"))?;
    let app2 = app.clone();
    win.on_window_event(move |ev| {
        if let WindowEvent::Destroyed = ev {
            with(&app2, |st| st.destroyed(gen));
        }
    });
    let app3 = app.clone();
    let posted = app.run_on_main_thread(move || {
        let Some(win) = app3.get_webview_window(events::EDITOR) else { return };
        if let Some(r) = rect {
            w::place(&win, r);
        }
        if let Err(e) = win.show() {
            logx::warn("screenshot", &format!("editor show: {e}"));
        }
        if let Some(r) = rect {
            // A DPI change on the first show of a window built on another monitor: re-assert.
            w::place(&win, r);
        }
        foreground(&win);
    });
    posted.map_err(|e| format!("failed: {e}"))
}

/// Bring the existing window forward (main thread: tao's queued calls stay in order).
fn focus(app: &AppHandle) {
    picker::hide_now(app);
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(win) = app2.get_webview_window(events::EDITOR) else { return };
        if win.is_minimized().unwrap_or(false) {
            let _ = win.unminimize();
        }
        let _ = win.show();
        foreground(&win);
    });
}

/// `SetForegroundWindow` — allowed after a click in the QAO / the shelf or a hotkey; a refusal
/// makes Windows flash the taskbar button (never forced: no Alt injection, as the shelf).
fn foreground(win: &tauri::WebviewWindow) {
    if let Some(h) = w::hwnd(win) {
        if !w::set_foreground(h) {
            logx::warn("screenshot", "editor: foreground refused (taskbar button flashes)");
        }
    }
}

/// The window's rect: the picture at 100 % (1 px = 1 physical px) plus the chrome, shrunk to
/// fit [`FIT`] of the work area, at least [`MIN_SIZE`], centred on the work area. A window larger
/// than a small work area (the minimum at 150 % on a 1024×600 screen) starts at its top-left
/// corner: the title bar — the only grip of the window — never lands above the screen.
fn editor_rect(mon: &Mon, pic_w: i64, pic_h: i64) -> PxRect {
    let (s, wa) = (mon.scale, mon.work);
    let (cw, ch) = (w::px(CHROME.0, s), w::px(CHROME.1, s));
    let (min_w, min_h) = (w::px(MIN_SIZE.0, s), w::px(MIN_SIZE.1, s));
    let (max_w, max_h) = (((f64::from(wa.w) * FIT) as i32).max(min_w), ((f64::from(wa.h) * FIT) as i32).max(min_h));
    let (pw, ph) = (pic_w.max(1) as f64, pic_h.max(1) as f64);
    let fit = (f64::from((max_w - cw).max(1)) / pw).min(f64::from((max_h - ch).max(1)) / ph).min(1.0);
    let width = ((pw * fit).round() as i32 + cw).clamp(min_w, max_w.max(min_w));
    let height = ((ph * fit).round() as i32 + ch).clamp(min_h, max_h.max(min_h));
    PxRect { x: wa.x + ((wa.w - width) / 2).max(0), y: wa.y + ((wa.h - height) / 2).max(0), w: width, h: height }
}

// ── Record → payload ────────────────────────────────────────────────────────

/// The record, if it is a picture the editor can open.
fn picture(app: &AppHandle, id: i64) -> Result<ClipDetail, String> {
    let store = app.try_state::<Store>().ok_or("failed: store")?;
    let d = store.clip_detail(id)?;
    if !d.clip.kind.is_image() {
        return Err(format!("invalid: clip {id} is not a picture"));
    }
    Ok(d)
}

fn payload_of(app: &AppHandle, d: &ClipDetail, dirty_guard: bool) -> ShotEditorPayload {
    let id = d.clip.id;
    // The stored document (`shot_docs`); a row whose JSON no longer parses counts as none.
    let doc = app
        .try_state::<Store>()
        .and_then(|st| st.shot_doc(id).ok().flatten())
        .map(|d| d.doc)
        .filter(serde_json::Value::is_object);
    // The base picture's size: the document's (a crop makes the record smaller), else the record's.
    let dim = |key: &str, fallback: Option<i64>| {
        doc.as_ref().and_then(|v| v.get(key)).and_then(serde_json::Value::as_i64).or(fallback).unwrap_or(0)
    };
    ShotEditorPayload {
        clip_id: id,
        name: name_of(d, lang(app)),
        base_url: format!("http://liq.localhost/base/{id}?v={}", VERSION.fetch_add(1, Ordering::Relaxed) + 1),
        width: dim("w", d.clip.width),
        height: dim("h", d.clip.height),
        doc,
        dirty_guard,
    }
}

fn lang(app: &AppHandle) -> Lang {
    i18n_native::resolve(settings::get(app).general.language)
}

/// The record's name: the user's title, else the saved file's name, else "Screenshot 1920×1080".
fn name_of(d: &ClipDetail, lang: Lang) -> String {
    if let Some(t) = d.clip.title.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        return t.to_string();
    }
    let saved = d.clip.meta.shot.as_ref().and_then(|s| s.saved_path.as_deref());
    if let Some(stem) = saved.and_then(|p| std::path::Path::new(p).file_stem()).map(|s| s.to_string_lossy().into_owned()) {
        if !stem.trim().is_empty() {
            return stem;
        }
    }
    let base = untitled(lang, d.clip.kind);
    match (d.clip.width, d.clip.height) {
        (Some(w), Some(h)) => format!("{base} {w}×{h}"),
        _ => base.to_string(),
    }
}

/// "Name — Liqlip" (SPEC §9.5).
fn title(name: &str) -> String {
    format!("{name} — Liqlip")
}

/// "Screenshot" / "Image" (`i18n_native`, INT-5).
fn untitled(lang: Lang, kind: Kind) -> &'static str {
    i18n_native::tr(lang, if kind == Kind::Screenshot { Key::EditorUntitledShot } else { Key::EditorUntitledImage })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ClipMeta, OcrState, ShotMeta};

    fn detail(kind: Kind, title: Option<&str>, saved: Option<&str>, size: Option<(i64, i64)>) -> ClipDetail {
        ClipDetail {
            clip: Clip {
                id: 5,
                kind,
                title: title.map(str::to_string),
                preview: String::new(),
                app_id: None,
                created_at: 1,
                copied_at: 1,
                copy_count: 1,
                used_at: None,
                use_count: 0,
                favorite: false,
                folder_id: None,
                size: 0,
                width: size.map(|s| s.0),
                height: size.map(|s| s.1),
                char_count: None,
                file_count: None,
                thumb_url: None,
                formats: vec![],
                meta: ClipMeta {
                    shot: saved.map(|p| ShotMeta { saved_path: Some(p.into()), edited: false }),
                    ..Default::default()
                },
                ocr: OcrState::None,
            },
            text: None,
            word_count: None,
            line_count: None,
            files: None,
            image_url: None,
            ocr_text: None,
        }
    }

    #[test]
    fn the_name_is_title_then_saved_file_then_kind_and_size() {
        let d = detail(Kind::Screenshot, Some("  Bug report "), Some(r"C:\Pics\Liqlip 2026-11-26.png"), Some((800, 600)));
        assert_eq!(name_of(&d, Lang::En), "Bug report");
        let d = detail(Kind::Screenshot, Some(" "), Some(r"C:\Pics\Liqlip 2026-11-26 10.15.03.png"), Some((800, 600)));
        assert_eq!(name_of(&d, Lang::En), "Liqlip 2026-11-26 10.15.03");
        let d = detail(Kind::Screenshot, None, None, Some((1920, 1080)));
        assert_eq!(name_of(&d, Lang::Ru), "Снимок экрана 1920×1080");
        assert_eq!(name_of(&detail(Kind::Image, None, None, None), Lang::En), "Image");
        assert_eq!(title("Image 4×4"), "Image 4×4 — Liqlip");
    }

    /// A window showing record 1 (built, page loaded).
    fn showing(id: i64) -> EditorState {
        let mut st = EditorState::default();
        assert_eq!(st.open(id, false), Next::Build(1));
        st.built(1, true);
        st
    }

    #[test]
    fn done_closes_unless_it_saves_the_record_a_switch_leaves() {
        let mut st = showing(1);
        assert_eq!(st.after(ShotEditorAction::Done, 1), After::Close);
        assert_eq!(st.after(ShotEditorAction::Pin, 1), After::Pin);
        assert_eq!(st.after(ShotEditorAction::Copy, 1), After::Nothing);
        assert_eq!(st.after(ShotEditorAction::SaveAs, 1), After::Nothing);
        // Another record while 1 has unsaved changes: "Save" in the page's dialog sends `done` of 1
        // — the window stays for 2; "Done" of 2 itself would close it.
        assert_eq!(st.open(2, true), Next::Switch);
        assert_eq!((st.previous, st.current), (Some(1), Some(2)));
        assert_eq!(st.after(ShotEditorAction::Done, 1), After::Nothing);
        assert_eq!(st.after(ShotEditorAction::Done, 2), After::Close);
        // Accepted: nothing pends, "Done" closes again.
        assert_eq!(st.answer(true), None);
        assert_eq!(st.after(ShotEditorAction::Done, 2), After::Close);
    }

    #[test]
    fn a_switch_the_page_never_answered_does_not_keep_done_from_closing() {
        // `shot_edit(2)` right after the window was built, before the page subscribed: its
        // `shot:editor` is lost, the page loads `current` through `shot_editor_get`.
        let mut st = showing(1);
        assert_eq!(st.open(2, true), Next::Switch);
        assert_eq!(st.get(), Some(2));
        assert_eq!(st.previous, None);
        assert_eq!(st.after(ShotEditorAction::Done, 2), After::Close);
        // A later switch remembers the record the page really showed (2), not the stale 1.
        assert_eq!(st.open(3, true), Next::Switch);
        assert_eq!((st.previous, st.current), (Some(2), Some(3)));
        // "Cancel": back to 2.
        assert_eq!(st.answer(false), Some(2));
        assert_eq!((st.previous, st.current), (None, Some(2)));
        // The same record again only brings the window forward.
        assert_eq!(st.open(2, true), Next::Focus);
    }

    #[test]
    fn a_shot_edit_while_done_closes_the_window_builds_a_fresh_one() {
        let mut st = showing(1);
        st.close();
        assert_eq!((st.current, st.closing), (None, true));
        // The dying window is still in the window manager: wait, never switch it.
        assert_eq!(st.open(2, true), Next::Closing);
        assert_eq!(st.current, None);
        // Gone: a window of the next generation.
        assert_eq!(st.open(2, false), Next::Build(2));
        assert_eq!((st.current, st.building, st.closing), (Some(2), true, false));
        // A third `shot_edit` while it builds only moves `current`.
        assert_eq!(st.open(3, false), Next::Wait);
        // The OLD window's late `Destroyed` leaves the new window's state alone…
        st.destroyed(1);
        assert_eq!((st.current, st.building, st.gen), (Some(3), true, 2));
        st.built(2, true);
        assert!(!st.building);
        // …its own clears it, and the generation keeps growing.
        st.destroyed(2);
        assert_eq!(st, EditorState { gen: 2, ..EditorState::default() });
        assert_eq!(st.open(4, false), Next::Build(3));
        // A failed build leaves nothing current.
        st.built(3, false);
        assert_eq!((st.current, st.building), (None, false));
    }

    fn mon(w: i32, h: i32, scale: f64) -> Mon {
        Mon { name: None, rect: PxRect { x: 0, y: 0, w, h: h + 48 }, work: PxRect { x: 0, y: 0, w, h }, scale }
    }

    #[test]
    fn the_window_fits_the_picture_into_80_percent_of_the_work_area() {
        // A small picture: picture + chrome, but never below 640×420.
        let r = editor_rect(&mon(1920, 1032, 1.0), 200, 100);
        assert_eq!((r.w, r.h), (640, 420));
        assert_eq!((r.x, r.y), ((1920 - 640) / 2, (1032 - 420) / 2));
        // A medium picture: 1:1 plus the chrome.
        let r = editor_rect(&mon(1920, 1032, 1.0), 1000, 600);
        assert_eq!((r.w, r.h), (1048, 756));
        // A full-screen shot is shrunk to 80 % of the work area, aspect kept inside the chrome.
        let r = editor_rect(&mon(1920, 1032, 1.0), 1920, 1080);
        assert!(r.w <= 1536 && r.h <= 825, "{r:?}");
        let (pw, ph) = (f64::from(r.w - 48), f64::from(r.h - 156));
        assert!((pw / ph - 1920.0 / 1080.0).abs() < 0.01, "{r:?}");
        // At 150 % the minimum and the chrome scale with the monitor.
        let r = editor_rect(&mon(2880, 1560, 1.5), 10, 10);
        assert_eq!((r.w, r.h), (960, 630));
        // The minimum outgrows a small work area (1024×600 at 150 %): the title bar stays on screen.
        let small = Mon { name: None, rect: PxRect { x: 0, y: 0, w: 1024, h: 600 }, work: PxRect { x: 0, y: 0, w: 1024, h: 560 }, scale: 1.5 };
        let r = editor_rect(&small, 1920, 1080);
        assert_eq!((r.y, r.h), (0, 630));
        assert_eq!(r.x, (1024 - 960) / 2);
    }
}
