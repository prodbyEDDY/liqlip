//! Capture sessions (owner: W6-A; ARCHITECTURE §18.3).
//!
//! A trigger (hotkey, tray, `shot_capture`, `liqlip --capture`) runs on its own short thread:
//! delay countdown (`hud:show countdown`, one per second) → remember who has the focus and
//! which app it is (record source) → hide the shelf (`picker::hide_now`, no focus return), the
//! HUD toast and the QAO, `DwmFlush` → freeze (`grab.rs`). `screen` / `window` / `repeat` go
//! straight to the commit (`commit::direct`, the focus is not touched). `region` / `text` freeze
//! EVERY monitor, read the window tree of the cursor's monitor (`targets.rs`) and open a session:
//! `shot:session` to the pre-created `capture` window, then it is placed on that monitor
//! (position → size → position, as §7.4), shown, kept out of Alt+Tab (`WS_EX_TOOLWINDOW`, as the
//! shelf) and focused. While the overlay is in stage `select` a follow thread (50 ms) moves it to
//! the monitor under the cursor (new window tree, new `shot:session`; the frames stay those of
//! the trigger moment). The session ends with the commit (`commit.rs` takes it) or a cancel:
//! the overlay hides, the focus goes back to the remembered window (`bring_to_front`),
//! `shot:end`, the frames are freed, the QAO comes back.
//!
//! A trigger while one is prepared or a session is open is ignored (SPEC §9.1). Window state
//! goes through Tauri on the main thread (`run_on_main_thread` + a bounded wait); the session
//! lock is never held across such a wait (`on_capture_close` runs ON the main thread).

use super::frame::Frame;
use super::grab::{self, Monitor};
use super::{commit, qao, targets, ScreenshotState};
use crate::model::{HudKind, MonitorFrame, Rect, ScreenshotSettings, ShotMode, ShotSession, ShotSessionMode, ShotStage, WinRect};
use crate::store::SourceApp;
use crate::{events, logx, picker, settings};
use std::sync::{mpsc, Arc};
use std::time::Duration;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize};

/// Cursor-follow period while the overlay selects.
const FOLLOW_TICK: Duration = Duration::from_millis(50);
/// Longest wait for a window call on the main thread.
const MAIN_WAIT: Duration = Duration::from_millis(1500);
/// Longest wait for the focus to land back on the remembered window.
const FOCUS_WAIT: Duration = Duration::from_millis(500);
/// Longest delay `shot_capture` accepts (the tray offers 3 / 5 / 10 s).
pub(crate) const DELAY_MAX: u32 = 10;

/// The last committed region (`repeat`, `R` in the overlay) — process memory only.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LastRegion {
    /// Monitor device name.
    pub monitor: String,
    pub rect: Rect,
}

/// What a capture remembers from the trigger moment.
#[derive(Debug, Clone)]
pub(crate) struct Context {
    /// Window that gets the focus back when the overlay closes.
    pub restore: Option<isize>,
    /// Application that had the focus — the record's source.
    pub source: Option<SourceApp>,
    pub settings: ScreenshotSettings,
}

/// An open overlay session.
pub(crate) struct Active {
    pub id: i64,
    pub mode: ShotSessionMode,
    pub monitors: Vec<Monitor>,
    /// One frozen frame per monitor, same order.
    pub frames: Vec<Arc<Frame>>,
    /// Monitor the overlay is on.
    pub current: usize,
    pub windows: Vec<WinRect>,
    pub stage: ShotStage,
    pub ctx: Context,
}

/// `ScreenshotState.capture`: the trigger in preparation, the open session, the last region.
#[derive(Default)]
pub(crate) struct SessionState {
    next_id: i64,
    next_token: u64,
    /// Token of the trigger being prepared (between `begin` and its session / its freeze).
    busy: Option<u64>,
    active: Option<Active>,
    last_region: Option<LastRegion>,
}

