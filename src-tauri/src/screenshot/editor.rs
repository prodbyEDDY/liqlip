//! The editor window `editor` (owner: W6-B; ARCHITECTURE §18.2, §18.9; SPEC §9.5).
//!
//! - **One window, built on demand** (`windows.rs`): an ordinary window — system title bar,
//!   taskbar button, Alt+Tab — min 640×420, sized to fit the picture into 80 % of the work area
//!   of the cursor's monitor, centred there, focused. Title "Name — Liqlip" (follows the
//!   language). Closing destroys it (the page's `onCloseRequested` asks about unsaved changes
//!   first and then calls `destroy()`; `WindowEvent::Destroyed` clears [`EditorState`]).
//! - **A second `shot_edit`** while it is open keeps the window and switches the record: the
//!   new record becomes current at once and `shot:editor` carries it with `dirtyGuard: true`;
//!   the page loads it, or — with unsaved changes — asks and answers `shot_editor_switch`
//!   (`false` = stay: the previous record becomes current again). A page that switches
//!   without asking needs no answer.
//! - **The commit** (`shot_editor_commit`, body already split by `body.rs`) is [`commit`]:
//!   `done` → `store::shots::apply_edit` + clipboard (if `settings.screenshot.copy`) + rewrite of
//!   the saved file, then the window closes; `copy` → clipboard only; `save_as` → dialog;
//!   `pin` → [`super::pin::open`]. The store / clipboard / file primitives are W6-A's — reached
//!   through the [`seam`] functions below until INT-5 wires them (ARCHITECTURE §18.9).

use super::windows::{self as w, Mon, PxRect};
use crate::i18n_native::{self, Lang};
use crate::model::{Clip, ClipDetail, Kind, ShotEditorAction, ShotEditorCommit, ShotEditorPayload};
use crate::store::Store;
use crate::{events, logx, picker, settings};
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};

/// Minimum inner size, logical px (SPEC §9.5).
pub(crate) const MIN_SIZE: (f64, f64) = (640.0, 420.0);
/// Room around the picture inside the window, logical px: toolbar + properties bar on top,
/// canvas padding (W6-C's layout; only these numbers change if it differs).
const CHROME: (f64, f64) = (48.0, 136.0);
/// Share of the work area the window may take by default.
const FIT: f64 = 0.8;

/// `?v=` of the base picture URL: every payload asks for a fresh copy (`no-store` keeps the
/// disk cache out, this keeps the page's memory cache out after an edit).
static VERSION: AtomicU64 = AtomicU64::new(0);

/// What the editor window side remembers (`ScreenshotState.editor`).
#[derive(Default)]
pub(crate) struct EditorState {
    /// The record the window shows (or is switching to).
    current: Option<i64>,
    /// The record shown before a switch the page has not answered yet.
    previous: Option<i64>,
    /// The window is being built (a second `shot_edit` only moves `current`).
    building: bool,
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
    let existing = app.get_webview_window(events::EDITOR);
    enum Next {
        Wait,
        Focus,
        Switch,
        Build,
    }
    let next = with(app, |st| {
        if st.building {
            st.current = Some(id); // the page asks `shot_editor_get` once it has loaded
            return Next::Wait;
        }
        if existing.is_some() {
            if st.current == Some(id) && st.previous.is_none() {
                return Next::Focus;
            }
            if st.previous.is_none() {
                st.previous = st.current;
            }
            st.current = Some(id);
            return Next::Switch;
        }
        *st = EditorState { current: Some(id), previous: None, building: true };
        Next::Build
    })
    .ok_or("failed: screenshot state")?;
    match next {
        Next::Wait => Ok(()),
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
        Next::Build => {
            let built = build(app, &detail);
            with(app, |st| {
                st.building = false;
                if built.is_err() {
                    *st = EditorState::default();
                }
            });
            built
        }
    }
}

/// `shot_editor_get` — what the editor window shows, `None` when nothing is open.
pub(crate) fn payload(app: &AppHandle) -> Result<Option<ShotEditorPayload>, String> {
    let Some(id) = with(app, |st| st.current).flatten() else { return Ok(None) };
    let detail = picture(app, id)?;
    Ok(Some(payload_of(app, &detail, false)))
}

/// `shot_editor_switch { accept }` — the page's answer to a pending switch (`shot:editor`).
/// `false` makes the previous record current again. No pending switch → nothing to do.
pub(crate) fn switch(app: &AppHandle, accept: bool) -> Result<(), String> {
    let back = with(app, |st| match st.previous.take() {
        Some(prev) if !accept => {
            st.current = Some(prev);
            Some(prev)
        }
        _ => None,
    })
    .ok_or("failed: screenshot state")?;
    if let (Some(id), Some(win)) = (back, app.get_webview_window(events::EDITOR)) {
        if let Ok(d) = picture(app, id) {
            let _ = win.set_title(&title(&name_of(&d, lang(app))));
        }
    }
    Ok(())
}

