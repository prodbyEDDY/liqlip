//! Pinned pictures — windows `pin-<n>` (owner: W6-B; ARCHITECTURE §18.2, §18.8; SPEC §9.5).
//!
//! - **Built on demand** from the `async` command (`shot_pin`, or the editor / commit calling
//!   [`open`]; on the main thread the work moves to a short thread — `windows.rs`). Transparent,
//!   frameless, always-on-top, skip-taskbar, out of Alt+Tab, focusable (Esc, the wheel, the menu)
//!   but shown WITHOUT activation (Win32 `SWP_NOACTIVATE`; a click activates it), no shadow of
//!   its own (the page draws one inside [`BLEED`]). Pins are **not** excluded from captures:
//!   they are pinned to be shown (SPEC §9.5).
//! - **Size**: the picture at 100 % = 1 picture px per physical px of the monitor (a screenshot
//!   pinned on the monitor it came from looks exactly as it did), shrunk to fit 80 % of that
//!   monitor, plus [`BLEED`] logical px of transparent margin on each side. Centred on the
//!   cursor, kept inside the work area. `shot_pin_resize { scale }` is ABSOLUTE (W6-E's page,
//!   `pinMath.ts`): physical screen px per picture px, 1 = 100 %, 0.1–4 — the window becomes
//!   `round(picture × scale)` physical px + the bleed, around the cursor.
//! - **Live record**: `clips:changed` reaches every `pin-*` window (`events.rs`); the page asks
//!   [`payload`] again, which reads the record's current size and version (`?v=`) — an edit
//!   in the editor shows up in the pin; a deleted record answers `null` (the page's
//!   "unavailable" notice).
//! - **Labels** `pin-<n>` with a counter that never repeats in a process (a window still being
//!   destroyed never clashes with a new one). Up to [`MAX_PINS`]; the next one closes the oldest.
//!   Closing = destroying; `WindowEvent::Destroyed` drops the entry.

use super::windows::{self as w, Mon, PxRect};
use crate::model::PinPayload;
use crate::store::Store;
use crate::{events, logx};
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};

/// At most this many pins at once (SPEC §9.5).
pub(crate) const MAX_PINS: usize = 6;
/// Transparent margin around the picture for the page's shadow, logical px (W6-E draws in it:
/// `src/windows/pin/pinMath.ts::PIN_BLEED`).
pub(crate) const BLEED: f64 = 24.0;
/// `shot_pin_resize` limits (10 %–400 %).
pub(crate) const SCALE_MIN: f64 = 0.1;
pub(crate) const SCALE_MAX: f64 = 4.0;
/// Share of the monitor a new pin may take.
const FIT: f64 = 0.8;
/// Largest window side Win32 handles comfortably (coordinates are 16-bit in places).
const MAX_SIDE: i32 = 32_767;

struct Pin {
    label: String,
    clip_id: i64,
    /// The picture in px.
    w: i64,
    h: i64,
}

/// Open pins (`ScreenshotState.pins`), oldest first, and the label counter.
#[derive(Default)]
pub(crate) struct PinState {
    seq: u64,
    pins: Vec<Pin>,
}

fn with<T>(app: &AppHandle, f: impl FnOnce(&mut PinState) -> T) -> Option<T> {
    let st = app.try_state::<super::ScreenshotState>()?;
    let mut guard = st.pins.lock();
    Some(f(&mut guard))
}

// ── Commands ────────────────────────────────────────────────────────────────

/// `shot_pin { id }` — pin a record on the screen (also the editor's and the commit's `pin`).
pub(crate) fn open(app: &AppHandle, id: i64) -> Result<(), String> {
    if w::on_main_thread() {
        let app = app.clone();
        std::thread::Builder::new()
            .name("liqlip-pin-open".into())
            .spawn(move || {
                if let Err(e) = open_now(&app, id) {
                    logx::warn("screenshot", &format!("pin open: {e}"));
                }
            })
            .map_err(|e| format!("failed: {e}"))?;
        return Ok(());
    }
    open_now(app, id)
}