impl SessionState {
    /// A trigger may start only when nothing is prepared or open; `Some(token)` marks it.
    pub(crate) fn begin(&mut self) -> Option<u64> {
        if self.busy.is_some() || self.active.is_some() {
            return None;
        }
        self.next_token += 1;
        self.busy = Some(self.next_token);
        self.busy
    }

    /// The trigger `token` is over without a session (frozen for a direct commit, or failed).
    pub(crate) fn release(&mut self, token: u64) {
        if self.busy == Some(token) {
            self.busy = None;
        }
    }

    /// Open the session prepared by `token`; returns its id.
    pub(crate) fn open(&mut self, token: u64, mut a: Active) -> i64 {
        self.release(token);
        self.next_id += 1;
        a.id = self.next_id;
        self.active = Some(a);
        self.next_id
    }

    pub(crate) fn set_stage(&mut self, id: i64, stage: ShotStage) -> bool {
        match self.active.as_mut() {
            Some(a) if a.id == id => {
                a.stage = stage;
                true
            }
            _ => false,
        }
    }

    /// End the session `id` and hand it over (frames included).
    pub(crate) fn take(&mut self, id: i64) -> Option<Active> {
        if self.active.as_ref().is_some_and(|a| a.id == id) {
            self.active.take()
        } else {
            None
        }
    }

    pub(crate) fn remember(&mut self, monitor: &str, rect: Rect) {
        self.last_region = Some(LastRegion { monitor: monitor.to_string(), rect });
    }

    pub(crate) fn last_region(&self) -> Option<&LastRegion> {
        self.last_region.as_ref()
    }

    /// The open session as the overlay sees it.
    pub(crate) fn session(&self) -> Option<ShotSession> {
        let a = self.active.as_ref()?;
        let m = a.monitors.get(a.current)?;
        Some(ShotSession {
            id: a.id,
            mode: a.mode,
            monitor: MonitorFrame {
                index: a.current as u32,
                id: m.id.clone(),
                x: m.x,
                y: m.y,
                width: m.width,
                height: m.height,
                scale: m.scale,
                frame_url: frame_url(a.id, a.current),
            },
            windows: a.windows.clone(),
            last_region: self.last_region.as_ref().filter(|l| l.monitor == m.id).map(|l| l.rect),
            quick: a.ctx.settings.quick,
            magnifier: a.ctx.settings.magnifier,
            dimensions: a.ctx.settings.dimensions,
        })
    }

    fn frame(&self, id: i64, monitor: usize) -> Option<Arc<Frame>> {
        self.active.as_ref().filter(|a| a.id == id)?.frames.get(monitor).cloned()
    }
}

/// `http://liq.localhost/shot/<session>/<index>.bmp` (§18.3).
pub(crate) fn frame_url(session: i64, index: usize) -> String {
    format!("http://liq.localhost/shot/{session}/{index}.bmp")
}

fn with_state<T>(app: &AppHandle, f: impl FnOnce(&mut SessionState) -> T) -> Option<T> {
    let st = app.try_state::<ScreenshotState>()?;
    let mut guard = st.capture.lock();
    Some(f(&mut guard))
}

/// Run `f` on the main thread and wait for it (bounded). Never call it with a lock held that
/// the main thread may want.
fn on_main<T: Send + 'static>(app: &AppHandle, f: impl FnOnce(&AppHandle) -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = mpsc::channel();
    let app2 = app.clone();
    if let Err(e) = app.run_on_main_thread(move || {
        let _ = tx.send(f(&app2));
    }) {
        logx::warn("screenshot", &format!("main thread: {e}"));
        return None;
    }
    rx.recv_timeout(MAIN_WAIT).ok()
}

// ── Commands' logic ─────────────────────────────────────────────────────────

