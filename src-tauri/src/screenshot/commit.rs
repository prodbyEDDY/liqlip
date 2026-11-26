//! Commit of a shot (owner: W6-A; ARCHITECTURE §18.4) and the store step of an editor commit
//! that `editor::commit` orchestrates (§18.9: [`apply_edit`]).
//!
//! `screenshot/mod.rs` has already split the raw body (`body.rs`): the JSON header arrives
//! parsed, the pixels as a slice borrowed from the IPC body — `None` when the webview sent
//! none (no annotations → the crop of the frozen frame is the picture: the fast path).
//!
//! Order, tuned for "Ctrl+V right away": (1) the session is taken, the overlay hides and the
//! focus goes back (`session::close`); (2) PNG encoding — on this thread, never the UI thread
//! (an `async` command runs on the runtime's workers; a hotkey capture on its own thread);
//! (3) the clipboard FIRST (`paste::copy_png`: PNG + DIB in one transaction, our own write is
//! not captured again); (4) the store (`insert_screenshot`, kind `screenshot`, source = the app
//! that had the focus at the trigger, thumbnail, base picture + document when annotated);
//! (5) the file (`files.rs`); (6) the QAO / editor / pin / nothing (`afterCapture`,
//! [`after_step`]). A failure after (3) never undoes the steps before it; a failed save is
//! `error: 'save_failed'` in `shot:added` (the QAO is then shown whatever `afterCapture` says,
//! so the error is seen); the editor or a pin without a record, or a window that would not
//! open, falls back to the QAO card.
//! `storage.types.screenshot` off or a picture above 64 MP: no record, the QAO gets a temporary
//! file (`%TEMP%\Liqlip\shots\`).

use super::frame::{rect_to_crop, Frame};
use super::grab::Monitor;
use super::qao;
use super::session::{self, Context};
use super::files;
use crate::content::{self, IMAGE_MAX_PIXELS};
use crate::model::{
    ChangeOrigin, Clip, ClipsChange, ClipsChanged, HudKind, ScreenshotSettings, ShotAction, ShotAddedError, ShotAdded,
    ShotAfterCapture, ShotCommit, ShotCommitResult, ShotSessionMode,
};
use crate::store::shots::{NewShot, ShotEdit};
use crate::store::Store;
use crate::{events, logx, paste, picker};
use base64::Engine;
use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageBuffer, ImageEncoder, Rgba};
use std::borrow::Cow;
use tauri::{AppHandle, Manager};

/// Longest side of a record thumbnail (= `store::media::THUMB_MAX`).
const THUMB_MAX: u32 = 360;

/// Where the file of a commit goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Save {
    No,
    /// The screenshot folder, name from the template.
    Folder,
    /// "Save as…" dialog.
    Dialog,
}

/// What follows the commit on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum After {
    Nothing,
    /// The QAO card.
    Overlay,
    Editor,
    Pin,
}

/// The steps of one commit action (`action: 'text'` has none — it recognises text instead).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Plan {
    pub copy: bool,
    pub save: Save,
    pub after: After,
}

/// §18.4: `done` follows the settings; `copy` = clipboard (+ history); `save` = folder
/// (+ history); `save_as` = dialog (+ history); `pin` = history + pin. History itself is
/// decided by `storage.types.screenshot` and the 64 MP limit, not by the action.
pub(crate) fn plan(action: ShotAction, s: &ScreenshotSettings) -> Option<Plan> {
    let p = |copy, save, after| Some(Plan { copy, save, after });
    match action {
        ShotAction::Done => p(
            s.copy,
            if s.save { Save::Folder } else { Save::No },
            match s.after_capture {
                ShotAfterCapture::Overlay => After::Overlay,
                ShotAfterCapture::Editor => After::Editor,
                ShotAfterCapture::None => After::Nothing,
            },
        ),
        ShotAction::Copy => p(true, Save::No, After::Nothing),
        ShotAction::Save => p(false, Save::Folder, After::Nothing),
        ShotAction::SaveAs => p(false, Save::Dialog, After::Nothing),
        ShotAction::Pin => p(false, Save::No, After::Pin),
        ShotAction::Text => None,
    }
}

/// Whether a picture may go to the history (§2.3, §9.6).
pub(crate) fn keeps_history(kept_type: bool, width: u32, height: u32) -> bool {
    kept_type && u64::from(width) * u64::from(height) <= IMAGE_MAX_PIXELS
}