fn open_now(app: &AppHandle, id: i64) -> Result<(), String> {
    w::may_build()?;
    let (pw, ph) = picture_size(app, id)?;
    // Register before building: the page asks `shot_pin_get` as soon as it loads.
    let (label, evicted) = with(app, |st| {
        st.seq += 1;
        let label = format!("{}{}", events::PIN_PREFIX, st.seq);
        let evicted = (st.pins.len() >= MAX_PINS).then(|| st.pins.remove(0).label);
        st.pins.push(Pin { label: label.clone(), clip_id: id, w: pw, h: ph });
        (label, evicted)
    })
    .ok_or("failed: screenshot state")?;
    if let Some(old) = evicted.and_then(|l| app.get_webview_window(&l)) {
        if let Err(e) = old.destroy() {
            logx::warn("screenshot", &format!("pin evict: {e}"));
        }
    }
    let cursor = w::cursor_pos();
    let mons = w::monitors(app);
    let mon = w::pick_monitor(&mons, None, cursor).cloned();
    let rect = mon.as_ref().map(|m| pin_rect(m, cursor, pw, ph));
    let scale = mon.as_ref().map_or(1.0, |m| m.scale);
    let (lw, lh) = rect.map_or((400.0, 300.0), |r| (f64::from(r.w) / scale, f64::from(r.h) / scale));
    let built = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("pin.html".into()))
        .title("Liqlip Pin")
        .transparent(true)
        .decorations(false)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .focusable(true)
        .focused(false)
        .visible(false)
        .inner_size(lw, lh)
        .theme(w::native_theme(app))
        .additional_browser_args(w::BROWSER_ARGS)
        .on_navigation(w::navigation_guard(app))
        // W8-A: wry's drop target took any file (a QAO card released over the pin closed as
        // "dropped"); without it the page refuses drops itself (`refuseDrop`, §18.2).
        .disable_drag_drop_handler()
        .build();
    let win = match built {
        Ok(win) => win,
        Err(e) => {
            forget(app, &label);
            return Err(format!("failed: {e}"));
        }
    };
    let (app2, label2) = (app.clone(), label.clone());
    win.on_window_event(move |ev| {
        if let WindowEvent::Destroyed = ev {
            forget(&app2, &label2);
        }
    });
    let app3 = app.clone();
    let posted = app.run_on_main_thread(move || {
        let Some(win) = app3.get_webview_window(&label) else { return };
        let Some(h) = w::hwnd(&win) else { return };
        if let Some(r) = rect {
            w::place(&win, r);
        }
        w::apply_toolwindow(h);
        w::show_no_activate(h);
        if let Some(r) = rect {
            // A DPI change deferred to the first show: re-assert the exact geometry.
            w::place(&win, r);
        }
    });
    posted.map_err(|e| format!("failed: {e}"))
}

/// `shot_pin_get` — the record of the calling `pin-<n>` window as it is NOW (`None`: unknown
/// window, or its record was deleted). The size is re-read (an edit may have cropped it) and
/// kept for [`resize`]; `?v=` is the record's version, so an edited picture is fetched anew.
pub(crate) fn payload(app: &AppHandle, win: &WebviewWindow) -> Result<Option<PinPayload>, String> {
    let label = win.label();
    let found = with(app, |st| st.pins.iter().find(|p| p.label == label).map(|p| p.clip_id)).flatten();
    let Some(clip_id) = found else { return Ok(None) };
    let store = app.try_state::<Store>().ok_or("failed: store")?;
    let src = match store.shot_source(clip_id) {
        Ok(src) => src,
        Err(e) if e.starts_with("not_found") => return Ok(None),
        Err(e) => return Err(e),
    };
    let (width, height) = (src.width.max(1), src.height.max(1));
    with(app, |st| {
        if let Some(p) = st.pins.iter_mut().find(|p| p.label == label) {
            (p.w, p.h) = (width, height);
        }
    });
    Ok(Some(PinPayload { clip_id, image_url: format!("http://liq.localhost/media/{clip_id}?v={}", src.version), width, height }))
}