/// `shot_capture { mode, delay? }` (also the hotkeys, the tray, `liqlip --capture`): returns at
/// once; the capture runs on its own thread. Ignored while a capture is prepared or open.
pub(crate) fn capture(app: &AppHandle, mode: ShotMode, delay: Option<u32>) -> Result<(), String> {
    let delay = delay.unwrap_or(0);
    if delay > DELAY_MAX {
        return Err(format!("invalid: delay {delay} s (at most {DELAY_MAX})"));
    }
    let Some(token) = with_state(app, SessionState::begin).ok_or("failed: screenshots are not ready")? else {
        logx::warn("screenshot", "trigger ignored: a capture is already open");
        return Ok(());
    };
    let app2 = app.clone();
    let spawned = std::thread::Builder::new().name("liqlip-shot".into()).spawn(move || run(&app2, mode, delay, token));
    if let Err(e) = spawned {
        with_state(app, |s| s.release(token));
        return Err(format!("failed: {e}"));
    }
    Ok(())
}

/// `shot_session` — the open session, `None` when there is none.
pub(crate) fn current(app: &AppHandle) -> Result<Option<ShotSession>, String> {
    Ok(with_state(app, |s| s.session()).flatten())
}

/// `shot_stage { sessionId, stage }`. A stale id is ignored (the overlay may report after the
/// session ended).
pub(crate) fn stage(app: &AppHandle, session_id: i64, stage: ShotStage) -> Result<(), String> {
    with_state(app, |s| s.set_stage(session_id, stage));
    Ok(())
}

/// `shot_cancel { sessionId }` (idempotent).
pub(crate) fn cancel(app: &AppHandle, session_id: i64) -> Result<(), String> {
    end(app, session_id);
    Ok(())
}

/// The `capture` window got `CloseRequested` (Alt+F4) — ON the main thread: end the session on
/// a short thread (ending waits for main-thread window calls). `lib.rs` hides the window.
pub(crate) fn on_capture_close(app: &AppHandle) {
    let Some(id) = with_state(app, |s| s.active.as_ref().map(|a| a.id)).flatten() else { return };
    let app2 = app.clone();
    let _ = std::thread::Builder::new().name("liqlip-shot-close".into()).spawn(move || {
        end(&app2, id);
    });
}

/// The frame a `/shot/…` URL names (`frame.rs`).
pub(crate) fn frame(app: &AppHandle, session_id: i64, monitor: usize) -> Option<Arc<Frame>> {
    with_state(app, |s| s.frame(session_id, monitor)).flatten()
}

/// For the commit: end the session `id` and hand it over.
pub(crate) fn take(app: &AppHandle, session_id: i64) -> Result<Active, String> {
    with_state(app, |s| s.take(session_id)).flatten().ok_or_else(|| format!("not_found: session {session_id}"))
}

/// Remember the committed region of an overlay shot (`repeat`).
pub(crate) fn remember(app: &AppHandle, monitor: &str, rect: Rect) {
    with_state(app, |s| s.remember(monitor, rect));
}

/// Close a taken session on screen: hide the overlay, give the focus back to the remembered
/// window (Ctrl+V goes there at once), `shot:end`.
pub(crate) fn close(app: &AppHandle, a: &Active) {
    hide_overlay(app);
    if let Some(h) = a.ctx.restore {
        give_focus(h);
    }
    events::shot_end(app, a.id);
}

/// Cancel: take the session, close it, free its frames, bring the QAO back.
fn end(app: &AppHandle, id: i64) -> bool {
    let Some(a) = with_state(app, |s| s.take(id)).flatten() else { return false };
    close(app, &a);
    drop(a);
    qao::restore_after_capture(app);
    true
}

// ── The trigger ─────────────────────────────────────────────────────────────

fn run(app: &AppHandle, mode: ShotMode, delay: u32, token: u64) {
    for left in (1..=delay).rev() {
        picker::hud::show(app, HudKind::Countdown, Some(left.to_string()));
        std::thread::sleep(Duration::from_secs(1));
    }
    if let Err(e) = trigger(app, mode, token) {
        logx::warn("screenshot", &format!("capture failed: {e}"));
        picker::hud::show(app, HudKind::ShotFailed, None);
        qao::restore_after_capture(app);
    }
    with_state(app, |s| s.release(token));
}