/// The hub's hook after `commit::editor_commit` (W5-A wiring): the window side of [`commit`]
/// for the record the window shows. With [`commit`] wired instead, it is not needed.
pub(crate) fn after_commit(app: &AppHandle, action: ShotEditorAction) {
    let shown = with(app, |st| st.previous.or(st.current)).flatten();
    if let Some(id) = shown {
        after(app, action, id);
    }
}

/// `shot_editor_commit` — see the module docs. `rgba` = the flattened picture (the page's
/// export of the current document); `None` = the page sent no pixels: the base picture is used.
#[allow(dead_code)] // wired by the coordinator in `screenshot/mod.rs` (report W6-B, snippet 1)
pub(crate) async fn commit(app: &AppHandle, req: ShotEditorCommit, rgba: Option<&[u8]>) -> Result<Clip, String> {
    let id = req.clip_id;
    let detail = picture(app, id)?;
    let (png, w, h) = match rgba {
        Some(px) => (encode_png(req.width, req.height, px)?, req.width, req.height),
        None => {
            let png = super::frame::serve(app, &format!("/base/{id}"))
                .filter(|(mime, _)| mime == "image/png")
                .map(|(_, bytes)| bytes)
                .ok_or_else(|| format!("not_found: base picture of clip {id}"))?;
            let (w, h) = png_size(&png)?;
            (png, w, h)
        }
    };
    match req.action {
        ShotEditorAction::Done => {
            let clip = seam::apply_edit(app, id, &png, &req.doc, w, h)?;
            if settings::get(app).screenshot.copy {
                if let Err(e) = seam::copy_png(app, &png) {
                    logx::warn("screenshot", &format!("editor done: clipboard: {e}"));
                }
            }
            if let Err(e) = seam::rewrite_saved(app, id) {
                logx::warn("screenshot", &format!("editor done: saved file: {e}"));
            }
            after(app, ShotEditorAction::Done, id);
            Ok(clip)
        }
        ShotEditorAction::Copy => {
            seam::copy_png(app, &png)?;
            Ok(detail.clip)
        }
        ShotEditorAction::SaveAs => {
            let name = name_of(&detail, lang(app));
            seam::save_png_as(app, png, &name).await?;
            Ok(detail.clip)
        }
        ShotEditorAction::Pin => {
            after(app, ShotEditorAction::Pin, id);
            Ok(detail.clip)
        }
    }
}

// ── Window ──────────────────────────────────────────────────────────────────

/// What the window does after a successful commit of record `id`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum After {
    Nothing,
    Close,
    Pin,
}

/// `done` closes the window — unless a switch is pending: then the page saved before moving
/// on to the next record and the window stays. `pin` pins the record as it is in history.
fn after_effect(action: ShotEditorAction, switch_pending: bool) -> After {
    match action {
        ShotEditorAction::Done if !switch_pending => After::Close,
        ShotEditorAction::Pin => After::Pin,
        _ => After::Nothing,
    }
}