/// `shot_pin_resize { scale }` — the picture at `scale` × 100 % (clamped to 10 %–400 %),
/// keeping the point under the cursor in place.
pub(crate) fn resize(app: &AppHandle, win: &WebviewWindow, scale: f64) -> Result<(), String> {
    if !scale.is_finite() || scale <= 0.0 {
        return Err(format!("invalid: scale {scale}"));
    }
    let label = win.label().to_string();
    let (pw, ph) = with(app, |st| st.pins.iter().find(|p| p.label == label).map(|p| (p.w, p.h)))
        .flatten()
        .ok_or_else(|| format!("not_found: pin window {label}"))?;
    let (pos, size) = (win.outer_position().map_err(|e| format!("failed: {e}"))?, win.outer_size().map_err(|e| format!("failed: {e}"))?);
    let old = PxRect { x: pos.x, y: pos.y, w: size.width as i32, h: size.height as i32 };
    let mon_scale = win.scale_factor().unwrap_or(1.0);
    let bleed = w::px(BLEED, mon_scale);
    let cursor = w::cursor_pos().unwrap_or((old.x + old.w / 2, old.y + old.h / 2));
    let r = resize_rect(old, bleed, (pw, ph), scale.clamp(SCALE_MIN, SCALE_MAX), cursor);
    let _ = win.set_position(PhysicalPosition::new(r.x, r.y));
    let _ = win.set_size(PhysicalSize::new(r.w as u32, r.h as u32));
    let _ = win.set_position(PhysicalPosition::new(r.x, r.y));
    Ok(())
}