fn trigger(app: &AppHandle, mode: ShotMode, token: u64) -> Result<(), String> {
    // "Copy text from screen" without any OCR language pack: say so instead of an overlay
    // whose result could only fail (INT-5, HUD `text_unavailable`).
    if mode == ShotMode::Text && !crate::ocr::has_languages() {
        picker::hud::show(app, HudKind::TextUnavailable, None);
        return Ok(());
    }
    let settings = settings::get(app);
    // Who has the focus — before any window of ours moves.
    let (restore, source) = focus_context(app);
    // Our windows out of the picture (capture and QAO also carry WDA_EXCLUDEFROMCAPTURE).
    let hid_shelf = hide_shelf(app);
    let hid_hud = hide_hud(app);
    qao::hide_for_capture(app);
    if hid_shelf || hid_hud {
        grab::dwm_flush();
    }
    let monitors = grab::monitors();
    if monitors.is_empty() {
        return Err("failed: no monitor".into());
    }
    let cursor = grab::cursor_pos();
    let at_cursor = cursor.and_then(|p| grab::monitor_at(&monitors, p)).unwrap_or(0);
    let ctx = Context { restore, source, settings: settings.screenshot.clone() };
    let direct = |m: &Monitor, crop: Option<(u32, u32, u32, u32)>| -> Result<(), String> {
        let frame = grab::grab(m)?;
        with_state(app, |s| s.release(token)); // the next trigger may start while this one commits
        let crop = crop.unwrap_or((0, 0, frame.width(), frame.height()));
        commit::direct(app, &ctx, m, &frame, crop).map(|_| ())
    };
    match mode {
        ShotMode::Screen => direct(&monitors[at_cursor], None),
        ShotMode::Window => {
            let target = restore.and_then(grab::window_bounds);
            match target.and_then(|b| window_crop(&monitors, b)) {
                Some((i, crop)) => direct(&monitors[i], Some(crop)),
                // No usable window (the desktop has the focus): the monitor under the cursor.
                None => direct(&monitors[at_cursor], None),
            }
        }
        ShotMode::Repeat => {
            let last = with_state(app, |s| s.last_region().cloned()).flatten();
            let found = last.and_then(|l| {
                let i = monitors.iter().position(|m| m.id == l.monitor)?;
                let crop = super::frame::rect_to_crop(&l.rect, monitors[i].width as u32, monitors[i].height as u32)?;
                Some((i, crop))
            });
            match found {
                Some((i, crop)) => direct(&monitors[i], Some(crop)),
                // Nothing to repeat (or its monitor is gone): pick a region.
                None => overlay(app, token, ShotSessionMode::Region, monitors, at_cursor, cursor, ctx),
            }
        }
        ShotMode::Region => overlay(app, token, ShotSessionMode::Region, monitors, at_cursor, cursor, ctx),
        ShotMode::Text => overlay(app, token, ShotSessionMode::Text, monitors, at_cursor, cursor, ctx),
    }
}

/// The monitor holding a window's centre and the window's bounds as a crop of that monitor's
/// frame (a window across monitors is cut at the edge — §9.10).
pub(crate) fn window_crop(monitors: &[Monitor], (l, t, r, b): (i32, i32, i32, i32)) -> Option<(usize, (u32, u32, u32, u32))> {
    let centre = (l + (r - l) / 2, t + (b - t) / 2);
    let i = grab::monitor_at(monitors, centre)?;
    let m = &monitors[i];
    let crop = super::frame::clamp_crop(l - m.x, t - m.y, u32::try_from(r - l).ok()?, u32::try_from(b - t).ok()?, m.width as u32, m.height as u32)?;
    Some((i, crop))
}