/// RGBA8 → PNG (the image crate's default: fast deflate, adaptive filter).
pub(crate) fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    PngEncoder::new(&mut out)
        .write_image(rgba, width, height, ExtendedColorType::Rgba8)
        .map_err(|e| format!("failed: png: {e}"))?;
    Ok(out)
}

/// Thumbnail PNG ≤ [`THUMB_MAX`] px on the long side; a small picture is its own thumbnail.
fn thumbnail(width: u32, height: u32, rgba: &[u8], png: &[u8]) -> Option<Vec<u8>> {
    let long = width.max(height);
    if long <= THUMB_MAX {
        return Some(png.to_vec());
    }
    let src: ImageBuffer<Rgba<u8>, &[u8]> = ImageBuffer::from_raw(width, height, rgba)?;
    let scale = f64::from(THUMB_MAX) / f64::from(long);
    let tw = ((f64::from(width) * scale).round() as u32).clamp(1, THUMB_MAX);
    let th = ((f64::from(height) * scale).round() as u32).clamp(1, THUMB_MAX);
    let small = image::imageops::thumbnail(&src, tw, th);
    encode_png(tw, th, small.as_raw()).ok()
}

/// The picture of a commit: the webview's flattened RGBA when it sent one of the crop's size,
/// with the crop itself as the base picture; else the crop. Pixels of another size are still
/// taken as the picture (that is what the user saw), but then neither base nor document is kept
/// — they would not line up.
struct Picture<'a> {
    width: u32,
    height: u32,
    rgba: Cow<'a, [u8]>,
    base: Option<Vec<u8>>,
    doc: Option<serde_json::Value>,
}

fn picture<'a>(frame: &Frame, crop: (u32, u32, u32, u32), sent: Option<(&'a [u8], u32, u32)>, doc: Option<serde_json::Value>) -> Picture<'a> {
    let (x, y, w, h) = crop;
    let doc = doc.filter(serde_json::Value::is_object);
    match sent {
        Some((px, sw, sh)) if (sw, sh) == (w, h) => {
            Picture { width: w, height: h, rgba: Cow::Borrowed(px), base: Some(frame.crop_rgba(x, y, w, h)), doc }
        }
        Some((px, sw, sh)) => {
            logx::warn("screenshot", &format!("commit pixels {sw}×{sh} ≠ crop {w}×{h}: document not kept"));
            Picture { width: sw, height: sh, rgba: Cow::Borrowed(px), base: None, doc: None }
        }
        None => Picture { width: w, height: h, rgba: Cow::Owned(frame.crop_rgba(x, y, w, h)), base: None, doc },
    }
}

/// `shot_commit` (raw body §18.4).
pub(crate) fn commit(app: &AppHandle, req: ShotCommit, rgba: Option<&[u8]>) -> Result<ShotCommitResult, String> {
    let active = session::take(app, req.session_id)?;
    // (1) Hide the overlay and give the focus back FIRST — also when the request turns out bad
    // (it must never stay on screen behind an early return).
    session::close(app, &active);
    let result = (|| {
        let monitor = active.monitors.get(active.current).ok_or("failed: session without a monitor")?;
        let frame = active.frames.get(active.current).ok_or("failed: session without a frame")?;
        let crop = rect_to_crop(&req.rect, frame.width(), frame.height()).ok_or("invalid: the region is outside the screen")?;
        let action = if active.mode == ShotSessionMode::Text { ShotAction::Text } else { req.action };
        if action != ShotAction::Text {
            session::remember(app, &monitor.id, req.rect);
        }
        let sent = rgba.map(|px| (px, req.width, req.height));
        finish(app, &active.ctx, monitor, frame, crop, action, sent, req.doc.clone())
    })();
    drop(active); // the frozen frames go now
    qao::restore_after_capture(app);
    result
}

/// A capture without overlay (`screen` / `window` / `repeat`): straight to the steps, the focus
/// is not touched.
pub(crate) fn direct(app: &AppHandle, ctx: &Context, monitor: &Monitor, frame: &Frame, crop: (u32, u32, u32, u32)) -> Result<ShotCommitResult, String> {
    let result = finish(app, ctx, monitor, frame, crop, ShotAction::Done, None, None);
    qao::restore_after_capture(app);
    result
}