fn after(app: &AppHandle, action: ShotEditorAction, id: i64) {
    let pending = with(app, |st| st.previous.is_some()).unwrap_or(false);
    match after_effect(action, pending) {
        After::Close => {
            if let Some(win) = app.get_webview_window(events::EDITOR) {
                if let Err(e) = win.destroy() {
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

fn build(app: &AppHandle, detail: &ClipDetail) -> Result<(), String> {
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
        .build()
        .map_err(|e| format!("failed: {e}"))?;
    let app2 = app.clone();
    win.on_window_event(move |ev| {
        if let WindowEvent::Destroyed = ev {
            with(&app2, |st| *st = EditorState::default());
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
/// fit [`FIT`] of the work area, at least [`MIN_SIZE`], centred on the work area.
fn editor_rect(mon: &Mon, pic_w: i64, pic_h: i64) -> PxRect {
    let (s, wa) = (mon.scale, mon.work);
    let (cw, ch) = (w::px(CHROME.0, s), w::px(CHROME.1, s));
    let (min_w, min_h) = (w::px(MIN_SIZE.0, s), w::px(MIN_SIZE.1, s));
    let (max_w, max_h) = (((f64::from(wa.w) * FIT) as i32).max(min_w), ((f64::from(wa.h) * FIT) as i32).max(min_h));
    let (pw, ph) = (pic_w.max(1) as f64, pic_h.max(1) as f64);
    let fit = (f64::from((max_w - cw).max(1)) / pw).min(f64::from((max_h - ch).max(1)) / ph).min(1.0);
    let width = ((pw * fit).round() as i32 + cw).clamp(min_w, max_w.max(min_w));
    let height = ((ph * fit).round() as i32 + ch).clamp(min_h, max_h.max(min_h));
    PxRect { x: wa.x + (wa.w - width) / 2, y: wa.y + (wa.h - height) / 2, w: width, h: height }
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
    let doc = seam::doc(app, id);
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

/// Native strings of the editor window, kept here until `i18n_native.rs` (W6-A) has the keys
/// (report W6-B, snippet 3): `EditorUntitledShot`, `EditorUntitledImage`.
fn untitled(lang: Lang, kind: Kind) -> &'static str {
    match (lang, kind) {
        (Lang::En, Kind::Screenshot) => "Screenshot",
        (Lang::Ru, Kind::Screenshot) => "Снимок экрана",
        (Lang::En, _) => "Image",
        (Lang::Ru, _) => "Картинка",
    }
}

fn encode_png(w: u32, h: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    use image::ImageEncoder as _;
    // `write_image` panics on a wrong length (image 0.25 docs); `body.rs` checks it too.
    if w == 0 || h == 0 || rgba.len() as u64 != u64::from(w) * u64::from(h) * 4 {
        return Err(format!("invalid: {} bytes for {w}×{h} RGBA", rgba.len()));
    }
    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new(&mut out)
        .write_image(rgba, w, h, image::ExtendedColorType::Rgba8)
        .map_err(|e| format!("invalid: png: {e}"))?;
    Ok(out)
}

fn png_size(png: &[u8]) -> Result<(u32, u32), String> {
    image::ImageReader::with_format(std::io::Cursor::new(png), image::ImageFormat::Png)
        .into_dimensions()
        .map_err(|e| format!("invalid: png: {e}"))
}

/// W6-A's primitives (ARCHITECTURE §18.4–18.5) the editor commit calls. Each body is a stand-in
/// until INT-5 replaces it with the real call named in its doc (report W6-B, snippet 2).
#[allow(dead_code)] // reached through `commit`, wired at INT-5
mod seam {
    use crate::model::Clip;
    use serde_json::Value;
    use tauri::AppHandle;

    /// `store::shots::doc(id)` → the stored `AnnotDoc` (`None`: never annotated).
    pub(super) fn doc(_app: &AppHandle, _id: i64) -> Option<Value> {
        None
    }

    /// `store::shots::apply_edit(id, flattened_png, base_png: None, doc, w, h)` + `clips:changed
    /// {origin:'user', change:'updated'}`.
    pub(super) fn apply_edit(_app: &AppHandle, _id: i64, _png: &[u8], _doc: &Value, _w: u32, _h: u32) -> Result<Clip, String> {
        Err("unimplemented: store::shots::apply_edit (W6-A)".into())
    }

    /// `paste::copy_png(png)` — PNG + DIB in one clipboard transaction, own write suppressed.
    pub(super) fn copy_png(_app: &AppHandle, _png: &[u8]) -> Result<(), String> {
        Err("unimplemented: paste::copy_png (W6-A)".into())
    }

    /// `files` (W6-A): write the record's picture over its `saved_path` (format by extension);
    /// `Ok(None)` when it was never saved.
    pub(super) fn rewrite_saved(_app: &AppHandle, _id: i64) -> Result<Option<String>, String> {
        Ok(None)
    }

    /// `files` (W6-A): the "Save as…" dialog for these PNG bytes (start in the screenshot
    /// folder, suggested name) → the written path, `None` = cancelled.
    pub(super) async fn save_png_as(_app: &AppHandle, _png: Vec<u8>, _name: &str) -> Result<Option<String>, String> {
        Err("unimplemented: files::save_png_as (W6-A)".into())
    }
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

    #[test]
    fn done_closes_unless_a_switch_waits_and_pin_pins() {
        assert_eq!(after_effect(ShotEditorAction::Done, false), After::Close);
        assert_eq!(after_effect(ShotEditorAction::Done, true), After::Nothing);
        assert_eq!(after_effect(ShotEditorAction::Pin, false), After::Pin);
        assert_eq!(after_effect(ShotEditorAction::Copy, false), After::Nothing);
        assert_eq!(after_effect(ShotEditorAction::SaveAs, true), After::Nothing);
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
        assert_eq!((r.w, r.h), (1048, 736));
        // A full-screen shot is shrunk to 80 % of the work area, aspect kept inside the chrome.
        let r = editor_rect(&mon(1920, 1032, 1.0), 1920, 1080);
        assert!(r.w <= 1536 && r.h <= 825, "{r:?}");
        let (pw, ph) = (f64::from(r.w - 48), f64::from(r.h - 136));
        assert!((pw / ph - 1920.0 / 1080.0).abs() < 0.01, "{r:?}");
        // At 150 % the minimum and the chrome scale with the monitor.
        let r = editor_rect(&mon(2880, 1560, 1.5), 10, 10);
        assert_eq!((r.w, r.h), (960, 630));
    }

    #[test]
    fn png_round_trip() {
        let px = vec![255u8; 3 * 2 * 4];
        let png = encode_png(3, 2, &px).unwrap();
        assert_eq!(png_size(&png).unwrap(), (3, 2));
        assert!(encode_png(3, 3, &px).is_err(), "pixel count must match");
        assert!(png_size(b"not a png").is_err());
    }
}