#[allow(clippy::too_many_arguments)]
fn overlay(
    app: &AppHandle,
    token: u64,
    mode: ShotSessionMode,
    monitors: Vec<Monitor>,
    current: usize,
    cursor: Option<(i32, i32)>,
    ctx: Context,
) -> Result<(), String> {
    let frames = monitors.iter().map(|m| grab::grab(m).map(Arc::new)).collect::<Result<Vec<_>, _>>()?;
    let windows = targets::enumerate(&monitors[current], cursor);
    let place_on = monitors[current].clone();
    let active = Active { id: 0, mode, monitors, frames, current, windows, stage: ShotStage::Select, ctx };
    let (id, payload) = with_state(app, |s| {
        let id = s.open(token, active);
        (id, s.session())
    })
    .ok_or("failed: screenshots are not ready")?;
    // The overlay starts loading the frame while it is still hidden.
    if let Some(p) = &payload {
        events::shot_session(app, p);
    }
    if !show_overlay(app, &place_on) {
        end(app, id);
        return Err("failed: the capture window could not be shown".into());
    }
    let app2 = app.clone();
    let _ = std::thread::Builder::new().name("liqlip-shot-follow".into()).spawn(move || follow(&app2, id));
    Ok(())
}

/// While the session is open and selecting, move the overlay to the monitor under the cursor.
fn follow(app: &AppHandle, id: i64) {
    loop {
        std::thread::sleep(FOLLOW_TICK);
        let Some(snapshot) = with_state(app, |s| match s.active.as_ref() {
            Some(a) if a.id == id => Some((a.stage, a.current, a.monitors.clone())),
            _ => None,
        })
        .flatten() else {
            return;
        };
        let (stage, current, monitors) = snapshot;
        if stage != ShotStage::Select {
            continue;
        }
        let Some(cursor) = grab::cursor_pos() else { continue };
        let Some(next) = grab::monitor_at(&monitors, cursor).filter(|&i| i != current) else { continue };
        let windows = targets::enumerate(&monitors[next], Some(cursor));
        let payload = with_state(app, |s| {
            let a = s.active.as_mut().filter(|a| a.id == id && a.stage == ShotStage::Select)?;
            a.current = next;
            a.windows = windows;
            s.session()
        })
        .flatten();
        let Some(payload) = payload else { continue };
        move_overlay(app, &monitors[next]);
        events::shot_session(app, &payload);
    }
}

// ── Focus, our windows ──────────────────────────────────────────────────────

/// The window to give the focus back to and the app to credit. The shelf in front → its own
/// remembered paste target (the user's context did not change — also when the trigger is the
/// shelf's own actions menu: the shelf closes right before the command and hands the focus back
/// asynchronously). No foreground window at all (mid-switch) → wait up to [`FG_SETTLE`] for
/// one, then fall back to the shelf's target too. Otherwise the foreground window (any: the
/// desktop, the settings window); the source app only for an application window
/// (`paste::focus::target_for` — never ours, the desktop or the taskbar). Unlike
/// `capture_target` this never changes the shelf's paste target.
#[cfg(windows)]
fn focus_context(app: &AppHandle) -> (Option<isize>, Option<SourceApp>) {
    use crate::paste::focus;
    let mut fg = focus::foreground();
    let settle_until = std::time::Instant::now() + FG_SETTLE;
    while fg == 0 && std::time::Instant::now() < settle_until {
        std::thread::sleep(Duration::from_millis(10));
        fg = focus::foreground();
    }
    let shelf_target = fg == 0 || focus::picker_hwnd(app) == Some(fg);
    let target = if shelf_target { focus::current() } else { focus::target_for(app, fg) };
    let restore = if shelf_target { target.as_ref().map(|t| t.hwnd) } else { Some(fg) };
    let source = target.map(|t| SourceApp { exe: t.info.exe, name: t.info.name });
    (restore, source)
}

/// How long a trigger waits for a foreground window when there is none (a focus hand-off in
/// flight).
#[cfg(windows)]
const FG_SETTLE: Duration = Duration::from_millis(200);

#[cfg(not(windows))]
fn focus_context(_app: &AppHandle) -> (Option<isize>, Option<SourceApp>) {
    (None, None)
}