#[allow(clippy::too_many_arguments)]
fn finish(
    app: &AppHandle,
    ctx: &Context,
    monitor: &Monitor,
    frame: &Frame,
    crop: (u32, u32, u32, u32),
    action: ShotAction,
    sent: Option<(&[u8], u32, u32)>,
    doc: Option<serde_json::Value>,
) -> Result<ShotCommitResult, String> {
    let pic = picture(frame, crop, sent, doc);
    let Some(plan) = plan(action, &ctx.settings) else { return recognize(app, &pic) };
    let png = encode_png(pic.width, pic.height, &pic.rgba)?;
    // (3) Clipboard first.
    if plan.copy {
        if let Err(e) = paste::copy_png(png.clone(), Some((pic.width, pic.height, pic.rgba.as_ref()))) {
            logx::warn("screenshot", &format!("clipboard: {e}"));
        }
    }
    // (4) History.
    let settings = crate::settings::get(app);
    let mut clip: Option<Clip> = None;
    if keeps_history(*settings.storage.types.get(crate::model::Kind::Screenshot), pic.width, pic.height) {
        match store_shot(app, ctx, monitor, &pic, &png) {
            Ok(c) => clip = Some(c),
            Err(e) => logx::warn("screenshot", &format!("history: {e}")),
        }
    }
    // (5) File.
    let mut saved_path: Option<String> = None;
    let mut error: Option<ShotAddedError> = None;
    let target = match plan.save {
        Save::No => Ok(None),
        Save::Folder => files::save_new(&ctx.settings, ctx.source.as_ref().map(|s| s.name.as_str()), crate::store::system_now(), &pic.rgba, pic.width, pic.height, &png).map(Some),
        Save::Dialog => files::save_new_as(app, &ctx.settings, ctx.source.as_ref().map(|s| s.name.as_str()), &pic.rgba, pic.width, pic.height, &png),
    };
    match target {
        Ok(Some(path)) => {
            if let Some(id) = clip.as_ref().map(|c| c.id) {
                let store = app.state::<Store>();
                if let Err(e) = store.set_saved_path(id, Some(&path)) {
                    logx::warn("screenshot", &format!("saved path: {e}"));
                }
                // The card shows "Show in folder" from `meta.shot.savedPath`.
                clip = store.shot_clip(id).ok().or(clip.take());
            }
            saved_path = Some(path);
        }
        Ok(None) => {}
        Err(e) => {
            logx::warn("screenshot", &format!("save: {e}"));
            error = Some(ShotAddedError::SaveFailed);
        }
    }
    // (6) What follows on screen.
    let after = after_step(plan.after, clip.is_some(), error.is_some());
    let opened = match (after, &clip) {
        (After::Editor, Some(c)) => super::editor::open(app, c.id).map_err(|e| format!("editor: {e}")),
        (After::Pin, Some(c)) => super::pin::open(app, c.id).map_err(|e| format!("pin: {e}")),
        _ => Ok(()),
    };
    if let Err(e) = &opened {
        logx::warn("screenshot", e);
    }
    // The QAO — also when the editor / pin window could not be opened: the shot stays in reach.
    if after == After::Overlay || opened.is_err() {
        let temp_path = match &clip {
            Some(_) => None,
            None => files::write_temp(&png).map_err(|e| logx::warn("screenshot", &format!("temp file: {e}"))).ok(),
        };
        let payload = ShotAdded { clip: clip.clone(), saved_path: saved_path.clone(), temp_path, monitor: monitor.id.clone(), error };
        qao::show_added(app, payload);
    }
    Ok(ShotCommitResult { clip_id: clip.map(|c| c.id), saved_path, text: None })
}

/// What actually follows a commit on screen (W7-R1). The editor and a pin work on a record —
/// without one (`storage.types.screenshot` off, a picture above 64 MP) the QAO card shows the
/// shot instead (copy / save as from its temp file), so "Pin" or "then: editor" never closes
/// the overlay on nothing (SPEC §9.0.3 "nothing is lost"). A failed save is always shown on
/// the QAO (SPEC §9.9: the toast "Could not save · Change folder"), also with
/// `afterCapture: none` / `editor` — the card's own "Edit" opens the editor.
pub(crate) fn after_step(planned: After, has_record: bool, save_failed: bool) -> After {
    match planned {
        _ if save_failed => After::Overlay,
        After::Editor | After::Pin if !has_record => After::Overlay,
        other => other,
    }
}

