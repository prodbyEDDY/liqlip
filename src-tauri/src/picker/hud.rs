//! HUD toast at the cursor (ARCHITECTURE §7 "HUD", SPEC §1) — owner: **W1-D**.
//!
//! The `hud` window is pre-created with `focusable(false)` (tao → `WS_EX_NOACTIVATE`) and
//! click-through. It is shown ONLY through Win32 `SetWindowPos(SWP_SHOWWINDOW |
//! SWP_NOACTIVATE)` and hidden with `ShowWindow(SW_HIDE)` — never through tao's `show()`,
//! whose `ShowWindow(SW_SHOW)` activates ("Activates the window and displays it",
//! https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-showwindow) and would take
//! focus from the window the user is typing in. tao keeps believing the HUD is hidden, which
//! is consistent: no tao flag call is made on it after `lib.rs` built it.

use super::geometry::{self, Rect};
use super::win;
use crate::events;
use crate::model::{HudKind, HudShow};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};

/// Visible time. The frontend starts its exit transition 200 ms before (src/windows/hud).
const HUD_MS: u64 = 1600;
/// Native hide a little after the frontend's exit has finished.
const HIDE_AFTER: Duration = Duration::from_millis(HUD_MS + 60);

static GEN: AtomicU64 = AtomicU64::new(0);

/// Position the `hud` window at the cursor (its monitor's work area), show it WITHOUT
/// activation, emit `hud:show { kind, text }`, auto-hide after 1.6 s (a new show extends).
/// Callable from any thread; never blocks.
pub fn show(app: &AppHandle, kind: HudKind, text: Option<String>) {
    let gen = GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let app2 = app.clone();
    if let Err(e) = app.run_on_main_thread(move || show_on_main(&app2, kind, text)) {
        crate::logx::warn("hud", &format!("show: {e}"));
        return;
    }
    let app3 = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(HIDE_AFTER);
        if GEN.load(Ordering::SeqCst) == gen {
            let app4 = app3.clone();
            let _ = app3.run_on_main_thread(move || {
                // A show queued meanwhile wins.
                if GEN.load(Ordering::SeqCst) == gen {
                    if let Some(h) = hud_hwnd(&app4) {
                        win::hide(h);
                    }
                }
            });
        }
    });
}

fn hud_hwnd(app: &AppHandle) -> Option<win::Hwnd> {
    super::hwnds().map(|h| h.hud).filter(|&h| h != 0).or_else(|| {
        app.get_webview_window(events::HUD).and_then(|w| w.hwnd().ok()).map(|h| h.0 as win::Hwnd)
    })
}

fn show_on_main(app: &AppHandle, kind: HudKind, text: Option<String>) {
    let Some(w) = app.get_webview_window(events::HUD) else { return };
    let Some(h) = hud_hwnd(app) else { return };
    let rect = place(&w);
    // Content first: the webview renders the new toast while the window appears.
    events::hud_show(app, &HudShow { kind, text });
    apply(&w, &rect);
    win::apply_toolwindow(h);
    win::show_no_activate(h);
    // A DPI change deferred to the first show of a window moved while hidden: re-assert.
    apply(&w, &rect);
}

/// Where the toast goes: under the cursor on the cursor's monitor (primary as a fallback).
fn place(w: &WebviewWindow) -> Rect {
    let cursor = win::cursor_pos();
    let mon = cursor
        .and_then(|(x, y)| w.monitor_from_point(f64::from(x), f64::from(y)).ok().flatten())
        .or_else(|| w.primary_monitor().ok().flatten());
    let (work, scale) = match &mon {
        Some(m) => {
            let wa = m.work_area();
            (Rect::new(wa.position.x, wa.position.y, wa.size.width as i32, wa.size.height as i32), m.scale_factor())
        }
        None => (Rect::new(0, 0, 1920, 1080), 1.0),
    };
    let c = cursor.unwrap_or(((work.left + work.right) / 2, (work.top + work.bottom) / 2));
    geometry::hud_rect(c, work, scale)
}

/// Same order as the shelf: position → size → position (tao handles WM_DPICHANGED on the
/// first move; these calls do not touch tao's window flags, so the HUD stays "hidden" to tao).
fn apply(w: &WebviewWindow, r: &Rect) {
    let _ = w.set_position(PhysicalPosition::new(r.left, r.top));
    let _ = w.set_size(PhysicalSize::new(r.width().max(1) as u32, r.height().max(1) as u32));
    let _ = w.set_position(PhysicalPosition::new(r.left, r.top));
}