#[cfg(windows)]
fn give_focus(h: isize) {
    if !crate::paste::focus::bring_to_front(h, FOCUS_WAIT) {
        logx::warn("screenshot", "focus not given back (the window is gone, minimized or refused)");
    }
}

#[cfg(not(windows))]
fn give_focus(_h: isize) {}

/// Hide an open shelf at once, without giving focus anywhere (the overlay takes it next).
fn hide_shelf(app: &AppHandle) -> bool {
    let h = picker::hwnds().map(|h| h.picker).unwrap_or(0);
    if h == 0 || !win::is_visible(h) {
        return false;
    }
    picker::hide_now(app);
    true
}

/// A toast at the cursor (e.g. the last countdown second) must not be frozen into the shot.
fn hide_hud(app: &AppHandle) -> bool {
    let h = picker::hwnds().map(|h| h.hud).unwrap_or(0);
    if h == 0 || !win::is_visible(h) {
        return false;
    }
    on_main(app, move |_| win::hide(h)).is_some()
}

/// Position → size → position (physical): moving onto a monitor with another scale makes tao
/// apply `WM_DPICHANGED`'s suggested rect during the first move; the size and the second move
/// then set the exact geometry (as `picker::apply_geometry`).
fn place(w: &tauri::WebviewWindow, m: &Monitor) {
    let _ = w.set_position(PhysicalPosition::new(m.x, m.y));
    let _ = w.set_size(PhysicalSize::new(m.width.max(1) as u32, m.height.max(1) as u32));
    let _ = w.set_position(PhysicalPosition::new(m.x, m.y));
}

fn capture_hwnd(w: &tauri::WebviewWindow) -> isize {
    w.hwnd().map(|h| h.0 as isize).unwrap_or(0)
}

/// Place, show and focus the overlay on `m`. `false` when the window is missing or would not
/// show.
fn show_overlay(app: &AppHandle, m: &Monitor) -> bool {
    let m = m.clone();
    on_main(app, move |app| {
        let Some(w) = app.get_webview_window(events::CAPTURE) else { return false };
        let h = capture_hwnd(&w);
        place(&w, &m);
        // tao rewrites GWL_EXSTYLE on show: WS_EX_TOOLWINDOW before and after (as the shelf).
        win::apply_toolwindow(h);
        let shown = w.show();
        win::apply_toolwindow(h);
        place(&w, &m); // a DPI change deferred to the first show is corrected
        if let Err(e) = w.set_focus() {
            logx::warn("screenshot", &format!("overlay focus: {e}"));
        }
        if let Err(e) = &shown {
            logx::warn("screenshot", &format!("overlay show: {e}"));
        }
        shown.is_ok()
    })
    .unwrap_or(false)
}

fn move_overlay(app: &AppHandle, m: &Monitor) {
    let m = m.clone();
    on_main(app, move |app| {
        if let Some(w) = app.get_webview_window(events::CAPTURE) {
            place(&w, &m);
        }
    });
}

fn hide_overlay(app: &AppHandle) {
    on_main(app, |app| {
        if let Some(w) = app.get_webview_window(events::CAPTURE) {
            let h = capture_hwnd(&w);
            win::apply_toolwindow(h);
            if let Err(e) = w.hide() {
                logx::warn("screenshot", &format!("overlay hide: {e}"));
            }
            win::apply_toolwindow(h);
        }
    });
}

// ── `liqlip --capture [mode]` ───────────────────────────────────────────────

/// The capture mode a command line asks for: `--capture` (region), `--capture <mode>` or
/// `--capture=<mode>`; `None` without `--capture` or with an unknown mode.
pub(crate) fn capture_arg(args: &[String]) -> Option<ShotMode> {
    let mode = |s: &str| match s.to_ascii_lowercase().as_str() {
        "region" => Some(ShotMode::Region),
        "screen" => Some(ShotMode::Screen),
        "window" => Some(ShotMode::Window),
        "repeat" => Some(ShotMode::Repeat),
        "text" => Some(ShotMode::Text),
        _ => None,
    };
    let i = args.iter().position(|a| a == "--capture" || a.starts_with("--capture="))?;
    if let Some(v) = args[i].strip_prefix("--capture=") {
        return mode(v);
    }
    match args.get(i + 1).filter(|a| !a.starts_with("--")) {
        Some(v) => mode(v),
        None => Some(ShotMode::Region),
    }
}