/// Step (4): the record, its change event, the source app's icon.
fn store_shot(app: &AppHandle, ctx: &Context, monitor: &Monitor, pic: &Picture<'_>, png: &[u8]) -> Result<Clip, String> {
    let store = app.state::<Store>();
    let base_png = match &pic.base {
        Some(b) => Some(encode_png(pic.width, pic.height, b)?),
        None => None,
    };
    let shot = NewShot {
        png: png.to_vec(),
        width: pic.width,
        height: pic.height,
        hash: content::image_hash(pic.width, pic.height, &pic.rgba),
        thumb_png: thumbnail(pic.width, pic.height, &pic.rgba, png),
        base_png,
        doc: pic.doc.clone(),
        unit: monitor.scale,
        saved_path: None,
        source: ctx.source.clone(),
        copied_at: crate::store::system_now(),
    };
    let out = store.insert_screenshot(shot)?;
    let change = if out.created { ClipsChange::Added } else { ClipsChange::Updated };
    events::clips_changed(app, &ClipsChanged { origin: ChangeOrigin::Capture, change, ids: Some(vec![out.id]) });
    if let (true, Some(app_id), Some(src)) = (out.app_icon_missing, out.app_id, ctx.source.as_ref()) {
        store_app_icon(&store, app_id, &src.exe);
    }
    store.shot_clip(out.id)
}

/// The icon of the source app from the capture module's cache (a miss extracts it in the
/// background; the next shot or copy from that app stores it).
fn store_app_icon(store: &Store, app_id: i64, exe: &str) {
    let Some(url) = crate::capture::app_icon_data_url(exe) else { return };
    let Some(b64) = url.strip_prefix("data:image/png;base64,") else { return };
    match base64::engine::general_purpose::STANDARD.decode(b64) {
        Ok(png) => {
            if let Err(e) = store.set_app_icon(app_id, &png) {
                logx::warn("screenshot", &format!("app icon {app_id}: {e}"));
            }
        }
        Err(e) => logx::warn("screenshot", &format!("app icon {app_id}: {e}")),
    }
}

/// `action: 'text'`: OCR of the region → the clipboard, HUD `text_copied` / `text_empty`; no
/// record, no file. No OCR language → HUD `text_unavailable` (the overlay is already gone, so
/// the HUD is where it can be seen) and the command's error; other failures are the error.
fn recognize(app: &AppHandle, pic: &Picture<'_>) -> Result<ShotCommitResult, String> {
    let png = encode_png(pic.width, pic.height, &pic.rgba)?;
    let text = crate::ocr::recognize_png(app, png).inspect_err(|e| {
        if e == crate::ocr::NO_LANGUAGE {
            picker::hud::show(app, HudKind::TextUnavailable, None);
        }
    })?;
    let text = text.trim().to_string();
    if text.is_empty() {
        picker::hud::show(app, HudKind::TextEmpty, None);
        return Ok(ShotCommitResult { clip_id: None, saved_path: None, text: Some(String::new()) });
    }
    paste::copy_text(&text)?;
    picker::hud::show(app, HudKind::TextCopied, None);
    Ok(ShotCommitResult { clip_id: None, saved_path: None, text: Some(text) })
}