/// `shot_pin_close` — close (destroy) the calling pin window.
pub(crate) fn close(_app: &AppHandle, win: &WebviewWindow) -> Result<(), String> {
    if !win.label().starts_with(events::PIN_PREFIX) {
        return Err(format!("invalid: {} is not a pin window", win.label()));
    }
    win.destroy().map_err(|e| format!("failed: {e}"))
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn forget(app: &AppHandle, label: &str) {
    with(app, |st| st.pins.retain(|p| p.label != label));
}

/// The picture size of a record that can be pinned (`image` / `screenshot`).
fn picture_size(app: &AppHandle, id: i64) -> Result<(i64, i64), String> {
    let store = app.try_state::<Store>().ok_or("failed: store")?;
    let d = store.clip_detail(id)?;
    if !d.clip.kind.is_image() {
        return Err(format!("invalid: clip {id} is not a picture"));
    }
    match (d.clip.width, d.clip.height) {
        (Some(w), Some(h)) if w > 0 && h > 0 => Ok((w, h)),
        _ => Err(format!("invalid: clip {id} has no picture size")),
    }
}

/// A new pin: the picture at 100 % shrunk to [`FIT`] of the monitor, + bleed, centred on the
/// cursor (the work area's centre without one), then pushed inside the work area.
fn pin_rect(mon: &Mon, cursor: Option<(i32, i32)>, pic_w: i64, pic_h: i64) -> PxRect {
    let bleed = w::px(BLEED, mon.scale);
    let (pw, ph) = (pic_w.max(1) as f64, pic_h.max(1) as f64);
    let fit = (f64::from(mon.rect.w) * FIT / pw).min(f64::from(mon.rect.h) * FIT / ph).min(1.0);
    let ww = ((pw * fit).round() as i32).max(1) + 2 * bleed;
    let wh = ((ph * fit).round() as i32).max(1) + 2 * bleed;
    let wa = mon.work;
    let (cx, cy) = cursor.unwrap_or((wa.x + wa.w / 2, wa.y + wa.h / 2));
    let clamp = |v: i32, lo: i32, len: i32, size: i32| if size >= len { lo } else { v.clamp(lo, lo + len - size) };
    PxRect { x: clamp(cx - ww / 2, wa.x, wa.w, ww), y: clamp(cy - wh / 2, wa.y, wa.h, wh), w: ww, h: wh }
}

/// Resize a pin window to `scale` × the picture, keeping the picture point under `cursor` where
/// it is (a cursor outside the picture keeps the nearest edge).
fn resize_rect(old: PxRect, bleed: i32, (pic_w, pic_h): (i64, i64), scale: f64, cursor: (i32, i32)) -> PxRect {
    let (ox, oy) = (old.x + bleed, old.y + bleed);
    let (ow, oh) = ((old.w - 2 * bleed).max(1), (old.h - 2 * bleed).max(1));
    let fx = (f64::from(cursor.0 - ox) / f64::from(ow)).clamp(0.0, 1.0);
    let fy = (f64::from(cursor.1 - oy) / f64::from(oh)).clamp(0.0, 1.0);
    let side = |pic: i64| ((pic.max(1) as f64 * scale).round() as i32).clamp(1, MAX_SIDE - 2 * bleed);
    let (nw, nh) = (side(pic_w), side(pic_h));
    let nx = cursor.0 - (fx * f64::from(nw)).round() as i32;
    let ny = cursor.1 - (fy * f64::from(nh)).round() as i32;
    PxRect { x: nx - bleed, y: ny - bleed, w: nw + 2 * bleed, h: nh + 2 * bleed }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(x: i32, w: i32, h: i32, scale: f64) -> Mon {
        Mon { name: None, rect: PxRect { x, y: 0, w, h }, work: PxRect { x, y: 0, w, h: h - 40 }, scale }
    }

    #[test]
    fn a_pin_is_the_picture_at_100_percent_centred_on_the_cursor() {
        let m = mon(0, 1920, 1080, 1.0);
        let r = pin_rect(&m, Some((960, 500)), 400, 300);
        assert_eq!(r, PxRect { x: 960 - 224, y: 500 - 174, w: 448, h: 348 });
        // Near the corner it is pushed back inside the work area (above the taskbar).
        let r = pin_rect(&m, Some((1915, 1075)), 400, 300);
        assert_eq!((r.x + r.w, r.y + r.h), (1920, 1040));
        // At 150 % the bleed is in logical px, the picture stays 1:1.
        let r = pin_rect(&mon(1920, 2880, 1620, 1.5), Some((3000, 800)), 400, 300);
        assert_eq!((r.w, r.h), (472, 372));
    }

    #[test]
    fn a_big_picture_takes_at_most_80_percent_of_the_monitor() {
        let m = mon(0, 1920, 1080, 1.0);
        let r = pin_rect(&m, None, 3840, 2160);
        assert_eq!((r.w - 48, r.h - 48), (1536, 864));
        // Taller than the work area (a huge taskbar): aligned to its top edge.
        let short = Mon { work: PxRect { x: 0, y: 0, w: 800, h: 400 }, ..mon(0, 800, 600, 1.0) };
        let r = pin_rect(&short, Some((10, 10)), 100, 4000);
        assert_eq!((r.x, r.y, r.h), (0, 0, 480 + 48));
    }

    #[test]
    fn resizing_keeps_the_point_under_the_cursor() {
        // 400×300 picture shown at 100 % at (100,100) with a 12 px bleed; cursor on its centre.
        let old = PxRect { x: 88, y: 88, w: 424, h: 324 };
        let r = resize_rect(old, 12, (400, 300), 2.0, (300, 250));
        assert_eq!(r, PxRect { x: 100 - 12, y: 100 - 12, w: 824, h: 624 }.translated(-200, -150));
        // Cursor on the picture's top-left corner: that corner stays.
        let r = resize_rect(old, 12, (400, 300), 0.5, (100, 100));
        assert_eq!((r.x + 12, r.y + 12, r.w - 24, r.h - 24), (100, 100, 200, 150));
        // A cursor outside the picture keeps the nearest edge (right/bottom here).
        let r = resize_rect(old, 12, (400, 300), 0.5, (900, 900));
        assert_eq!((r.x + 12 + r.w - 24, r.y + 12 + r.h - 24), (900, 900));
        // Huge scale on a huge picture never exceeds the Win32 limit.
        let r = resize_rect(old, 12, (20_000, 20_000), 4.0, (300, 250));
        assert_eq!((r.w, r.h), (MAX_SIDE, MAX_SIDE));
    }

    impl PxRect {
        fn translated(self, dx: i32, dy: i32) -> PxRect {
            PxRect { x: self.x + dx, y: self.y + dy, ..self }
        }
    }
}