/// A second launch's arguments (single-instance, `lib.rs`): `true` when they asked for a capture
/// (and it was started); the caller summons the shelf otherwise.
pub(crate) fn handle_args(app: &AppHandle, args: &[String]) -> bool {
    let Some(mode) = capture_arg(args) else { return false };
    if let Err(e) = capture(app, mode, None) {
        logx::warn("screenshot", &format!("--capture: {e}"));
    }
    true
}

// ── Win32 helpers (thread-safe calls) ───────────────────────────────────────

#[cfg(windows)]
mod win {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, IsWindowVisible, SetWindowLongPtrW, SetWindowPos, ShowWindow, GWL_EXSTYLE, SWP_FRAMECHANGED,
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SW_HIDE, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
    };

    fn hwnd(h: isize) -> HWND {
        HWND(h as *mut core::ffi::c_void)
    }

    /// https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-iswindowvisible
    pub fn is_visible(h: isize) -> bool {
        // SAFETY: any handle value is accepted.
        unsafe { IsWindowVisible(hwnd(h)).as_bool() }
    }

    /// https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-showwindow
    pub fn hide(h: isize) {
        // SAFETY: own window, main thread.
        let _ = unsafe { ShowWindow(hwnd(h), SW_HIDE) };
    }

    /// Out of Alt+Tab: `WS_EX_TOOLWINDOW` on, `WS_EX_APPWINDOW` off, flushed with
    /// `SWP_FRAMECHANGED` (same helper as the shelf, `picker/win.rs`, which is private there).
    /// https://learn.microsoft.com/windows/win32/winmsg/extended-window-styles
    pub fn apply_toolwindow(h: isize) {
        if h == 0 {
            return;
        }
        // SAFETY: own window; style read-modify-write.
        unsafe {
            let ex = GetWindowLongPtrW(hwnd(h), GWL_EXSTYLE);
            let want = (ex | WS_EX_TOOLWINDOW.0 as isize) & !(WS_EX_APPWINDOW.0 as isize);
            if want != ex {
                SetWindowLongPtrW(hwnd(h), GWL_EXSTYLE, want);
                let _ = SetWindowPos(hwnd(h), None, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED);
            }
        }
    }
}

#[cfg(not(windows))]
mod win {
    pub fn is_visible(_h: isize) -> bool {
        false
    }
    pub fn hide(_h: isize) {}
    pub fn apply_toolwindow(_h: isize) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(id: &str, x: i32, w: i32) -> Monitor {
        Monitor { id: id.into(), x, y: 0, width: w, height: 1000, scale: 1.25 }
    }

    fn active(monitors: Vec<Monitor>) -> Active {
        let frames = monitors.iter().map(|m| Arc::new(Frame::new(m.width as u32, m.height as u32).unwrap())).collect();
        Active {
            id: 0,
            mode: ShotSessionMode::Region,
            monitors,
            frames,
            current: 0,
            windows: vec![WinRect { x: 1, y: 2, w: 30, h: 40, children: vec![] }],
            stage: ShotStage::Select,
            ctx: Context { restore: None, source: None, settings: ScreenshotSettings::default() },
        }
    }

    #[test]
    fn one_capture_at_a_time() {
        let mut s = SessionState::default();
        let t1 = s.begin().unwrap();
        assert_eq!(s.begin(), None, "a second trigger while one is prepared is ignored");
        let id = s.open(t1, active(vec![mon("A", 0, 100)]));
        assert_eq!(id, 1);
        assert_eq!(s.begin(), None, "… and while a session is open");
        assert!(s.take(2).is_none(), "a stale id takes nothing");
        assert!(s.take(id).is_some());
        assert!(s.session().is_none());
        let t2 = s.begin().unwrap();
        assert_ne!(t1, t2);
        s.release(t1); // a late release of the old trigger does not free the new one
        assert_eq!(s.begin(), None);
        s.release(t2);
        assert!(s.begin().is_some());
    }