/// The editor's "Done" (`editor::commit`): apply an edit to a live picture record
/// — PNG of `rgba`, pixel hash, thumbnail → `store::shots::apply_edit` (the first edit of an
/// image keeps its picture as the base) → `clips:changed { origin: 'user', change: 'updated' }`.
/// Returns the updated record and the PNG (for `paste::copy_png` / `files::overwrite_saved`).
/// Blocking — call from a worker.
pub(crate) fn apply_edit(app: &AppHandle, id: i64, rgba: &[u8], width: u32, height: u32, doc: serde_json::Value) -> Result<(Clip, Vec<u8>), String> {
    if width == 0 || height == 0 || rgba.len() as u64 != u64::from(width) * u64::from(height) * 4 {
        return Err("invalid: picture size".into());
    }
    let png = encode_png(width, height, rgba)?;
    let edit = ShotEdit {
        png: png.clone(),
        width,
        height,
        hash: content::image_hash(width, height, rgba),
        thumb_png: thumbnail(width, height, rgba, &png),
        base_png: None,
        doc,
    };
    let clip = app.state::<Store>().apply_edit(id, edit)?;
    events::clips_changed(app, &ClipsChanged { origin: ChangeOrigin::User, change: ClipsChange::Updated, ids: Some(vec![id]) });
    Ok((clip, png))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(copy: bool, save: bool, after: ShotAfterCapture) -> ScreenshotSettings {
        ScreenshotSettings { copy, save, after_capture: after, ..ScreenshotSettings::default() }
    }

    #[test]
    fn done_follows_the_settings_other_actions_do_not() {
        let s = ScreenshotSettings::default(); // copy, save, QAO
        assert_eq!(plan(ShotAction::Done, &s), Some(Plan { copy: true, save: Save::Folder, after: After::Overlay }));
        let quiet = settings(false, false, ShotAfterCapture::None);
        assert_eq!(plan(ShotAction::Done, &quiet), Some(Plan { copy: false, save: Save::No, after: After::Nothing }));
        let editor = settings(true, false, ShotAfterCapture::Editor);
        assert_eq!(plan(ShotAction::Done, &editor).unwrap().after, After::Editor);
        for s in [&s, &quiet, &editor] {
            assert_eq!(plan(ShotAction::Copy, s), Some(Plan { copy: true, save: Save::No, after: After::Nothing }));
            assert_eq!(plan(ShotAction::Save, s), Some(Plan { copy: false, save: Save::Folder, after: After::Nothing }));
            assert_eq!(plan(ShotAction::SaveAs, s), Some(Plan { copy: false, save: Save::Dialog, after: After::Nothing }));
            assert_eq!(plan(ShotAction::Pin, s), Some(Plan { copy: false, save: Save::No, after: After::Pin }));
            assert_eq!(plan(ShotAction::Text, s), None);
        }
    }

    /// W7-R1: "Pin" / "then: editor" without a record, or a failed save, end on the QAO card.
    #[test]
    fn what_follows_never_drops_the_shot() {
        for after in [After::Nothing, After::Overlay, After::Editor, After::Pin] {
            assert_eq!(after_step(after, true, false), after, "with a record the plan stands");
            assert_eq!(after_step(after, true, true), After::Overlay, "a failed save is shown");
            assert_eq!(after_step(after, false, true), After::Overlay);
        }
        assert_eq!(after_step(After::Pin, false, false), After::Overlay, "nothing to pin → the card");
        assert_eq!(after_step(After::Editor, false, false), After::Overlay, "nothing to edit → the card");
        assert_eq!(after_step(After::Nothing, false, false), After::Nothing, "copy / save actions show nothing");
        assert_eq!(after_step(After::Overlay, false, false), After::Overlay);
    }

    #[test]
    fn history_needs_the_type_and_at_most_64_megapixels() {
        assert!(keeps_history(true, 7680, 4320)); // 8K: 33 MP
        assert!(keeps_history(true, 8000, 8000)); // exactly 64 MP
        assert!(!keeps_history(true, 8001, 8000));
        assert!(!keeps_history(false, 10, 10));
    }

    fn frame_4x3() -> Frame {
        let mut f = Frame::new(4, 3).unwrap();
        for (i, px) in f.pixels_mut().chunks_exact_mut(4).enumerate() {
            px.copy_from_slice(&[i as u8, 0, 0, 255]);
        }
        f
    }

    #[test]
    fn the_picture_is_the_crop_or_the_flattened_pixels() {
        let f = frame_4x3();
        let doc = serde_json::json!({ "v": 1, "objects": [{ "id": "a" }] });
        // Fast path: the crop, the document kept as sent.
        let p = picture(&f, (1, 1, 2, 1), None, Some(doc.clone()));
        assert_eq!((p.width, p.height, p.base.is_none()), (2, 1, true));
        assert_eq!(&p.rgba[..], &[0, 0, 5, 255, 0, 0, 6, 255]);
        assert_eq!(p.doc, Some(doc.clone()));
        // Annotated: the sent pixels are the picture, the crop becomes the base.
        let flat = [9u8; 8];
        let p = picture(&f, (1, 1, 2, 1), Some((&flat, 2, 1)), Some(doc.clone()));
        assert_eq!(&p.rgba[..], &flat);
        assert_eq!(p.base.as_deref(), Some(&[0u8, 0, 5, 255, 0, 0, 6, 255][..]));
        assert!(p.doc.is_some());
        // Pixels of another size: still the picture, but no base and no document.
        let other = [7u8; 12];
        let p = picture(&f, (1, 1, 2, 1), Some((&other, 3, 1)), Some(doc));
        assert_eq!((p.width, p.height, p.base, p.doc), (3, 1, None, None));
        // A document that is not an object is dropped.
        assert_eq!(picture(&f, (0, 0, 1, 1), None, Some(serde_json::json!([1]))).doc, None);
    }

    #[test]
    fn thumbnails_fit_and_small_pictures_are_their_own() {
        let px = vec![200u8; 800 * 400 * 4];
        let png = encode_png(800, 400, &px).unwrap();
        let t = thumbnail(800, 400, &px, &png).unwrap();
        let d = image::load_from_memory(&t).unwrap();
        assert_eq!((d.width(), d.height()), (360, 180));
        let small = encode_png(10, 10, &vec![1u8; 400]).unwrap();
        assert_eq!(thumbnail(10, 10, &vec![1u8; 400], &small), Some(small));
    }
}