    #[test]
    fn the_session_payload_follows_the_monitor() {
        let mut s = SessionState::default();
        let t = s.begin().unwrap();
        let id = s.open(t, active(vec![mon("A", -100, 100), mon("B", 0, 200)]));
        s.remember("B", Rect { x: 5.0, y: 6.0, w: 7.0, h: 8.0 });
        let p = s.session().unwrap();
        assert_eq!((p.id, p.mode, p.monitor.index, p.monitor.id.as_str()), (id, ShotSessionMode::Region, 0, "A"));
        assert_eq!(p.monitor.frame_url, "http://liq.localhost/shot/1/0.bmp");
        assert_eq!((p.monitor.x, p.monitor.width, p.monitor.scale), (-100, 100, 1.25));
        assert_eq!(p.last_region, None, "the last region belongs to another monitor");
        assert_eq!(p.windows.len(), 1);
        assert!(!p.quick && p.magnifier && p.dimensions, "from settings.screenshot");
        s.active.as_mut().unwrap().current = 1;
        let p = s.session().unwrap();
        assert_eq!((p.monitor.index, p.monitor.id.as_str()), (1, "B"));
        assert_eq!(p.last_region, Some(Rect { x: 5.0, y: 6.0, w: 7.0, h: 8.0 }));
        assert_eq!(p.monitor.frame_url, "http://liq.localhost/shot/1/1.bmp");
        // Frames are served only for the open session.
        assert!(s.frame(id, 1).is_some());
        assert!(s.frame(id, 2).is_none() && s.frame(id + 1, 0).is_none());
    }

    #[test]
    fn stages_only_change_the_open_session() {
        let mut s = SessionState::default();
        assert!(!s.set_stage(1, ShotStage::Frame));
        let t = s.begin().unwrap();
        let id = s.open(t, active(vec![mon("A", 0, 100)]));
        assert!(s.set_stage(id, ShotStage::Frame));
        assert!(!s.set_stage(id + 1, ShotStage::Select));
        assert_eq!(s.active.as_ref().unwrap().stage, ShotStage::Frame);
    }

    #[test]
    fn a_window_is_cropped_on_the_monitor_of_its_centre() {
        let ms = vec![mon("L", -1000, 1000), mon("R", 0, 2000)];
        // Mostly on R: centre at x = 150.
        assert_eq!(window_crop(&ms, (-100, 10, 400, 510)), Some((1, (0, 10, 400, 500))));
        // Mostly on L: cut at L's right edge.
        assert_eq!(window_crop(&ms, (-600, 0, 100, 300)), Some((0, (400, 0, 600, 300))));
        // Off every monitor's area (nearest monitor, nothing of the window inside it).
        assert_eq!(window_crop(&ms, (5000, 0, 5100, 100)), None);
    }

    #[test]
    fn capture_arguments() {
        let a = |v: &[&str]| capture_arg(&v.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(a(&["liqlip.exe"]), None);
        assert_eq!(a(&["liqlip.exe", "--capture"]), Some(ShotMode::Region));
        assert_eq!(a(&["liqlip.exe", "--capture", "screen"]), Some(ShotMode::Screen));
        assert_eq!(a(&["liqlip.exe", "--capture", "TEXT"]), Some(ShotMode::Text));
        assert_eq!(a(&["liqlip.exe", "--capture=window"]), Some(ShotMode::Window));
        assert_eq!(a(&["liqlip.exe", "--capture", "--minimized"]), Some(ShotMode::Region));
        assert_eq!(a(&["liqlip.exe", "--capture", "video"]), None);
        assert_eq!(a(&["liqlip.exe", "--capture=repeat"]), Some(ShotMode::Repeat));
    }
}
